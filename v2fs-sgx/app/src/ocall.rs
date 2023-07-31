use vfs_common::{TMP_FILE_PATH, MAIN_PATH, MERKLE_PATH, page::PageId, digest::{Digest, Digestible}};
use rand::Rng;
use std::io::{ErrorKind, Result};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    slice, str,
    time::{Duration, Instant},
};
use time;
use crate::{MerkleDB, NodeId};
use merkle_tree::{read::ReadContext, write::WriteContext, proof::Proof, storage::{ReadInterface, WriteInterface}};
use std::ptr::copy_nonoverlapping;

#[no_mangle]
pub unsafe extern "C" fn ocall_file_open(str_ptr: *const u8, len: usize) -> i32 {
    let slice: &[u8] = slice::from_raw_parts(str_ptr, len);
    let f_path = str::from_utf8(slice).unwrap();

    let prefix = Path::new(f_path).parent().unwrap();
    fs::create_dir_all(prefix).unwrap();

    match fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(f_path)
    {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn ocall_file_delete(str_ptr: *const u8, len: usize) -> i32 {
    let slice: &[u8] = slice::from_raw_parts(str_ptr, len);
    let f_path = str::from_utf8(slice).unwrap();

    match fs::remove_file(f_path) {
        Ok(_) => 0,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                0
            } else {
                1
            }
        }
    }
}

// return 1 if exists, otherwise 0, it's a special design
#[no_mangle]
pub unsafe extern "C" fn ocall_file_exists(str_ptr: *const u8, len: usize, is_existed: *mut u8) -> i32 {
    let slice: &[u8] = slice::from_raw_parts(str_ptr, len);
    let f_path = str::from_utf8(slice).unwrap();
    let path = Path::new(&f_path);
    if path.is_file() {
        *is_existed = 1;
    } else {
        *is_existed = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_file_read(
    str_ptr: *const u8,
    len: usize,
    ofset: u64,
    buf: *mut u8,
    amt: usize,
) -> i32 {
    let slice: &[u8] = slice::from_raw_parts(str_ptr, len);
    let f_path = str::from_utf8(slice).unwrap();

    if f_path.eq(MAIN_PATH) {
        trace!("reading main db: {}", f_path);
    } else {
        trace!("reading other file: {}", f_path);
    }

    trace!("path: {}", f_path);
    let mut file = match File::open(&f_path) {
        Ok(f) => f,
        Err(_) => {
            trace!("cannot open");
            return 14; // ffi::SQLITE_CANTOPEN
        }
    };

    // move the cursor to the offset
    match file.seek(SeekFrom::Start(ofset)) {
        Ok(o) => {
            if o != ofset {
                trace!("io err1");
                return 266; // ffi::SQLITE_IOERR_READ
            }
        }
        Err(_) => {
            trace!("io err2");
            return 266; // ffi::SQLITE_IOERR_READ
        }
    }

    unsafe {
        let out = slice::from_raw_parts_mut(buf, amt);
        if let Err(err) = file.read_exact(out) {
            let kind = err.kind();
            if kind == ErrorKind::UnexpectedEof {
                // if len not enough, sqlite will fill with 0s
                trace!("len not enough");
                trace!("{:?}", out);
                return 522; // ffi::SQLITE_IOERR_SHORT_READ
            } else {
                return 266; // ffi::SQLITE_IOERR_READ
            }
        }
        trace!("{:?}", out);
    }
    0 // ffi::SQLITE_OK
}

fn file_update_open(path: &str) -> Result<File> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
}

#[no_mangle]
pub unsafe extern "C" fn ocall_file_write(
    str_ptr: *const u8,
    len: usize,
    ofset: u64,
    buf: *const u8,
    amt: usize,
) -> i32 {
    let slice: &[u8] = slice::from_raw_parts(str_ptr, len);
    let f_path = str::from_utf8(slice).unwrap();

    let mut file = match file_update_open(&f_path) {
        Ok(f) => f,
        Err(_) => return 14, // ffi::SQLITE_CANTOPEN
    };

    // move the cursor to the offset
    match file.seek(SeekFrom::Start(ofset)) {
        Ok(o) => {
            if o != ofset {
                return 266; // ffi::SQLITE_IOERR_READ
            }
        }
        Err(_) => {
            return 266; // ffi::SQLITE_IOERR_READ
        }
    }

    unsafe {
        let data = slice::from_raw_parts(buf, amt);
        trace!("{:?}", data);
        if let Err(_err) = file.write_all(data) {
            return 778; // ffi::SQLITE_IOERR_WRITE
        }
    }
    0 // ffi::SQLITE_OK
}

#[no_mangle]
pub unsafe extern "C" fn ocall_file_trancate(
    str_ptr: *const u8,
    len: usize,
    size: u64
) -> i32 {
    let slice: &[u8] = slice::from_raw_parts(str_ptr, len);
    let f_path = str::from_utf8(slice).unwrap();

    let file = match file_update_open(&f_path) {
        Ok(f) => f,
        Err(_) => return 14, // ffi::SQLITE_CANTOPEN
    };

    if file.set_len(size).is_err() {
        return 1546; // ffi::SQLITE_IOERR_TRUNCATE
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_file_flash(str_ptr: *const u8, len: usize) -> i32 {
    let slice: &[u8] = slice::from_raw_parts(str_ptr, len);
    let f_path = str::from_utf8(slice).unwrap();

    let mut file = match file_update_open(&f_path) {
        Ok(f) => f,
        Err(_) => return 14, // ffi::SQLITE_CANTOPEN
    };

    if file.flush().is_err() {
        return 1034; // ffi::SQLITE_IOERR_FSYNC
    }

    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_file_size(str_ptr: *const u8, len: usize, f_size: *mut u64) -> i32 {
    let slice: &[u8] = slice::from_raw_parts(str_ptr, len);
    let f_path = str::from_utf8(slice).unwrap();

    let file = match File::open(&f_path) {
        Ok(f) => f,
        Err(_) => {
            trace!("file {:?} does not exist", f_path);
            *f_size = 0;
            return 0;
        },
    };

    let len = file.metadata().expect("failed to query metadata").len();
    *f_size = len;
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_close_tmp_files() -> i32 {
    let path = Path::new(TMP_FILE_PATH);
    if path.exists() {
        trace!("removing tmp files: {}", TMP_FILE_PATH);
        fs::remove_file(path).expect("cannot remove tmp file");
    } else {
        trace!("tmp files do not exist");
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_sleep(n_micro: u64, elapsed_time: *mut i32) -> i32 {
    let instant = Instant::now();
    std::thread::sleep(Duration::from_micros(n_micro as u64));
    *elapsed_time = instant.elapsed().as_micros() as i32;
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_cur_time(cur_time: *mut f64) -> i32 {
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as f64;
    *cur_time = 2440587.5 + now / 864.0e5;
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_cur_time_i64(cur_time: *mut i64) -> i32 {
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as f64;
    *cur_time = ((2440587.5 + now / 864.0e5) * 864.0e5) as i64;
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_fill_rand_bytes(dest: *mut i8, len: usize) -> i32 {
    let slice = slice::from_raw_parts_mut(dest, len);
    rand::thread_rng().fill(slice);
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_read_proof_len(
    ptr: *const u8, 
    len: usize, 
    proof_len: *mut usize,
) -> i32 {
    let bytes: Vec<u8> = slice::from_raw_parts(ptr, len).to_vec();
    let p_ids: Vec<PageId> =
        postcard::from_bytes::<Vec<PageId>>(&bytes).unwrap();
    
    let proof = gen_proof(p_ids);
    let p_bytes = match postcard::to_allocvec(&proof) {
        Ok(buf) => buf,
        Err(e) => {
            println!("failed to cast Proof to bytes, reason: {:?}", e);
            return 1;
        }
    };
    *proof_len = p_bytes.len();

    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_read_proof_with_len(
    ptr: *const u8, 
    len: usize, 
    proof_ptr: *mut u8, 
    _predicated_p_len: usize,
    real_p_len: *mut usize,
) -> i32 {
    let bytes: Vec<u8> = slice::from_raw_parts(ptr, len).to_vec();
    let p_ids: Vec<PageId> =
        postcard::from_bytes::<Vec<PageId>>(&bytes).unwrap();

    let proof = gen_proof(p_ids);

    let p_bytes = match postcard::to_allocvec(&proof) {
        Ok(buf) => buf,
        Err(e) => {
            println!("failed to cast Proof to bytes, reason: {:?}", e);
            return 1;
        }
    };
    let real_proof_len = p_bytes.len();
    println!("dbg: real proof len: {}", real_proof_len);
    *real_p_len = real_proof_len;
    copy_nonoverlapping(p_bytes.as_ptr(), proof_ptr, real_proof_len);
    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_read_proof(
    ptr: *const u8, 
    len: usize, 
    proof_ptr: *mut u8, 
    proof_len: usize
) -> i32 {
    let bytes: Vec<u8> = slice::from_raw_parts(ptr, len).to_vec();
    let p_ids: Vec<PageId> =
        postcard::from_bytes::<Vec<PageId>>(&bytes).unwrap();

    let proof = gen_proof(p_ids);
    let p_bytes = match postcard::to_allocvec(&proof) {
        Ok(buf) => buf,
        Err(e) => {
            println!("failed to cast Proof to bytes, reason: {:?}", e);
            return 1;
        }
    };
    copy_nonoverlapping(p_bytes.as_ptr(), proof_ptr, proof_len);

    0
}

fn gen_proof(p_ids: Vec<PageId>) -> Proof {
    let merkle_db = MerkleDB::open_read_only(Path::new(MERKLE_PATH)).expect("Cannot open MerkleDB");
    let root_id = merkle_db.get_root_id();
    let mut ctx = ReadContext::new(&merkle_db, root_id).expect("Failed to create read ctx");
    for p_id in p_ids {
        ctx.query(p_id).expect("Query failed for a page");
    }
    ctx.into_proof()
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_merkle_root(ptr: *mut u8, len: usize) -> i32 {
    let merkle_db = MerkleDB::create_new(Path::new(MERKLE_PATH)).expect("Cannot open MerkleDB");
    let root_id: Option<NodeId> = merkle_db.get_root_id();
    let dig = if let Some(r_id) = root_id {
        let root = merkle_db.get_node(&r_id.to_digest()).unwrap().expect("Cannot find root");
        root.get_hash()
    } else {
        Digest::default()
    };
    let root_info = (root_id, dig);

    let bytes = match postcard::to_allocvec(&root_info) {
        Ok(buf) => buf,
        Err(e) => {
            println!("failed to cast root info to bytes, reason: {:?}", e);
            return 1;
        }
    };
    copy_nonoverlapping(bytes.as_ptr(), ptr, len);

    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_get_node(
    id_ptr: *const u8, 
    id_len: usize,
    ptr: *mut u8, 
    len: usize,
) -> i32 {
    let bytes: Vec<u8> = slice::from_raw_parts(id_ptr, id_len).to_vec();
    let n_id: NodeId =
        postcard::from_bytes::<NodeId>(&bytes).unwrap();
    let merkle_db = MerkleDB::open_read_only(Path::new(MERKLE_PATH)).expect("Cannot open MerkleDB");
    let node = merkle_db.get_node(&n_id.to_digest()).unwrap();

    let bytes = match postcard::to_allocvec(&node) {
        Ok(buf) => buf,
        Err(e) => {
            println!("failed to cast root info to bytes, reason: {:?}", e);
            return 1;
        }
    };
    copy_nonoverlapping(bytes.as_ptr(), ptr, len);

    0
}

#[no_mangle]
pub unsafe extern "C" fn ocall_update_merkle_db(
    ptr: *const u8, 
    len: usize,
) -> i32 {
    let bytes: Vec<u8> = slice::from_raw_parts(ptr, len).to_vec();
    let modif =
        postcard::from_bytes::<Vec<(PageId, Digest)>>(&bytes).unwrap();
    let mut merkle_db = MerkleDB::create_new(Path::new(MERKLE_PATH)).expect("failed to open or create merkle db");
    let root_id = merkle_db.get_root_id();
    let mut ctx = WriteContext::new(&merkle_db, root_id);
    for (p_id, dig) in modif {
        ctx.update(dig, p_id)
            .expect("Failed to update merkle tree");
    }
    let changes = ctx.changes();
    let new_root_id = changes.root_id;
    for (addr, node) in changes.nodes {
        merkle_db.write_node(&addr, &node).unwrap();
    }
    merkle_db.update_param(new_root_id).unwrap();

    // for dbg only
    println!("dbg: real new root id: {:?}", new_root_id.unwrap());
    let new_root_hash = merkle_db.get_node(&new_root_id.unwrap().to_digest()).unwrap().unwrap().get_hash();
    println!("dbg: real new root hash: {:?}", new_root_hash);

    merkle_db.close();

    0
}