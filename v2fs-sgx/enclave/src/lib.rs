#![no_std]

#![crate_name = "vsqlite_enclave_rust"]
#![crate_type = "staticlib"]

#[macro_use]
extern crate sgx_tstd as std;
#[macro_use]
extern crate alloc;
#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate tracing;

use std::string::String;
use std::slice;
use std::str;
use sgx_types::sgx_status_t;
use vfs_common::{MAIN_PATH, SGX_VFS};
use crate::{verify::verify_then_update};
use rusqlite::{Connection, OpenFlags};
use alloc::vec::Vec;

pub mod vfs;
pub mod verify;

#[no_mangle]
pub extern "C" fn ecall_exec(stmt_ptr: *const u8, len: usize) -> sgx_status_t {
    let bytes = unsafe { slice::from_raw_parts(stmt_ptr, len) };
    let stmts =
        postcard::from_bytes::<Vec<String>>(&bytes).expect("failed to cast bytes to Vec<String>");

    for stmt in stmts {
        if exec_stmt(&stmt) != 0 {
            return sgx_status_t::SGX_ERROR_UNEXPECTED;
        }
    }
    return sgx_status_t::SGX_SUCCESS;
    // if exec_stmt_in_batch(&stmts) == 0 {
    //     sgx_status_t::SGX_SUCCESS
    // } else {
    //     sgx_status_t::SGX_ERROR_UNEXPECTED
    // }
}

#[allow(dead_code)]
fn exec_stmt(stmt: &str) -> u32 {
    let conn = Connection::open_with_flags_and_vfs(
        MAIN_PATH,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            SGX_VFS,
    ).unwrap();
    conn.execute(stmt,[],).unwrap();
    verify_then_update().unwrap();
    0
}

#[allow(dead_code)]
fn exec_stmt_in_batch(stmts: &Vec<String>) -> u32 {
    let mut conn = Connection::open_with_flags_and_vfs(
        MAIN_PATH,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            SGX_VFS,
    ).unwrap();

    let tx = conn.transaction().unwrap();
    for stmt in stmts {
        tx.execute(&stmt, []).unwrap();
    }
    tx.commit().unwrap();
    verify_then_update().unwrap();
    0
}


#[allow(dead_code)]
fn sqlite_test() -> u32 {
    #[derive(Debug)]
    pub struct Person {
        pub id: u32,
        pub name: String,
        pub balance: u32,
    }
    
    let conn = Connection::open_with_flags_and_vfs(
        MAIN_PATH,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        SGX_VFS,
    ).unwrap();
    conn.execute("CREATE TABLE IF NOT EXISTS person (id INT PRIMARY KEY, name VARCHAR, balance INT)",[],).unwrap();
    conn.execute("INSERT INTO person VALUES (31, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (32, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (33, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (34, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (35, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (36, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (37, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (38, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (39, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (40, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (41, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (42, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (43, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (44, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (45, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (46, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (47, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (48, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (49, 'Tom', 200)", []).unwrap();
    conn.execute("INSERT INTO person VALUES (50, 'Tom', 200)", []).unwrap();

    let sql = "select * from person;";
    let mut stmt = conn.prepare(sql).unwrap();
    let res_iter = match stmt.query_map([], |row| {
        let id: u32 = row.get(0).unwrap();
        let name: String = row.get(1).unwrap();
        let balance: u32 = row.get(2).unwrap();
        Ok(Person { id, name, balance })
    }) {
        Ok(r) => r,
        Err(_) => {
            println!("cannot conduct query map");
            return 1;
        }
    };

    for p in res_iter {
        println!("{:?}", p.unwrap());
    }

    verify_then_update().unwrap();
    0
}

