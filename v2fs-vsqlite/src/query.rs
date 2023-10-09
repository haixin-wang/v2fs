use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use std::{
    ffi::CString,
    io::{Read, Write},
    mem::ManuallyDrop,
    net::TcpStream,
};

use crate::{
    merkle_cb_tree::proof::Proof,
    utils::{end_request, get_height, hand_shake, ResInfo, Time},
    verify::verify,
    vfs::{
        server_vfs::{server_vfs_state, update_merkle_db},
        user_vfs::user_vfs_state,
        BOTH_CACHE, HOLDER_FILE_PATH, MAIN_PATH, NO_CACHE, PAGE_SIZE, SERVER_VFS, USER_VFS,
        YES_FLAG,
    },
    Type,
};

pub fn query(sql: &str, tp: Type, stream: &mut TcpStream) -> Result<ResInfo> {
    let signal = match tp {
        Type::None | Type::Intra => NO_CACHE,
        Type::Both | Type::BothBloom | Type::SimpleBloom => BOTH_CACHE,
    };

    // let signal = if opt_level == 2 || opt_level == 3 {
    //     BOTH_CACHE
    // } else {
    //     NO_CACHE
    // };

    let timer1 = howlong::ProcessCPUTimer::new();
    hand_shake(stream, signal)?;
    query_from_vfs(sql, stream)?;
    let buf = receive_proof(stream);
    let proof = bincode::deserialize::<Proof>(&buf)?;
    let q_time = Time::from(timer1.elapsed());
    info!("query time: {}ms", q_time.real / 1000);

    // verification
    info!("verifying results...");
    let height = get_height()?;
    let timer2 = howlong::ProcessCPUTimer::new();
    let name = ManuallyDrop::new(CString::new(USER_VFS)?);
    let (cache_size, _cache_height, map) = unsafe {
        let p_vfs = libsqlite3_sys::sqlite3_vfs_find(name.as_ptr());
        let state = user_vfs_state(p_vfs).expect("null pointer");
        let u_vfs = &state.vfs;
        match tp {
            Type::None | Type::Intra | Type::Both => {
                let cache = &u_vfs.cache;
                let (cache_size, cache_height) = cache.cache_size_and_height();
                (cache_size, cache_height, &u_vfs.map)
            }
            Type::BothBloom => {
                let vcache = &u_vfs.vcache;
                let (cache_size, cache_height) = vcache.cache_size_and_height();
                (cache_size, cache_height, &u_vfs.map)
            }
            Type::SimpleBloom => {
                let svcache = &u_vfs.svcache;
                let (cache_size, cache_height) = svcache.cache_size_and_height();
                (cache_size, cache_height, &u_vfs.map)
            }
        }
        // match opt_level {
        //     4 => {
        //         let svcache = &u_vfs.svcache;
        //         let (cache_size, cache_height) = svcache.cache_size_and_height();
        //         (cache_size, cache_height, &u_vfs.map)
        //     }
        //     3 => {
        //         let vcache = &u_vfs.vcache;
        //         let (cache_size, cache_height) = vcache.cache_size_and_height();
        //         (cache_size, cache_height, &u_vfs.map)
        //     }
        //     2 | 1 | 0 => {
        //         let cache = &u_vfs.cache;
        //         let (cache_size, cache_height) = cache.cache_size_and_height();
        //         (cache_size, cache_height, &u_vfs.map)
        //     }
        //     _ => {
        //         panic!("invalid opt_level");
        //     }
        // }
    };
    verify(height, &proof, map)?;
    let v_time = Time::from(timer2.elapsed());
    let p_size = bincode::serialize(&proof)?.len();
    info!("verification succeeds!");
    info!("verification time: {}ms", v_time.real / 1000);
    // info!(
    //     "Cache size: {} MB, height: {}",
    //     cache_size as f64 / (1024.0 * 1024.0),
    //     cache_height
    // );
    Ok(ResInfo::new(q_time, v_time, p_size, cache_size))
}

fn query_from_vfs(sql: &str, stream: &mut TcpStream) -> Result<()> {
    let conn = Connection::open_with_flags_and_vfs(
        HOLDER_FILE_PATH,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        USER_VFS,
    )?;
    let mut stmt = conn.prepare(sql)?;
    let mut res_rows = stmt.query([])?;
    let mut res_cnt = 0;

    while let Some(_row) = res_rows.next()? {
        res_cnt += 1;
    }

    info!("Query finished, the num of res records: {}", res_cnt);
    end_request(stream)?;
    Ok(())
}

fn receive_proof(stream: &mut TcpStream) -> Vec<u8> {
    // Read the vector size from the client
    let mut buffer = [0; PAGE_SIZE as usize];
    let _bytes_read = stream.read(&mut buffer).expect("failed to read stream");
    let vector_size = bincode::deserialize::<u32>(&buffer).expect("failed to deserialize bincode");

    // Write response
    let _w_amt = stream
        .write(&YES_FLAG.to_le_bytes())
        .expect("failed to write");

    // Read the vector data from the client
    let mut vector = vec![0u8; vector_size as usize];
    stream
        .read_exact(&mut vector)
        .expect("Failed to read vector data");

    vector
}

pub fn update_db(sql: &str) -> Result<()> {
    let conn = Connection::open_with_flags_and_vfs(
        MAIN_PATH,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        SERVER_VFS,
    )?;
    conn.execute(sql, [])?;
    update_merkle_db()?;
    Ok(())
}

#[allow(dead_code)]
pub fn dbg_query(sql: &str) -> Result<()> {
    let conn = Connection::open_with_flags_and_vfs(
        MAIN_PATH,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        SERVER_VFS,
    )?;
    let mut stmt = conn.prepare(sql)?;
    let mut res_rows = stmt.query([])?;
    let mut res_cnt = 0;

    while let Some(_row) = res_rows.next()? {
        res_cnt += 1;
    }
    info!("Query finished, the num of res records: {}", res_cnt);
    Ok(())
}

// simulate to obtain the latest vbf from the sgx
pub fn update_user_bf() -> Result<()> {
    let s_name = ManuallyDrop::new(CString::new(SERVER_VFS)?);
    let vbf = unsafe {
        let p_vfs = libsqlite3_sys::sqlite3_vfs_find(s_name.as_ptr());
        let state = server_vfs_state(p_vfs).expect("null pointer");
        let s_vfs = &mut state.vfs;
        s_vfs.vbf.clone()
    };

    let u_name = ManuallyDrop::new(CString::new(USER_VFS)?);
    unsafe {
        let p_vfs = libsqlite3_sys::sqlite3_vfs_find(u_name.as_ptr());
        let state = user_vfs_state(p_vfs).expect("null pointer");
        let u_vfs = &mut state.vfs;
        u_vfs.set_vbf(vbf);
    }

    Ok(())
}
