#[macro_use]
extern crate tracing;

use anyhow::{bail, Result};
use std::collections::{HashMap, VecDeque};
use std::net::TcpStream;
use structopt::StructOpt;
use v2fs_vsqlite::digest::Digest;
use v2fs_vsqlite::query::{query, update_user_bf};
use v2fs_vsqlite::script::load_query_wkld;
use v2fs_vsqlite::simple_vcache::SVCache;
use v2fs_vsqlite::utils::{cal_cap, default_connect, register_vfs};
use v2fs_vsqlite::utils::{init_tracing_subscriber, ResInfo, Time};
use v2fs_vsqlite::{cache::Cache, version_cache::VCache};
use v2fs_vsqlite::{PageId, Type};

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(short, long, default_value = "500")]
    cache_size_in_mb: usize,

    // 0: no opt, 1: intra-cache, 2: inter-cache, 3: inter+vbf
    #[structopt(short, long, default_value = "0")]
    opt_level: u8,

    #[structopt(short, long, default_value = "./query/test_wkld.txt")]
    workload_path: String,

    #[structopt(short, long, default_value = "10000")]
    map_size: usize,

    #[structopt(short, long, default_value = "5")]
    hash_num: u32,
}

pub fn main() -> Result<()> {
    init_tracing_subscriber("info")?;
    let opts = Opt::from_args();
    let cache_size_in_mb = opts.cache_size_in_mb;
    let opt_level = opts.opt_level;
    let map_size = opts.map_size;
    let hash_num = opts.hash_num;
    let workload_path = opts.workload_path;
    let cache_cap = cal_cap(cache_size_in_mb, opt_level);

    let tp = match opt_level {
        0 => Type::None,
        1 => Type::Intra,
        2 => Type::Both,
        // 3 => Type::BothBloom,
        3 => Type::SimpleBloom,
        _ => bail!("Invalid opt_level"),
    };

    exp(cache_cap, tp, workload_path, map_size, hash_num)?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn exp(
    cache_cap: usize,
    tp: Type,
    workload_path: String,
    map_size: usize,
    hash_num: u32,
) -> Result<()> {
    let queries = load_query_wkld(&workload_path)?;
    let mut cache = Cache::new(cache_cap);
    let mut vcache = VCache::new(cache_cap);
    let mut svcache = SVCache::new(cache_cap);
    let mut stream = default_connect()?;
    let mut map = HashMap::new();

    register_vfs(
        tp,
        &mut cache,
        &mut vcache,
        &mut svcache,
        &mut stream,
        &mut map,
        map_size,
        hash_num,
    )?;

    exec_wkld(
        &mut cache,
        &mut vcache,
        &mut svcache,
        tp,
        &mut stream,
        &mut map,
        queries,
    )?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn exec_wkld(
    cache: &mut Cache,
    vcache: &mut VCache,
    svcache: &mut SVCache,
    tp: Type,
    stream: &mut TcpStream,
    map: &mut HashMap<PageId, Digest>,
    queries: Vec<String>,
) -> Result<()> {
    let mut res_infos = VecDeque::<ResInfo>::new();
    for (i, sql) in queries.iter().enumerate() {
        info!("Processing query: {}...", i);
        map.clear();
        match tp {
            Type::None => {}
            Type::Intra => {
                cache.clear();
            }
            Type::Both => {
                cache.unconfirm();
            }
            Type::BothBloom => {
                vcache.unconfirm();
                update_user_bf()?;
            }
            Type::SimpleBloom => {
                svcache.unconfirm();
                update_user_bf()?;
            }
        }

        // if opt_level == 4 {
        //     svcache.unconfirm();
        //     update_user_bf()?;
        // } else if opt_level == 3 {
        //     vcache.unconfirm();
        //     update_user_bf()?;
        // } else if opt_level == 2 {
        //     cache.unconfirm();
        // } else if opt_level == 1 {
        //     cache.clear();
        // }
        *stream = TcpStream::connect("127.0.0.1:7878")?;
        let timer = howlong::ProcessCPUTimer::new();
        let res_info = query(sql, tp, stream)?;
        let time = Time::from(timer.elapsed());
        info!("query time: {}ms", time.real / 1000);
        res_infos.push_back(res_info);
    }

    let size = res_infos.len();
    info!("res_infos len: {}", size);

    let mut total_q_t = 0;
    let mut total_v_t = 0;
    let mut total_p_s = 0;
    let mut c_size = 0;

    for res_info in res_infos {
        total_q_t += res_info.query_t.real;
        total_v_t += res_info.verify_t.real;
        total_p_s += res_info.proof_s;
        if c_size < res_info.cache_size {
            c_size = res_info.cache_size;
        }
    }
    let mut q_t_in_s = total_q_t as f64 / 1000000.0;
    q_t_in_s /= size as f64;
    let mut v_t_in_s = total_v_t as f64 / 1000000.0;
    v_t_in_s /= size as f64;
    let total_t_in_s = q_t_in_s + v_t_in_s;
    let mut p_s_in_kb = total_p_s as f64 / 1024.0;
    p_s_in_kb /= size as f64;

    info!(
        "average q_t: {}s, v_t: {}s, total_t: {}s, p_s: {}KB",
        q_t_in_s, v_t_in_s, total_t_in_s, p_s_in_kb
    );

    Ok(())
}
