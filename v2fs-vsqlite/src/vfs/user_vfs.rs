use crate::{
    cache::Cache,
    digest::Digest,
    simple_vcache::SVCache,
    vbf::VersionBloomFilter,
    version_cache::VCache,
    vfs::{NAME_CNT, REMOTE_FLAG, TMP_FLAG},
    PageId, Type, UserVfs,
};
use anyhow::{bail, Context, Result};
use libsqlite3_sys as ffi;
use std::{
    collections::HashMap,
    ffi::{c_void, CStr, CString},
    fs::OpenOptions,
    mem::{size_of, ManuallyDrop, MaybeUninit},
    net::TcpStream,
    os::raw::{c_char, c_int},
    ptr::null_mut,
    slice, thread,
    time::{Duration, Instant},
};

use super::{io, FileData, MAX_PATH_LENGTH};

pub struct UserState<'a, 'b> {
    pub vfs: UserVfs<'a, 'b>,
    io_methods: ffi::sqlite3_io_methods,
}

#[derive(Debug)]
#[repr(C)]
pub(crate) struct UserFileState<'a, 'b> {
    pub(crate) ctx: ffi::sqlite3_file,
    pub(crate) tp: Type,
    pub(crate) cache: &'a mut Cache,
    pub(crate) vcache: &'a mut VCache,
    pub(crate) svcache: &'a mut SVCache,
    pub(crate) map: &'a mut HashMap<PageId, Digest>,
    pub(crate) stream: &'b mut TcpStream,
    pub(crate) tmp_file: MaybeUninit<FileData>,
    pub(crate) vbf: &'a VersionBloomFilter,
}

/// # Safety
///
/// this function gets the vfs state for user
pub unsafe fn user_vfs_state<'a, 'b>(
    ptr: *mut ffi::sqlite3_vfs,
) -> Result<&'a mut UserState<'a, 'b>> {
    let vfs: &mut ffi::sqlite3_vfs = ptr.as_mut().context("received null pointer")?;
    let state = (vfs.pAppData as *mut UserState)
        .as_mut()
        .context("received null pointer")?;
    Ok(state)
}

/// # Safety
///
/// Open a new file handler.
pub unsafe extern "C" fn u_open(
    p_vfs: *mut ffi::sqlite3_vfs,
    z_name: *const c_char,
    p_file: *mut ffi::sqlite3_file,
    _flags: c_int,
    _p_out_flag: *mut c_int,
) -> c_int {
    let state = user_vfs_state(p_vfs).expect("null pointer");
    let u_vfs = &mut state.vfs;

    let u_file_data = if z_name.is_null() {
        let surfix = NAME_CNT.to_string();
        let path = "./db/tmp_file".to_string() + &surfix;
        debug!("z_name is null, open {}", path);
        NAME_CNT += 1;
        FileData {
            id: TMP_FLAG,
            name: path.clone(),
            file: OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
                .expect("failed to open tmp file"),
        }
    } else {
        debug!("z_name is not null, open remote file");
        let path = CStr::from_ptr(z_name).to_string_lossy().to_string();
        debug!("create holder file: {}", path);
        FileData {
            id: REMOTE_FLAG,
            name: path.clone(),
            file: OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
                .expect("failed to open holder file"),
        }
    };
    let file_state = (p_file as *mut UserFileState)
        .as_mut()
        .expect("null pointer");
    file_state.ctx.pMethods = &state.io_methods;
    file_state.tp = u_vfs.tp;
    file_state.cache = &mut u_vfs.cache;
    file_state.vcache = &mut u_vfs.vcache;
    file_state.svcache = &mut u_vfs.svcache;
    file_state.map = &mut u_vfs.map;
    file_state.stream = &mut u_vfs.stream;
    file_state.tmp_file.write(u_file_data);
    file_state.vbf = &u_vfs.vbf;

    trace!("open succeeds");
    ffi::SQLITE_OK
}

/// # Safety
///
/// Delete file for user. This function should never be called
pub unsafe extern "C" fn u_delete(
    _p_vfs: *mut ffi::sqlite3_vfs,
    _z_path: *const c_char,
    _sync_dir: c_int,
) -> c_int {
    trace!("user delete");
    panic!("Should not reach user delete");
}

/// # Safety
///
/// Test for access permissions. Return true if the requested permission is available, or false otherwise.
pub unsafe extern "C" fn u_access(
    _p_vfs: *mut ffi::sqlite3_vfs,
    _z_path: *const c_char,
    flags: c_int,
    p_res_out: *mut c_int,
) -> c_int {
    trace!("user access");

    let result = match flags {
        ffi::SQLITE_ACCESS_EXISTS => 0,
        ffi::SQLITE_ACCESS_READ => 0,
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
pub unsafe extern "C" fn u_full_pathname(
    _p_vfs: *mut ffi::sqlite3_vfs,
    z_path: *const c_char,
    n_out: c_int,
    z_out: *mut c_char,
) -> c_int {
    trace!("user full p_name");
    let name = CStr::from_ptr(z_path);
    let name = name.to_bytes_with_nul();
    if name.len() > n_out as usize {
        return ffi::SQLITE_ERROR;
    }
    let out = slice::from_raw_parts_mut(z_out as *mut u8, name.len());
    out.copy_from_slice(name);

    ffi::SQLITE_OK
}

/// # Safety
///
/// Open the dynamic library located at `z_path` and return a handle.
pub unsafe extern "C" fn u_dlopen(
    _p_vfs: *mut ffi::sqlite3_vfs,
    _z_path: *const c_char,
) -> *mut c_void {
    trace!("user dlopen");
    null_mut()
}

/// # Safety
///
/// Populate the buffer `z_err_msg` (size `n_byte` bytes) with a human readable utf-8 string describing the most recent error encountered associated with dynamic libraries.
pub unsafe extern "C" fn u_dlerror(
    _p_vfs: *mut ffi::sqlite3_vfs,
    n_byte: c_int,
    z_err_msg: *mut c_char,
) {
    trace!("user dlerror");
    let msg = concat!("Loadable extensions are not supported", "\0");
    ffi::sqlite3_snprintf(n_byte, z_err_msg, msg.as_ptr() as _);
}

/// # Safety
///
/// Return a pointer to the symbol `z_sym` in the dynamic library pHandle.
pub unsafe extern "C" fn u_dlsym(
    _p_vfs: *mut ffi::sqlite3_vfs,
    _p: *mut c_void,
    _z_sym: *const c_char,
) -> Option<unsafe extern "C" fn(*mut ffi::sqlite3_vfs, *mut c_void, *const c_char)> {
    trace!("user u_dlsym");
    None
}

/// # Safety
///
/// Close the dynamic library handle `p_handle`.
pub unsafe extern "C" fn u_dlclose(_p_vfs: *mut ffi::sqlite3_vfs, _p_handle: *mut c_void) {
    trace!("user u_dlclose");
}

/// # Safety
///
/// Populate the buffer pointed to by `z_buf_out` with `n_byte` bytes of random data.
pub unsafe extern "C" fn u_randomness(
    _p_vfs: *mut ffi::sqlite3_vfs,
    n_byte: c_int,
    z_buf_out: *mut c_char,
) -> c_int {
    use rand::Rng;

    trace!("user u_randomness");
    let bytes = slice::from_raw_parts_mut(z_buf_out, n_byte as usize);
    rand::thread_rng().fill(bytes);
    bytes.len() as c_int
}

/// # Safety
///
/// Sleep for `n_micro` microseconds. Return the number of microseconds actually slept.
pub unsafe extern "C" fn u_sleep(_p_vfs: *mut ffi::sqlite3_vfs, n_micro: c_int) -> c_int {
    trace!("user u_sleep");
    let instant = Instant::now();
    thread::sleep(Duration::from_micros(n_micro as u64));
    instant.elapsed().as_micros() as c_int
}

/// # Safety
///
/// Return the current time as a Julian Day number in `p_time_out`.
pub unsafe extern "C" fn u_current_time(
    _p_vfs: *mut ffi::sqlite3_vfs,
    p_time_out: *mut f64,
) -> c_int {
    trace!("user u_current_time");
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as f64;
    *p_time_out = 2440587.5 + now / 864.0e5;
    ffi::SQLITE_OK
}

/// # Safety
///
pub unsafe extern "C" fn u_get_last_error(
    _p_vfs: *mut ffi::sqlite3_vfs,
    _n_byte: c_int,
    _z_err_msg: *mut c_char,
) -> c_int {
    trace!("user u_get_last_error");
    ffi::SQLITE_OK
}

/// # Safety
///
pub unsafe extern "C" fn u_current_time_int64(_p_vfs: *mut ffi::sqlite3_vfs, p: *mut i64) -> i32 {
    trace!("user u_current_time_int64");
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as f64;
    *p = ((2440587.5 + now / 864.0e5) * 864.0e5) as i64;
    ffi::SQLITE_OK
}

/// Register a virtual file system ([Vfs]) to SQLite.
pub fn register_user(name: &str, u_vfs: UserVfs) -> Result<()> {
    let name = ManuallyDrop::new(CString::new(name)?);

    let io_methods = ffi::sqlite3_io_methods {
        iVersion: 3,
        xClose: Some(io::u_close),
        xRead: Some(io::u_read),
        xWrite: Some(io::u_write),
        xTruncate: Some(io::u_truncate),
        xSync: Some(io::u_sync),
        xFileSize: Some(io::u_file_size),
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

    let ptr = Box::into_raw(Box::new(UserState {
        vfs: u_vfs,
        io_methods,
    }));

    // an object of sqlite3_vfs defines the interface between the SQLite core and the underlying OS
    let vfs = Box::into_raw(Box::new(ffi::sqlite3_vfs {
        iVersion: 3,
        szOsFile: size_of::<UserFileState>() as i32, // size of subclassed sqlite3_file
        mxPathname: MAX_PATH_LENGTH as i32,          // max path length supported by VFS
        pNext: null_mut(), // next registered VFS (we do need it since we consider one single VFS)
        zName: name.as_ptr(), // must be unique if multiple VFSs exist
        pAppData: ptr as _, // pointer to application-specific data (state)
        xOpen: Some(u_open),
        xDelete: Some(u_delete),
        xAccess: Some(u_access),
        xFullPathname: Some(u_full_pathname),
        xDlOpen: Some(u_dlopen),
        xDlError: Some(u_dlerror),
        xDlSym: Some(u_dlsym),
        xDlClose: Some(u_dlclose),
        xRandomness: Some(u_randomness),
        xSleep: Some(u_sleep),
        xCurrentTime: Some(u_current_time),
        xGetLastError: Some(u_get_last_error),
        xCurrentTimeInt64: Some(u_current_time_int64),
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
