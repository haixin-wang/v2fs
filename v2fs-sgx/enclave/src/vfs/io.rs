use std::ffi::{c_int, c_void};
use std::string::String;
use super::server_vfs::{ServerFileState, CachePage};
use vfs_common::{MAIN_PATH, PAGE_SIZE, page::{PageId}};
use anyhow::{Context, Result};
use libsqlite3_sys as ffi;
use sgx_types::sgx_status_t;
use alloc::vec::Vec;
use std::io::{Cursor, Seek, SeekFrom, Read, Write};
use hashbrown::HashMap;
use std::slice;

extern "C" {
    fn ocall_file_read(
        retval: *mut i32, 
        name_ptr: *const u8, 
        len: usize, 
        ofset: u64, 
        buf: *mut u8, 
        amt: usize
    ) -> sgx_status_t; 
    fn ocall_file_write(
        retval: *mut i32, 
        name_ptr: *const u8, 
        len: usize, 
        ofset: u64, 
        input_buf: *const u8, 
        amt: usize
    ) -> sgx_status_t;
    fn ocall_file_trancate( 
        retval: *mut i32, 
        name_ptr: *const u8, 
        len: usize, 
        size: u64 
    ) -> sgx_status_t;
    fn ocall_file_flash(
        retval: *mut i32, 
        name_ptr: *const u8, 
        len: usize
    ) -> sgx_status_t;
    fn ocall_file_size(
        retval: *mut i32, 
        name_ptr: *const u8, 
        len: usize, 
        f_size: *mut u64
    ) -> sgx_status_t;
    fn ocall_close_tmp_files(retval: *mut i32) -> sgx_status_t;
}

unsafe fn s_get_file(ptr: *mut ffi::sqlite3_file) -> Result<String> {
    let file_state = (ptr as *mut ServerFileState)
        .as_mut()
        .context("null pointer")?;
    let file = file_state
        .file
        .clone()
        .context("File in server file state not exist")?;
    Ok(file)
}

/// # Safety
///
/// Server reads data from a file.
pub unsafe extern "C" fn s_read(
    p_file: *mut ffi::sqlite3_file,
    z_buf: *mut c_void,
    i_amt: c_int,
    i_ofst: ffi::sqlite3_int64,
) -> c_int {
    trace!("read offset={} len={}", i_ofst, i_amt);
    let f_name = s_get_file(p_file).expect("failed to get file in ServerFileState");
    let f_name_buf = f_name.as_bytes();
    let mut retval: i32 = 0;

    if !f_name.eq(MAIN_PATH) {
        let sgx_ret =
            ocall_file_read(&mut retval as *mut _, f_name_buf.as_ptr(), f_name_buf.len(), i_ofst as u64, z_buf as *mut u8, i_amt as usize);
        if retval != 0 && retval != 522 { // ffi::SQLITE_IOERR_SHORT_READ
            println!("vfs_err happened, err code: {}", retval);
        }

        if sgx_ret != sgx_status_t::SGX_SUCCESS {
            println!("sgx_err happened");
        }
 
        retval
    } else {
        let file_state = (p_file as *mut ServerFileState)
            .as_mut()
            .context("null pointer")
            .expect("");
        let read_map = &mut file_state.read_map;
        let write_map = &file_state.write_map;
        let (start_point, p_ids) = compute_page_ids(i_ofst as u64, i_amt as u64);
        // println!("involved page ids: {:?}", p_ids);
        // let buf_len: usize = PAGE_SIZE * p_ids.len();
        let buf_ofst = i_ofst as u64 - start_point;
        // println!("buf_ofst: {}, i_amt: {}", buf_ofst, i_amt);
        // let buf = collect_page_bytes_batch(&f_name_buf, &p_ids, read_map, write_map);
        let buf = collect_page_bytes_base(&f_name_buf, &p_ids, read_map, write_map);
        // println!("collected: {:?}", buf);
        let mut cursor = Cursor::new(buf);
        match cursor.seek(SeekFrom::Start(buf_ofst)) {
            Ok(o) => {
                if o != buf_ofst {
                    println!("seek position not correct");
                }
            }
            Err(_) => {
                println!("seek err happened");
            }
        }
        
        let out = slice::from_raw_parts_mut(z_buf as *mut u8, i_amt as usize);
        if let Err(_err) = cursor.read_exact(out) {
            println!("read err");
        }

        retval
    }
    
}

fn collect_page_bytes_base(
    f_name_buf: &[u8], 
    p_ids: &Vec<PageId>,
    read_map: &mut HashMap<PageId, CachePage>,
    write_map: &HashMap<PageId, CachePage>,
) -> Vec<u8> {
    let mut res = Vec::<u8>::new();
    for p_id in p_ids {
        let mut bytes = read_page(f_name_buf, p_id);
        if write_map.get(p_id).is_none() {
            if read_map.get(p_id).is_none() {
                let cache_p = CachePage::new(0, PAGE_SIZE, bytes.clone());
                read_map.insert(*p_id, cache_p);
            }
        }
        res.append(&mut bytes);
    }
    res
}

fn collect_page_bytes_batch(
    f_name_buf: &[u8], 
    p_ids: &Vec<PageId>,
    read_map: &mut HashMap<PageId, CachePage>,
    write_map: &HashMap<PageId, CachePage>,
) -> Vec<u8> {
    let mut res = Vec::<u8>::new();
    for p_id in p_ids {
        if let Some(w_cache_p) = write_map.get(p_id) {
            if w_cache_p.get_len() == PAGE_SIZE {
                res.append(&mut w_cache_p.copy_bytes());
            } else {
                let offset = w_cache_p.get_offset();
                let len = w_cache_p.get_len();
                let w_slice = &w_cache_p.get_bytes()[offset..offset + len];

                if let Some(r_cache_p) = read_map.get(p_id) {
                    let r_bytes = r_cache_p.copy_bytes();
                    let mut cursor = Cursor::new(r_bytes);
                    match cursor.seek(SeekFrom::Start(offset as u64)) {
                        Ok(o) => {
                            if o != offset as u64 {
                                println!("seek position not correct");
                            }
                        }
                        Err(_) => {
                            println!("seek err happened");
                        }
                    }
                    if let Err(_err) = cursor.write_all(w_slice) {
                        println!("write err");
                    }
                    let mut buf = cursor.into_inner();
                    res.append(&mut buf);
                } else {
                    let bytes = read_page(f_name_buf, p_id);
                    let new_cache_p = CachePage::new(0, PAGE_SIZE, bytes.clone());
                    read_map.insert(*p_id, new_cache_p);
                    let mut cursor = Cursor::new(bytes);
                    match cursor.seek(SeekFrom::Start(offset as u64)) {
                        Ok(o) => {
                            if o != offset as u64 {
                                println!("seek position not correct");
                            }
                        }
                        Err(_) => {
                            println!("seek err happened");
                        }
                    }
                    if let Err(_err) = cursor.write_all(w_slice) {
                        println!("write err");
                    }
                    let mut buf = cursor.into_inner();
                    res.append(&mut buf);
                }
            }
            
        } else if let Some(cache_p) = read_map.get(p_id) {
            res.append(&mut cache_p.copy_bytes());
        } else {
            let mut bytes = read_page(f_name_buf, p_id);
            let cache_p = CachePage::new(0, PAGE_SIZE, bytes.clone());
            read_map.insert(*p_id, cache_p);
            res.append(&mut bytes);
        }
    }
    res
}

fn read_page(f_name_buf: &[u8], p_id: &PageId) -> Vec<u8> {
    let mut bytes = vec![0 as u8; PAGE_SIZE];
    let mut retval: i32 = 0;
    let ofset = p_id.get_id() as u64 * PAGE_SIZE as u64;
    let sgx_ret =
        unsafe {
            ocall_file_read(&mut retval as *mut _, f_name_buf.as_ptr(), f_name_buf.len(), ofset, bytes.as_mut_ptr(), PAGE_SIZE)
        };
    if retval != 0 && retval != 522 { // ffi::SQLITE_IOERR_SHORT_READ
        println!("vfs_err happened, err code: {}", retval);
    }
            
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }
    bytes
}

/// # Safety
///
/// Server writes data to a file.
pub unsafe extern "C" fn s_write(
    p_file: *mut ffi::sqlite3_file,
    z: *const c_void, // buffer
    i_amt: c_int,
    i_ofst: ffi::sqlite3_int64,
) -> c_int {
    trace!("server write offset={} len={}", i_ofst, i_amt);
    let f_name = s_get_file(p_file).expect("failed to get file in ServerFileState");
    let f_name_buf = f_name.as_bytes();
    let input_data = std::slice::from_raw_parts(z as *const u8, i_amt as usize);
    let mut retval: i32 = 0;

    if !f_name.eq(MAIN_PATH) {
        let sgx_ret = ocall_file_write(&mut retval as *mut _, f_name_buf.as_ptr(), f_name_buf.len(), i_ofst as u64, input_data.as_ptr(), i_amt as usize);
    
        if retval != 0 && retval != 522 {
            println!("vfs_err happened, err code: {}", retval);
        }
        if sgx_ret != sgx_status_t::SGX_SUCCESS {
            println!("sgx_err happened");
        }
        retval
    } else {
        let file_state = (p_file as *mut ServerFileState)
            .as_mut()
            .context("null pointer")
            .expect("");
        let write_map = &mut file_state.write_map;

        let (start_point, p_ids) = compute_page_ids(i_ofst as u64, i_amt as u64);

        retval = do_write_base(p_ids, f_name_buf, start_point, i_ofst as u64, i_amt as usize, write_map, &input_data);
        retval
    }
}

fn do_write_base(
    p_ids: Vec<PageId>, 
    f_name_buf: &[u8], 
    start_point: u64,
    i_ofst: u64,
    i_amt: usize,
    write_map: &mut HashMap<PageId, CachePage>,
    input_data: &[u8],
) -> c_int {
    let mut retval: i32 = 0;
    let buf_len: usize = PAGE_SIZE * p_ids.len();
    let mut buf = vec![0_u8; buf_len];
    let sgx_ret = 
        unsafe {
            ocall_file_read(&mut retval as *mut _, f_name_buf.as_ptr(), f_name_buf.len(), start_point, buf.as_mut_ptr(), buf_len)
        };
    if retval != 0 && retval != 522 {
        println!("vfs_err happened during read in write, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened during read in write");
    }

    let buf_ofst = i_ofst - start_point;
    let mut cursor = Cursor::new(buf);
    match cursor.seek(SeekFrom::Start(buf_ofst)) {
        Ok(o) => {
            if o != buf_ofst {
                println!("seek position not correct");
            }
        }
        Err(_) => {
            println!("seek err happened");
        }
    }
    if let Err(_err) = cursor.write_all(&input_data) {
        println!("write err");
    }
    let mut buf = cursor.into_inner();
    for p_id in p_ids {
        let bytes = buf.split_off(PAGE_SIZE);
        let cache_p = CachePage::new(0, PAGE_SIZE, buf);
        buf = bytes;
        write_map.insert(p_id, cache_p);
    }

    let sgx_ret = 
        unsafe {
            ocall_file_write(&mut retval as *mut _, f_name_buf.as_ptr(), f_name_buf.len(), i_ofst as u64, input_data.as_ptr(), i_amt)
        };
    

    if retval != 0 && retval != 512 {
        println!("vfs_err happened during read in write, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened during read in write");
    }

    retval

}

fn do_write_batch() -> c_int {
    todo!()
}

fn compute_page_ids(ofst: u64, len: u64) -> (u64, Vec<PageId>) {
    let start_page = ofst / PAGE_SIZE as u64;
    let start_point = start_page * PAGE_SIZE as u64;
    // let end_page = (ofst + len) / PAGE_SIZE as u64; 
    let end_page = (ofst + len - 1) / PAGE_SIZE as u64; // ATTENTION PLEASE
    let mut res = Vec::new();
    for i in start_page..(end_page + 1) {
        res.push(PageId(i as u32));
    }
    (start_point, res)
}

/// # Safety
///
/// Server truncates the file.
pub unsafe extern "C" fn s_truncate(
    p_file: *mut ffi::sqlite3_file,
    size: ffi::sqlite3_int64,
) -> c_int {
    trace!("truncate");
    let f_name = s_get_file(p_file).expect("failed to get file in ServerFileState");
    let buf = f_name.as_bytes();
    let mut retval: i32 = 0;
    let sgx_ret = 
        ocall_file_trancate(&mut retval as *mut _, buf.as_ptr(), buf.len(), size as u64);
    if retval != 0 {
        println!("vfs_err happened, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }
    retval
}

/// # Safety
///
/// Server persists changes to the file.
pub unsafe extern "C" fn s_sync(p_file: *mut ffi::sqlite3_file, _flags: c_int) -> c_int {
    trace!("s_sync");
    let f_name = s_get_file(p_file).expect("failed to get file in ServerFileState");
    let buf = f_name.as_bytes();
    let mut retval: i32 = 0;
    let sgx_ret = 
        ocall_file_flash(&mut retval as *mut _, buf.as_ptr(), buf.len());
    if retval != 0 {
        println!("vfs_err happened, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }
    retval
}

/// # Safety
///
/// Server returns the current file-size of the file.
pub unsafe extern "C" fn s_file_size(
    p_file: *mut ffi::sqlite3_file,
    p_size: *mut ffi::sqlite3_int64,
) -> c_int {
    trace!("file_size");
    let f_name = s_get_file(p_file).expect("failed to get file in ServerFileState");
    let buf = f_name.as_bytes();
    let mut retval: i32 = 0;
    let mut f_size: u64 = 0;
    let sgx_ret = 
        ocall_file_size(&mut retval as *mut _, buf.as_ptr(), buf.len(), &mut f_size as *mut u64);
    if retval != 0 {
        println!("vfs_err happened, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }
    *p_size = f_size as ffi::sqlite3_int64;
    0
}

/// # Safety
///
/// Close a file.
pub unsafe extern "C" fn s_close(p_file: *mut ffi::sqlite3_file) -> c_int {
    trace!("close");
    if let Some(file_state) = (p_file as *mut ServerFileState).as_mut() {
        trace!("close file {:?}", file_state.file);
        file_state.file = None;
    }
    let mut retval: i32 = 0;
    let sgx_ret = ocall_close_tmp_files(&mut retval as *mut _);
    if retval != 0 {
        println!("vfs_err happened, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }

    0
}

/// # Safety
///
/// Lock a file.
pub unsafe extern "C" fn lock(_p_file: *mut ffi::sqlite3_file, _e_lock: c_int) -> c_int {
    trace!("lock");
    ffi::SQLITE_OK
}

/// # Safety
///
/// Unlock a file.
pub unsafe extern "C" fn unlock(_p_file: *mut ffi::sqlite3_file, _e_lock: c_int) -> c_int {
    trace!("unlock");
    // TODO: implement unlocking
    ffi::SQLITE_OK
}

/// # Safety
///
/// Check if another file-handle holds a RESERVED lock on a file.
pub unsafe extern "C" fn check_reserved_lock(
    _p_file: *mut ffi::sqlite3_file,
    p_res_out: *mut c_int,
) -> c_int {
    trace!("check_reserved_lock");
    match p_res_out.as_mut() {
        Some(p_res_out) => {
            *p_res_out = false as i32;
        }
        None => {
            return ffi::SQLITE_IOERR_CHECKRESERVEDLOCK;
        }
    }

    ffi::SQLITE_OK
}

/// # Safety
///
/// File control method. For custom operations on an mem-file.
pub unsafe extern "C" fn file_control(
    _p_file: *mut ffi::sqlite3_file,
    op: c_int,
    _p_arg: *mut c_void,
) -> c_int {
    trace!("file_control op={}", op);
    ffi::SQLITE_NOTFOUND
}

/// # Safety
///
/// Return the sector-size in bytes for a file.
pub unsafe extern "C" fn sector_size(_p_file: *mut ffi::sqlite3_file) -> c_int {
    trace!("sector_size");

    1024
}

/// # Safety
///
/// Return the device characteristic flags supported by a file.
pub unsafe extern "C" fn device_characteristics(_p_file: *mut ffi::sqlite3_file) -> c_int {
    trace!("device_characteristics");
    // For now, simply copied from [memfs] without putting in a lot of thought.
    // [memfs]: (https://github.com/sqlite/sqlite/blob/a959bf53110bfada67a3a52187acd57aa2f34e19/ext/misc/memvfs.c#L271-L276)

    // writes of any size are atomic
    ffi::SQLITE_IOCAP_ATOMIC |
        // after reboot following a crash or power loss, the only bytes in a file that were written
        // at the application level might have changed and that adjacent bytes, even bytes within
        // the same sector are guaranteed to be unchanged
        ffi::SQLITE_IOCAP_POWERSAFE_OVERWRITE |
        // when data is appended to a file, the data is appended first then the size of the file is
        // extended, never the other way around
        ffi::SQLITE_IOCAP_SAFE_APPEND |
        // information is written to disk in the same order as calls to xWrite()
        ffi::SQLITE_IOCAP_SEQUENTIAL
}

/// # Safety
///
/// Create a shared memory file mapping.
pub unsafe extern "C" fn shm_map(
    _p_file: *mut ffi::sqlite3_file,
    i_pg: i32,
    pgsz: i32,
    b_extend: i32,
    _pp: *mut *mut c_void,
) -> i32 {
    trace!("shm_map pg={} sz={} extend={}", i_pg, pgsz, b_extend);

    ffi::SQLITE_IOERR_SHMMAP
}

/// # Safety
///
/// Perform locking on a shared-memory segment.
pub unsafe extern "C" fn shm_lock(
    _p_file: *mut ffi::sqlite3_file,
    _offset: i32,
    _n: i32,
    _flags: i32,
) -> i32 {
    trace!("shm_lock");

    ffi::SQLITE_IOERR_SHMLOCK
}

/// # Safety
///
/// Memory barrier operation on shared memory.
pub unsafe extern "C" fn shm_barrier(_p_file: *mut ffi::sqlite3_file) {
    trace!("shm_barrier");
}

/// # Safety
///
/// Unmap a shared memory segment.
pub unsafe extern "C" fn shm_unmap(_p_file: *mut ffi::sqlite3_file, _delete_flags: i32) -> i32 {
    trace!("shm_unmap");

    ffi::SQLITE_OK
}

/// # Safety
///
/// Fetch a page of a memory-mapped file.
pub unsafe extern "C" fn mem_fetch(
    _p_file: *mut ffi::sqlite3_file,
    i_ofst: i64,
    i_amt: i32,
    _pp: *mut *mut c_void,
) -> i32 {
    trace!("mem_fetch offset={} len={}", i_ofst, i_amt);

    ffi::SQLITE_ERROR
}

/// # Safety
///
/// Release a memory-mapped page.
pub unsafe extern "C" fn mem_unfetch(
    _p_file: *mut ffi::sqlite3_file,
    i_ofst: i64,
    _p_page: *mut c_void,
) -> i32 {
    trace!("mem_unfetch offset={}", i_ofst);

    ffi::SQLITE_OK
}
