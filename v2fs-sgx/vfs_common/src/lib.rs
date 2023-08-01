#![no_std]

extern crate alloc;

pub mod digest;
pub mod page;

pub const MAX_PATH_LENGTH: usize = 512;
pub const PAGE_SIZE: usize = 4096;

pub const MERKLE_PATH: &str = "./db/merkle_db/merkle_test";

pub const MAIN_PATH: &str = "./db/sqlite_db/test.db";

pub const TMP_FILE_PATH: &str = "./db/tmp_file";

pub const SGX_VFS: &str = "sgx_vfs";

// 0: baseline; 1: no update for merkle db; 2: batch && no update for merkle db
pub const UPDATE_OPT_LEVEL: u8 = 0;
