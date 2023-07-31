use super::{io, MAX_PATH_LENGTH, MERKLE_PATH, SERVER_VFS};
use crate::digest::Digest;
use crate::merkle_cb_tree::write::WriteContext;
use crate::merkle_cb_tree::WriteInterface;
use crate::vbf::VersionBloomFilter;
use crate::vfs::{OpenOptions, TMP_FILE_PATH};
use crate::{MerkleDB, PageId, ServerVfs};
use anyhow::{bail, Context, Result};
use libsqlite3_sys as ffi;
use std::collections::HashMap;
use std::ffi::{c_void, CString};
use std::mem::{size_of, ManuallyDrop, MaybeUninit};
use std::path::Path;
use std::ptr::null_mut;
use std::time::{Duration, Instant};
use std::{
    ffi::CStr,
    fs::File,
    io::ErrorKind,
    os::raw::{c_char, c_int},
};
use std::{slice, thread};

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
    pub(crate) file: MaybeUninit<File>, // todo: use Option here
    pub(crate) merkle_db_path: String,
    pub(crate) map: &'a mut HashMap<PageId, Digest>,
    pub(crate) vbf: &'a mut VersionBloomFilter,
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
    flags: c_int,
    _p_out_flag: *mut c_int,
) -> c_int {
    debug!("s_open");
    let path;
    if z_name.is_null() {
        path = TMP_FILE_PATH.to_string();
        debug!("z_name is null");
    } else {
        path = CStr::from_ptr(z_name).to_string_lossy().to_string();
        debug!("opening {}", path);
    }

    let state = server_vfs_state(p_vfs).expect("null pointer");

    let opts = match OpenOptions::from_flags(flags) {
        Some(opts) => opts,
        None => {
            return ffi::SQLITE_CANTOPEN;
        }
    };

    let s_vfs = &mut state.vfs;
    let s_file = s_vfs
        .open(path.as_ref(), opts)
        .expect("failed to open path");
    let file_state = (p_file as *mut ServerFileState)
        .as_mut()
        .expect("null pointer");
    file_state.ctx.pMethods = &state.io_methods;
    file_state.merkle_db_path = s_vfs.merkle_db_path.clone();
    file_state.file.write(s_file);
    file_state.map = &mut s_vfs.map;
    file_state.vbf = &mut s_vfs.vbf;

    // todo: use option here will cause error due to unsuccessful assignment
    // debug!("{:?}", s_file);
    // file_state.file = Some(s_file);
    // let f = file_state.file.as_ref();
    // debug!("{:?}", f);
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

    match state.vfs.delete(path.as_ref()) {
        Ok(_) => ffi::SQLITE_OK,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                ffi::SQLITE_OK
            } else {
                ffi::SQLITE_DELETE
            }
        }
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

    let result = match flags {
        ffi::SQLITE_ACCESS_EXISTS => state.vfs.exists(path.as_ref()),
        ffi::SQLITE_ACCESS_READ => state.vfs.access(path.as_ref(), false),
        ffi::SQLITE_ACCESS_READWRITE => state.vfs.access(path.as_ref(), true),
        _ => return ffi::SQLITE_IOERR_ACCESS,
    };

    match result {
        Ok(ok) => {
            let p_res_out: &mut c_int = p_res_out.as_mut().expect("null pointer");
            *p_res_out = ok as i32;
        }
        Err(_) => {
            return ffi::SQLITE_IOERR_ACCESS;
        }
    }
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
    ffi::sqlite3_snprintf(n_byte, z_err_msg, msg.as_ptr() as _);
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
    use rand::Rng;

    trace!("s_randomness");
    let bytes = slice::from_raw_parts_mut(z_buf_out, n_byte as usize);
    rand::thread_rng().fill(bytes);
    bytes.len() as c_int
}

/// Sleep for `n_micro` microseconds. Return the number of microseconds actually slept.
pub extern "C" fn s_sleep(_p_vfs: *mut ffi::sqlite3_vfs, n_micro: c_int) -> c_int {
    trace!("s_sleep");
    let instant = Instant::now();
    thread::sleep(Duration::from_micros(n_micro as u64));
    instant.elapsed().as_micros() as c_int
}

/// # Safety
///
/// Return the current time as a Julian Day number in `p_time_out`.
pub unsafe extern "C" fn s_current_time(
    _p_vfs: *mut ffi::sqlite3_vfs,
    p_time_out: *mut f64,
) -> c_int {
    trace!("s_current_time");
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as f64;
    *p_time_out = 2440587.5 + now / 864.0e5;
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
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as f64;
    *p = ((2440587.5 + now / 864.0e5) * 864.0e5) as i64;
    ffi::SQLITE_OK
}

/// Register a virtual file system ([Vfs]) to SQLite.
pub fn register_server(name: &str, s_vfs: ServerVfs) -> Result<()> {
    let name = ManuallyDrop::new(CString::new(name)?);
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
    let vfs = Box::into_raw(Box::new(ffi::sqlite3_vfs {
        iVersion: 3,
        szOsFile: size_of::<ServerFileState>() as i32, // size of subclassed sqlite3_file
        mxPathname: MAX_PATH_LENGTH as i32,            // max path length supported by VFS
        pNext: null_mut(), // next registered VFS (we donot need it since we consider one single VFS)
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
        bail!("register error");
    }
    Ok(())
}

pub fn update_merkle_db() -> Result<()> {
    let name = ManuallyDrop::new(CString::new(SERVER_VFS)?);
    let map = unsafe {
        let p_vfs = libsqlite3_sys::sqlite3_vfs_find(name.as_ptr());
        let state = server_vfs_state(p_vfs).expect("null pointer");
        let s_vfs = &mut state.vfs;
        &mut s_vfs.map
    };
    let mut modif = Vec::new();
    for (p_id, dig) in map.drain() {
        modif.push((p_id, dig));
    }
    modif.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));

    let mut merkle_db =
        MerkleDB::create_new(Path::new(MERKLE_PATH)).expect("failed to open or create merkle db");
    let root_id = merkle_db.get_root_id();
    let mut ctx = WriteContext::new(&merkle_db, root_id);
    for (p_id, dig) in modif {
        ctx.update(dig, p_id).expect("Failed to update merkle tree");
    }
    let changes = ctx.changes();
    let new_root_id = changes.root_id;
    for (addr, node) in changes.nodes {
        merkle_db
            .write_node(&addr, &node)
            .expect("Failed to write node to merkle db");
    }
    merkle_db
        .update_param(new_root_id)
        .expect("Failed to update merkle root id in merkle db");
    merkle_db.close();

    Ok(())
}

// #[cfg(test)]
// mod tests {
//     use crate::{
//         utils::init_tracing_subscriber,
//         vfs::io::build_merkle_tree,
//     };
//     use anyhow::Result;

//     #[test]
//     fn test_build_mht_from_db() -> Result<()> {
//         init_tracing_subscriber("trace").unwrap();
//         build_merkle_tree().unwrap();
//         Ok(())
//     }

// }
