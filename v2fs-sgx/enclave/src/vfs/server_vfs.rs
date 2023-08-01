use anyhow::{Context, Result};
use libsqlite3_sys as ffi;
use ffi::{sqlite3_snprintf, sqlite3_vfs};
use hashbrown::HashMap;
use std::{
    ffi::{c_char, c_int, c_void, CStr, CString},
    mem::{size_of, ManuallyDrop},
    ptr::null_mut,
    slice,
    string::{String, ToString},
};
use alloc::{boxed::Box, vec::Vec};
use sgx_types::sgx_status_t;
use vfs_common::{page::PageId, MAX_PATH_LENGTH, TMP_FILE_PATH, MERKLE_PATH, SGX_VFS, digest::{Digest, Digestible}};

use super::io;

extern "C" {
    fn ocall_file_open( retval: *mut i32, name_ptr: *const u8, len: usize ) -> sgx_status_t;
    fn ocall_file_delete( retval: *mut i32, name_ptr: *const u8, len: usize ) -> sgx_status_t;
    fn ocall_file_exists( retval: *mut i32, name_ptr: *const u8, len: usize, is_existed: *mut i32 ) -> sgx_status_t;
    fn ocall_sleep( retval: *mut i32, n_micro: u64, elapsed_time: *mut i32) -> sgx_status_t;
    fn ocall_cur_time(retval: *mut i32, cur_time: *mut f64) -> sgx_status_t;
    fn ocall_cur_time_i64(retval: *mut i32, cur_time: *mut i64) -> sgx_status_t;
    fn ocall_fill_rand_bytes(retval: *mut i32, dest: *mut i8, len: usize) -> sgx_status_t;
}

#[derive(Debug)]
pub struct CachePage {
    offset: usize,
    len: usize,
    bytes: Vec<u8>,
}

impl CachePage {
    pub fn new(offset: usize, len: usize, bytes: Vec<u8>) -> Self {
        Self {
            offset,
            len,
            bytes,
        }
    }

    pub fn get_bytes(&self) -> &Vec<u8> {
        &self.bytes
    }
    
    pub fn copy_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    pub fn to_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn get_offset(&self) -> usize {
        self.offset
    }

    pub fn get_len(&self) -> usize {
        self.len
    }
}

impl Digestible for CachePage {
    fn to_digest(&self) -> Digest {
        self.bytes.to_digest()
    }
}

#[derive(Debug)]
pub struct ServerVfs {
    merkle_db_path: String,
    pub read_map: HashMap<PageId, CachePage>,
    pub write_map: HashMap<PageId, CachePage>,
}

impl ServerVfs {
    pub fn new(
        merkle_db_path: String,
        read_map: HashMap<PageId, CachePage>,
        write_map: HashMap<PageId, CachePage>,
    ) -> Self {
        Self { 
            merkle_db_path,
            read_map,
            write_map,
        }
    }

    fn sim_open(&self, path: &str) -> i32 {
        let f_name = String::from(path);
        let mut retval: i32 = 0;
        let buf = f_name.as_bytes();
        let sgx_ret = unsafe {
            ocall_file_open(&mut retval as *mut _, buf.as_ptr(), buf.len())
        };

        if retval != 0 {
            println!("vfs_err happened, err code: {}", retval);
        }
    
        if sgx_ret != sgx_status_t::SGX_SUCCESS {
            println!("sgx_err happened");
        }
        0
    }

    fn sim_delete(&self, path: &str) -> i32 {
        let f_name = String::from(path);
        let mut retval: i32 = 0;
        let buf = f_name.as_bytes();
        let sgx_ret = unsafe {
            ocall_file_delete(&mut retval as *mut _, buf.as_ptr(), buf.len())
        };

        if retval != 0 {
            println!("vfs_err happened, err code: {}", retval);
        }
    
        if sgx_ret != sgx_status_t::SGX_SUCCESS {
            println!("sgx_err happened");
        }
        0
    }

    fn sim_exists(&self, path: &str) -> i32 {
        let f_name = String::from(path);
        let mut retval: i32 = 0;
        let mut is_existed: i32 = 0;
        let buf = f_name.as_bytes();
        let sgx_ret = unsafe {
            ocall_file_exists(&mut retval as *mut _, buf.as_ptr(), buf.len(), &mut is_existed as *mut _)
        };
        if retval != 0 {
            println!("vfs_err happened, err code: {}", retval);
        }
    
        if sgx_ret != sgx_status_t::SGX_SUCCESS {
            println!("sgx_err happened");
        }
        is_existed
    }
}

// #[derive(Debug)]
// pub struct ServerVfs {
//     merkle_db_path: String,
//     pub read_map: HashMap<PageId, Digest>,
//     pub write_map: HashMap<PageId, Digest>,
// }

// impl ServerVfs {
//     pub fn new(
//         merkle_db_path: String,
//         read_map: HashMap<PageId, Digest>,
//         write_map: HashMap<PageId, Digest>,
//     ) -> Self {
//         Self { 
//             merkle_db_path,
//             read_map,
//             write_map,
//         }
//     }

//     fn sim_open(&self, path: &str) -> i32 {
//         let f_name = String::from(path);
//         let mut retval: i32 = 0;
//         let buf = f_name.as_bytes();
//         let sgx_ret = unsafe {
//             ocall_file_open(&mut retval as *mut _, buf.as_ptr(), buf.len())
//         };

//         if retval != 0 {
//             println!("vfs_err happened, err code: {}", retval);
//         }
    
//         if sgx_ret != sgx_status_t::SGX_SUCCESS {
//             println!("sgx_err happened");
//         }
//         0
//     }

//     fn sim_delete(&self, path: &str) -> i32 {
//         let f_name = String::from(path);
//         let mut retval: i32 = 0;
//         let buf = f_name.as_bytes();
//         let sgx_ret = unsafe {
//             ocall_file_delete(&mut retval as *mut _, buf.as_ptr(), buf.len())
//         };

//         if retval != 0 {
//             println!("vfs_err happened, err code: {}", retval);
//         }
    
//         if sgx_ret != sgx_status_t::SGX_SUCCESS {
//             println!("sgx_err happened");
//         }
//         0
//     }

//     fn sim_exists(&self, path: &str) -> i32 {
//         let f_name = String::from(path);
//         let mut retval: i32 = 0;
//         let mut is_existed: i32 = 0;
//         let buf = f_name.as_bytes();
//         let sgx_ret = unsafe {
//             ocall_file_exists(&mut retval as *mut _, buf.as_ptr(), buf.len(), &mut is_existed as *mut _)
//         };
//         if retval != 0 {
//             println!("vfs_err happened, err code: {}", retval);
//         }
    
//         if sgx_ret != sgx_status_t::SGX_SUCCESS {
//             println!("sgx_err happened");
//         }
//         is_existed
//     }
// }

#[derive(Debug)]
#[repr(C)]
pub struct ServerState {
    pub vfs: ServerVfs,
    io_methods: ffi::sqlite3_io_methods,
}


#[derive(Debug)]
#[repr(C)]
pub(crate) struct ServerFileState<'a> {
    pub(crate) ctx: ffi::sqlite3_file,
    pub(crate) file: Option<String>,
    pub(crate) merkle_db_path: String,
    pub(crate) read_map: &'a mut HashMap<PageId, CachePage>,
    pub(crate) write_map: &'a mut HashMap<PageId, CachePage>,
}


/// # Safety
///
/// this function gets the vfs state for server
pub unsafe fn server_vfs_state<'a>(ptr: *mut ffi::sqlite3_vfs) -> Result<&'a mut ServerState> {
    let vfs: &mut ffi::sqlite3_vfs = ptr.as_mut().context("received null pointer")?;
    let state = (vfs.pAppData as *mut ServerState)
        .as_mut()
        .context("received null pointer")?;
    Ok(state)
}

/// # Safety
///
/// Open a new file handler.
pub unsafe extern "C" fn s_open(
    p_vfs: *mut ffi::sqlite3_vfs,
    z_name: *const c_char,
    p_file: *mut ffi::sqlite3_file,
    _flags: c_int,
    _p_out_flag: *mut c_int,
) -> c_int {
    trace!("s_open");
    let path;
    if z_name.is_null() {
        path = TMP_FILE_PATH.to_string();
        trace!("z_name is null");
    } else {
        path = CStr::from_ptr(z_name).to_string_lossy().to_string();
        trace!("opening {}", path);
    }

    let state = server_vfs_state(p_vfs).expect("null pointer");

    let s_vfs = &mut state.vfs;

    let s_file = if s_vfs.sim_open(path.as_ref()) == 0 {
        path.to_string()
    } else {
        return ffi::SQLITE_CANTOPEN;
    };

    let file_state = (p_file as *mut ServerFileState)
        .as_mut()
        .expect("null pointer");
    file_state.ctx.pMethods = &state.io_methods;
    file_state.merkle_db_path = s_vfs.merkle_db_path.clone();
    file_state.file = Some(s_file);
    file_state.read_map = &mut s_vfs.read_map;
    file_state.write_map = &mut s_vfs.write_map;

    ffi::SQLITE_OK
}

/// # Safety
///
/// Delete the file located at `z_path`. If the `sync_dir` argument is true, ensure the file-system modifications are synced to disk before returning.
pub unsafe extern "C" fn s_delete(
    p_vfs: *mut ffi::sqlite3_vfs,
    z_path: *const c_char,
    _sync_dir: c_int,
) -> c_int {
    trace!("s_delete");
    let state = server_vfs_state(p_vfs).expect("null pointer");
    let path = CStr::from_ptr(z_path);
    let path = path.to_string_lossy().to_string();
    trace!("s_delete {}", path);

    let delete_res = state.vfs.sim_delete(path.as_ref());

    if delete_res == 0 {
        ffi::SQLITE_OK
    } else {
        ffi::SQLITE_DELETE
    }
}

/// # Safety
///
/// Test for access permissions. Return true if the requested permission is available, or false otherwise.
pub unsafe extern "C" fn s_access(
    p_vfs: *mut ffi::sqlite3_vfs,
    z_path: *const c_char,
    flags: c_int,
    p_res_out: *mut c_int,
) -> c_int {
    trace!("server access");
    let state = server_vfs_state(p_vfs).expect("null pointer");
    let path = CStr::from_ptr(z_path);
    let path = path.to_string_lossy().to_string();
    trace!("{}", path);

    let result = match flags {
        ffi::SQLITE_ACCESS_EXISTS => state.vfs.sim_exists(path.as_ref()),
        ffi::SQLITE_ACCESS_READ => 1,
        ffi::SQLITE_ACCESS_READWRITE => 1,
        _ => return ffi::SQLITE_IOERR_ACCESS,
    };

    let p_res_out: &mut c_int = p_res_out.as_mut().expect("null pointer");
    *p_res_out = result;

    ffi::SQLITE_OK
}

/// # Safety
///
// Populate buffer `z_out` with the full canonical pathname corresponding to the pathname in `z_path`. `z_out` is guaranteed to point to a buffer of at least (INST_MAX_PATHNAME+1) bytes.
pub unsafe extern "C" fn s_full_pathname(
    _p_vfs: *mut ffi::sqlite3_vfs,
    z_path: *const c_char,
    n_out: c_int,
    z_out: *mut c_char,
) -> c_int {
    trace!("s_full_pathname");
    let name = CStr::from_ptr(z_path);
    let name = name.to_bytes_with_nul();
    if name.len() > n_out as usize || name.len() > MAX_PATH_LENGTH {
        return ffi::SQLITE_ERROR;
    }
    let out = slice::from_raw_parts_mut(z_out as *mut u8, name.len());
    out.copy_from_slice(name);

    ffi::SQLITE_OK
}

/// # Safety
///
/// Open the dynamic library located at `z_path` and return a handle.
pub unsafe extern "C" fn s_dlopen(
    _p_vfs: *mut ffi::sqlite3_vfs,
    _z_path: *const c_char,
) -> *mut c_void {
    trace!("s_dlopen");
    null_mut()
}

/// # Safety
///
/// Populate the buffer `z_err_msg` (size `n_byte` bytes) with a human readable utf-8 string describing the most recent error encountered associated with dynamic libraries.
pub unsafe extern "C" fn s_dlerror(
    _p_vfs: *mut ffi::sqlite3_vfs,
    n_byte: c_int,
    z_err_msg: *mut c_char,
) {
    trace!("s_dlerror");
    let msg = concat!("Loadable extensions are not supported", "\0");
    sqlite3_snprintf(n_byte, z_err_msg, msg.as_ptr() as _);
}

/// # Safety
///
/// Return a pointer to the symbol `z_sym` in the dynamic library pHandle.
pub unsafe extern "C" fn s_dlsym(
    _p_vfs: *mut ffi::sqlite3_vfs,
    _p: *mut c_void,
    _z_sym: *const c_char,
) -> Option<unsafe extern "C" fn(*mut ffi::sqlite3_vfs, *mut c_void, *const c_char)> {
    trace!("s_dlsym");
    None
}

/// Close the dynamic library handle `p_handle`.
pub extern "C" fn s_dlclose(_p_vfs: *mut ffi::sqlite3_vfs, _p_handle: *mut c_void) {
    trace!("s_dlclose");
}

/// # Safety
///
/// Populate the buffer pointed to by `z_buf_out` with `n_byte` bytes of random data.
pub unsafe extern "C" fn s_randomness(
    _p_vfs: *mut ffi::sqlite3_vfs,
    n_byte: c_int,
    z_buf_out: *mut c_char,
) -> c_int {
    trace!("s_randomness");
    let bytes = slice::from_raw_parts_mut(z_buf_out, n_byte as usize);
    let len = bytes.len();
    let mut retval: i32 = 0;
    let sgx_ret = ocall_fill_rand_bytes(&mut retval as *mut _, bytes.as_mut_ptr(), len);
    if retval != 0 {
        println!("vfs_err happened, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }

    len as c_int
}

/// Sleep for `n_micro` microseconds. Return the number of microseconds actually slept.
pub unsafe extern "C" fn s_sleep(_p_vfs: *mut ffi::sqlite3_vfs, n_micro: c_int) -> c_int {
    trace!("s_sleep");
    let mut retval: i32 = 0;
    let mut elapsed_time: i32 = 0;
    let sgx_ret = ocall_sleep(&mut retval as *mut _, n_micro as u64, &mut elapsed_time as *mut _);
    if retval != 0 {
        println!("vfs_err happened, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }
    elapsed_time as c_int
}

/// # Safety
///
/// Return the current time as a Julian Day number in `p_time_out`.
pub unsafe extern "C" fn s_current_time(
    _p_vfs: *mut ffi::sqlite3_vfs,
    p_time_out: *mut f64,
) -> c_int {
    trace!("s_current_time");
    let mut retval: i32 = 0;
    let mut cur_time: f64 = 0.0;
    let sgx_ret = ocall_cur_time(&mut retval as *mut _, &mut cur_time as *mut _);
    *p_time_out = cur_time;
    if retval != 0 {
        println!("vfs_err happened, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }
    ffi::SQLITE_OK
}

pub extern "C" fn s_get_last_error(
    _p_vfs: *mut ffi::sqlite3_vfs,
    _n_byte: c_int,
    _z_err_msg: *mut c_char,
) -> c_int {
    trace!("s_get_last_error");
    ffi::SQLITE_OK
}

/// # Safety
///
///
pub unsafe extern "C" fn s_current_time_int64(_p_vfs: *mut ffi::sqlite3_vfs, p: *mut i64) -> i32 {
    trace!("s_current_time_int64");
    let mut retval: i32 = 0;
    let mut cur_time: i64 = 0;
    let sgx_ret = ocall_cur_time_i64(&mut retval as *mut _, &mut cur_time as *mut _);
    *p = cur_time;
    if retval != 0 {
        println!("vfs_err happened, err code: {}", retval);
    }
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }
    ffi::SQLITE_OK
}

/// Register a virtual file system ([Vfs]) to SQLite.
pub fn register_server(name: &str, s_vfs: ServerVfs) -> Result<()> {
    let cstring_name = match CString::new(name) {
        Ok(n) => n,
        Err(_) => { 
            bail!("Cannot convert name to CString style"); 
        },
    };
    let name = ManuallyDrop::new(cstring_name);
    let io_methods = ffi::sqlite3_io_methods {
        iVersion: 3,
        xClose: Some(io::s_close),
        xRead: Some(io::s_read),
        xWrite: Some(io::s_write),
        xTruncate: Some(io::s_truncate),
        xSync: Some(io::s_sync),
        xFileSize: Some(io::s_file_size),
        xLock: Some(io::lock),
        xUnlock: Some(io::unlock),
        xCheckReservedLock: Some(io::check_reserved_lock),
        xFileControl: Some(io::file_control),
        xSectorSize: Some(io::sector_size),
        xDeviceCharacteristics: Some(io::device_characteristics),
        xShmMap: Some(io::shm_map),
        xShmLock: Some(io::shm_lock),
        xShmBarrier: Some(io::shm_barrier),
        xShmUnmap: Some(io::shm_unmap),
        xFetch: Some(io::mem_fetch),
        xUnfetch: Some(io::mem_unfetch),
    };

    let ptr = Box::into_raw(Box::new(ServerState {
        vfs: s_vfs,
        io_methods,
    }));

    // an object of sqlite3_vfs defines the interface between the SQLite core and the underlying OS
    let vfs = Box::into_raw(Box::new(sqlite3_vfs {
        iVersion: 3,
        szOsFile: size_of::<ServerFileState>() as i32, // size of subclassed sqlite3_file
        mxPathname: MAX_PATH_LENGTH as i32,            // max path length supported by VFS
        pNext: null_mut(), // next registered VFS (we do not need it since we consider one single VFS)
        zName: name.as_ptr(), // must be unique if multiple VFSs exist
        pAppData: ptr as _, // pointer to application-specific data (state)
        xOpen: Some(s_open),
        xDelete: Some(s_delete),
        xAccess: Some(s_access),
        xFullPathname: Some(s_full_pathname),
        xDlOpen: Some(s_dlopen),
        xDlError: Some(s_dlerror),
        xDlSym: Some(s_dlsym),
        xDlClose: Some(s_dlclose),
        xRandomness: Some(s_randomness),
        xSleep: Some(s_sleep),
        xCurrentTime: Some(s_current_time),
        xGetLastError: Some(s_get_last_error),
        xCurrentTimeInt64: Some(s_current_time_int64),
        xSetSystemCall: None,
        xGetSystemCall: None,
        xNextSystemCall: None,
    }));

    let result = unsafe { ffi::sqlite3_vfs_register(vfs, false as i32) };
    if result != ffi::SQLITE_OK {
        println!("err code: {:?}", result);
        bail!("register error");
    }
    Ok(())
}


#[no_mangle]
pub extern "C" fn sqlite3_os_init() -> c_int {
    trace!("sqlite3_os_init is called");

    let read_map = HashMap::new();
    let write_map = HashMap::new();
    let s_vfs = ServerVfs::new(MERKLE_PATH.to_string(), read_map, write_map);
    match register_server(SGX_VFS, s_vfs) {
        Ok(_) => {},
        Err(_) => {
            println!("failed to register vfs");
            return 1;
        },
    }
    ffi::SQLITE_OK
}

#[no_mangle]
pub extern "C" fn sqlite3_os_end() -> c_int {
    trace!("sqlite3_os_end is called");
    ffi::SQLITE_OK
}
