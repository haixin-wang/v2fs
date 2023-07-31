#[macro_use]
extern crate tracing;
extern crate sgx_types;
extern crate sgx_urts;
use sgx_types::{sgx_attributes_t, sgx_launch_token_t, sgx_misc_attribute_t, SgxResult};
use sgx_urts::SgxEnclave;
use rocksdb::{Options, DB};
use merkle_tree::storage::{ReadInterface, WriteInterface, NodeId, MerkleNode};
use std::fs;
use std::path::Path;
use vfs_common::{digest::Digest, MERKLE_PATH};
use serde::{Serialize, Deserialize};
use anyhow::{Context, Result, Error};
use tracing_subscriber::EnvFilter;
use howlong::ProcessDuration;


pub mod ocall;

static ENCLAVE_FILE: &'static str =
    "/home/comp/hxwang/sgx_vsqlite/target/release/libvsqlite_enclave.signed.so";

pub fn init_enclave() -> SgxResult<SgxEnclave> {
    let mut launch_token: sgx_launch_token_t = [0; 1024];
    let mut launch_token_updated: i32 = 0;
    // call sgx_create_enclave to initialize an enclave instance
    // Debug Support: set 2nd parameter to 1
    let debug = 1;
    let mut misc_attr = sgx_misc_attribute_t {
        secs_attr: sgx_attributes_t { flags: 0, xfrm: 0 },
        misc_select: 0,
    };
    SgxEnclave::create(
        ENCLAVE_FILE,
        debug,
        &mut launch_token,
        &mut launch_token_updated,
        &mut misc_attr,
    )
}



pub fn init_tracing_subscriber(directives: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(directives));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(Error::msg)
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

    pub fn update_param(&mut self, new_root_id: Option<NodeId>) -> Result<()> {
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

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Time {
    pub real: u64,
    user: u64,
    sys: u64,
}

impl From<ProcessDuration> for Time {
    fn from(p_duration: ProcessDuration) -> Self {
        Self {
            real: p_duration.real.as_micros() as u64,
            user: p_duration.user.as_micros() as u64,
            sys: p_duration.system.as_micros() as u64,
        }
    }
}
