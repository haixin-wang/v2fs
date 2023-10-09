use anyhow::{bail, Error, Result};
use howlong::ProcessDuration;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{BufReader, Read, Write},
    net::TcpStream,
    path::Path,
};
use tracing_subscriber::EnvFilter;

use crate::{
    cache::Cache,
    digest::{Digest, Digestible},
    merkle_cb_tree::ReadInterface,
    simple_vcache::SVCache,
    vbf::VersionBloomFilter,
    version_cache::VCache,
    vfs::{
        server_vfs::register_server, user_vfs::register_user, DEFAULT, END, MERKLE_PATH, NO_FLAG,
        SERVER_VFS, USER_VFS, YES_FLAG,
    },
    MerkleDB, PageId, Parameter, ServerVfs, Type, UserVfs,
};

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

#[derive(Debug, Serialize, Deserialize)]
pub struct ResInfo {
    pub query_t: Time,
    pub verify_t: Time,
    pub proof_s: usize,
    pub cache_size: u32,
}

impl ResInfo {
    pub fn new(query_t: Time, verify_t: Time, proof_s: usize, cache_size: u32) -> Self {
        Self {
            query_t,
            verify_t,
            proof_s,
            cache_size,
        }
    }
}

pub fn init_tracing_subscriber(directives: &str) -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(directives));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(Error::msg)
}

pub fn end_request(stream: &mut TcpStream) -> Result<()> {
    let end_info: (u32, PageId, Vec<Digest>) = (END, PageId(0), vec![]);
    let bytes = bincode::serialize(&end_info).expect("failed to serialize");
    let _w_amt = stream.write(&bytes)?;
    Ok(())
}

pub fn hand_shake(stream: &mut TcpStream, signal: u32) -> Result<()> {
    let _w_amt = stream.write(&signal.to_le_bytes())?;
    let mut reader = BufReader::new(stream);
    let mut buffer = [0; 4];
    reader.read_exact(&mut buffer)?;
    let sig = bincode::deserialize::<u32>(&buffer)?;

    if sig == YES_FLAG {
        Ok(())
    } else if sig == NO_FLAG {
        bail!("received no sginal");
    } else {
        bail!("transmission error, unknown signal");
    }
}

// should get the height from blockchain
pub fn get_height() -> Result<u32> {
    let param = serde_json::from_str::<Parameter>(&fs::read_to_string(
        Path::new(MERKLE_PATH).join("param.json"),
    )?)?;
    Ok(param.get_height())
}

// should get the root from blockchain
fn get_root() -> Result<Digest> {
    let param = serde_json::from_str::<Parameter>(&fs::read_to_string(
        Path::new(MERKLE_PATH).join("param.json"),
    )?)?;
    let root_id = param.get_root_id().expect("Root digest not exists");
    let merkle_db = MerkleDB::open_read_only(Path::new(MERKLE_PATH))?;
    let root_n = merkle_db.get_node(&root_id.to_digest())?;
    if let Some(n) = root_n {
        Ok(n.get_hash())
    } else {
        bail!("Cannot find root in merkle tree");
    }
}
pub fn compare_with_root(computed_hash: Digest) -> Result<()> {
    let root_hash = get_root()?;
    if computed_hash == root_hash {
        Ok(())
    } else {
        bail!("Proof root hash not matched")
    }
}

pub fn cal_cap(c_size_in_mb: usize, opt_level: u8) -> usize {
    let total_byte_num = c_size_in_mb * 1024 * 1024;
    let cap = total_byte_num / 4100;
    if opt_level == 2 {
        (cap as f64 * 1.4) as usize
    } else {
        cap
    }
}

#[allow(clippy::too_many_arguments)]
pub fn register_vfs(
    tp: Type,
    cache: &mut Cache,
    vcache: &mut VCache,
    svcache: &mut SVCache,
    stream: &mut TcpStream,
    map: &mut HashMap<PageId, Digest>,
    map_size: usize,
    hash_num: u32,
) -> Result<()> {
    let u_vfs = UserVfs::new(
        tp,
        cache,
        vcache,
        svcache,
        stream,
        map,
        VersionBloomFilter::new(map_size, hash_num),
    );
    register_user(USER_VFS, u_vfs)?;

    let s_vfs = ServerVfs::new(
        MERKLE_PATH.to_string(),
        HashMap::new(),
        VersionBloomFilter::new(map_size, hash_num),
    );
    register_server(SERVER_VFS, s_vfs)?;
    Ok(())
}

pub fn default_connect() -> Result<TcpStream> {
    let mut default_stream = TcpStream::connect("127.0.0.1:7878")?;
    let _w_amt = default_stream.write(&DEFAULT.to_le_bytes())?;
    Ok(default_stream)
}
