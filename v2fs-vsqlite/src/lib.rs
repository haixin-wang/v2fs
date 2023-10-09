#![cfg_attr(not(test), warn(clippy::unwrap_used))]
#[macro_use]
extern crate tracing;
extern crate lru;

pub mod cache;
pub mod digest;
pub mod merkle_cb_tree;
pub mod query;
pub mod script;
pub mod simple_vcache;
pub mod utils;
pub mod vbf;
pub mod verify;
pub mod version_cache;
pub mod vfs;

use crate::merkle_cb_tree::NodeId;
use anyhow::{Context, Result};
use cache::Cache;
use digest::{Digest, Digestible};
use merkle_cb_tree::{MerkleNode, ReadInterface, WriteInterface};
use rocksdb::{Options, DB};
use serde::{Deserialize, Serialize};
use simple_vcache::SVCache;
use std::collections::HashMap;
use std::fs::{self, File};
use std::net::TcpStream;
use std::path::Path;
use vbf::VersionBloomFilter;
use version_cache::VCache;
use vfs::{OpenAccess, OpenOptions, MERKLE_PATH};

#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
pub struct PageId(pub u32);

impl PageId {
    pub fn get_id(&self) -> u32 {
        self.0
    }
}

impl Digestible for PageId {
    fn to_digest(&self) -> Digest {
        self.0.to_digest()
    }
}

#[derive(Debug)]
pub struct ServerVfs {
    merkle_db_path: String,
    pub map: HashMap<PageId, Digest>,
    pub vbf: VersionBloomFilter,
}

impl ServerVfs {
    pub fn new(
        merkle_db_path: String,
        map: HashMap<PageId, Digest>,
        vbf: VersionBloomFilter,
    ) -> Self {
        Self {
            merkle_db_path,
            map,
            vbf,
        }
    }

    /// Open the file (of type `opts.kind`) at `path`.
    fn open(&self, path: &Path, opts: OpenOptions) -> Result<File> {
        let mut o = fs::OpenOptions::new();
        o.read(true).write(opts.access != OpenAccess::Read);
        match opts.access {
            OpenAccess::Create => {
                o.create(true);
            }
            OpenAccess::CreateNew => {
                o.create_new(true);
            }
            _ => {}
        }
        let f = o.open(path)?;
        Ok(f)
    }

    /// Delete the file at `path`.
    fn delete(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        std::fs::remove_file(path)
    }

    /// Check if file at `path` already exists.
    fn exists(&self, path: &Path) -> Result<bool, std::io::Error> {
        Ok(path.is_file())
    }

    /// Check access to `path`. The default implementation always returns `true`.
    fn access(&self, _path: &Path, _write: bool) -> Result<bool, std::io::Error> {
        Ok(true)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Type {
    None,
    Intra,
    Both,
    BothBloom,
    SimpleBloom,
}

#[derive(Debug)]
pub struct UserVfs<'a, 'b> {
    tp: Type,
    pub cache: &'a mut Cache,
    pub vcache: &'a mut VCache,
    pub svcache: &'a mut SVCache,
    stream: &'b mut TcpStream,
    pub map: &'a mut HashMap<PageId, Digest>,
    pub vbf: VersionBloomFilter,
}

impl<'a, 'b> UserVfs<'a, 'b> {
    pub fn new(
        tp: Type,
        cache: &'a mut Cache,
        vcache: &'a mut VCache,
        svcache: &'a mut SVCache,
        stream: &'b mut TcpStream,
        map: &'a mut HashMap<PageId, Digest>,
        vbf: VersionBloomFilter,
    ) -> Self {
        Self {
            tp,
            cache,
            vcache,
            svcache,
            stream,
            map,
            vbf,
        }
    }

    pub fn set_vbf(&mut self, vbf: VersionBloomFilter) {
        self.vbf = vbf;
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Parameter {
    root_id: Option<NodeId>,
}

impl Parameter {
    fn new(root_id: Option<NodeId>) -> Self {
        Self { root_id }
    }

    pub fn get_height(&self) -> u32 {
        match self.root_id {
            Some(id) => id.get_height(),
            None => 0,
        }
    }

    pub fn get_root_id(&self) -> Option<NodeId> {
        self.root_id
    }
}

pub struct MerkleDB {
    param: Parameter,
    merkle_db: DB,
}

impl MerkleDB {
    fn create(path: &Path, param: Parameter) -> Result<Self> {
        fs::create_dir_all(path).with_context(|| format!("failed to create dir {:?}", path))?;
        fs::write(
            path.join("param.json"),
            serde_json::to_string_pretty(&param)?,
        )?;
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        Ok(Self {
            param,
            merkle_db: DB::open(&opts, path.join("merkle.db"))?,
        })
    }

    fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            param: serde_json::from_str::<Parameter>(&fs::read_to_string(
                path.join("param.json"),
            )?)?,
            merkle_db: DB::open_default(path.join("merkle.db"))?,
        })
    }

    pub fn open_read_only(path: &Path) -> Result<Self> {
        let opts = Options::default();
        Ok(Self {
            param: serde_json::from_str::<Parameter>(&fs::read_to_string(
                path.join("param.json"),
            )?)?,
            merkle_db: DB::open_for_read_only(&opts, path.join("merkle.db"), true)?,
        })
    }

    pub fn create_new(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::open(path)
        } else {
            info!("attention! merkle db create is called, path is {:?}", path);
            Self::create(path, Parameter::new(None))
        }
    }

    pub fn get_height(&self) -> u32 {
        self.param.get_height()
    }

    pub fn get_root_id(&self) -> Option<NodeId> {
        self.param.get_root_id()
    }

    fn update_param(&mut self, new_root_id: Option<NodeId>) -> Result<()> {
        let path = Path::new(MERKLE_PATH);
        let param = Parameter::new(new_root_id);
        fs::write(
            path.join("param.json"),
            serde_json::to_string_pretty(&param)?,
        )?;
        Ok(())
    }

    pub fn close(self) {
        drop(self.merkle_db);
    }
}

impl ReadInterface for MerkleDB {
    fn get_node(&self, addr: &Digest) -> Result<Option<MerkleNode>> {
        let data_opt = self.merkle_db.get(addr.as_bytes())?;
        if let Some(data) = data_opt {
            Ok(Some(bincode::deserialize::<MerkleNode>(&data)?))
        } else {
            Ok(None)
        }
    }
}

impl WriteInterface for MerkleDB {
    fn write_node(&mut self, addr: &Digest, node: &MerkleNode) -> Result<()> {
        let bytes = bincode::serialize(node)?;
        self.merkle_db.put(addr.as_bytes(), bytes)?;
        Ok(())
    }
}
