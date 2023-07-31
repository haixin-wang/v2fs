// Part of the implementation of vfs is motivated by https://github.com/rkusa/sqlite-vfs
pub mod io;
pub mod server_vfs;
pub mod user_vfs;

use crate::{
    digest::{Digest, Digestible},
    PageId,
};
use libsqlite3_sys as ffi;
use std::{cmp::Ordering, fs::File};

pub(crate) const MAX_PATH_LENGTH: usize = 512;
pub const PAGE_SIZE: u32 = 4096;

pub const MERKLE_PATH: &str = "./db/merkle_db/merkle_test";

pub const MAIN_PATH: &str = "./db/sqlite_db/test.db";

pub const TMP_FILE_PATH: &str = "./db/tmp_file";
pub const HOLDER_FILE_PATH: &str = "./db/holder_file";
pub const SERVER_VFS: &str = "server_vfs";
pub const USER_VFS: &str = "user_vfs";

// flags for tmp file processing
pub const REMOTE_FLAG: usize = 101;
pub const TMP_FLAG: usize = 100;

// signal from user
pub const NO_CACHE: u32 = 10;
pub const BOTH_CACHE: u32 = 11;
pub const DEFAULT: u32 = 12;
pub const END: u32 = u32::MAX;
pub const CONFIRM: u32 = u32::MAX - 1;
pub const QUERY: u32 = u32::MAX - 2;

pub static mut NAME_CNT: u8 = 0;

// response from server
pub const YES_FLAG: u32 = 28;
pub const NO_FLAG: u32 = 29;

pub static mut GLOBAL_TS: u32 = 1;

#[derive(Debug, PartialEq, Eq)]
pub struct Page {
    p_id: PageId,
    bytes: Box<[u8; PAGE_SIZE as usize]>,
}

impl Page {
    pub fn new(p_id: PageId, bytes: Box<[u8; PAGE_SIZE as usize]>) -> Self {
        Self { p_id, bytes }
    }
}

impl Digestible for Page {
    fn to_digest(&self) -> Digest {
        self.bytes.to_digest()
    }
}

impl Ord for Page {
    fn cmp(&self, other: &Self) -> Ordering {
        self.p_id.cmp(&other.p_id)
    }
}

impl PartialOrd for Page {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.p_id.partial_cmp(&other.p_id) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.bytes.partial_cmp(&other.bytes)
    }
}

#[derive(Debug)]
pub struct FileData {
    id: usize,
    name: String,
    file: File,
}

impl FileData {
    pub fn get_id(&self) -> usize {
        self.id
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OpenOptions {
    /// The object type that is being opened.
    pub kind: OpenKind,

    /// The access an object is opened with.
    pub access: OpenAccess,

    /// The file should be deleted when it is closed.
    pub delete_on_close: bool,
}

impl OpenOptions {
    pub fn from_flags(flags: i32) -> Option<Self> {
        Some(OpenOptions {
            kind: OpenKind::from_flags(flags)?,
            access: OpenAccess::from_flags(flags)?,
            delete_on_close: flags & ffi::SQLITE_OPEN_DELETEONCLOSE > 0,
        })
    }
}

/// The object type being opened.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OpenKind {
    MainDb,
    MainJournal,
    TempDb,
    TempJournal,
    TransientDb,
    SubJournal,
    SuperJournal,
    Wal,
}

impl OpenKind {
    pub fn from_flags(flags: i32) -> Option<Self> {
        match flags {
            flags if flags & ffi::SQLITE_OPEN_MAIN_DB > 0 => Some(Self::MainDb),
            flags if flags & ffi::SQLITE_OPEN_MAIN_JOURNAL > 0 => Some(Self::MainJournal),
            flags if flags & ffi::SQLITE_OPEN_TEMP_DB > 0 => Some(Self::TempDb),
            flags if flags & ffi::SQLITE_OPEN_TEMP_JOURNAL > 0 => Some(Self::TempJournal),
            flags if flags & ffi::SQLITE_OPEN_TRANSIENT_DB > 0 => Some(Self::TransientDb),
            flags if flags & ffi::SQLITE_OPEN_SUBJOURNAL > 0 => Some(Self::SubJournal),
            flags if flags & ffi::SQLITE_OPEN_SUPER_JOURNAL > 0 => Some(Self::SuperJournal),
            flags if flags & ffi::SQLITE_OPEN_WAL > 0 => Some(Self::Wal),
            _ => None,
        }
    }
}

/// The access an object is opened with.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OpenAccess {
    /// Read access.
    Read,

    /// Write access (includes read access).
    Write,

    /// Create the file if it does not exist (includes write and read access).
    Create,

    /// Create the file, but failing if it it already exist (includes write and read access).
    CreateNew,
}

impl OpenAccess {
    pub fn from_flags(flags: i32) -> Option<Self> {
        match flags {
            flags
                if (flags & ffi::SQLITE_OPEN_CREATE > 0)
                    && (flags & ffi::SQLITE_OPEN_EXCLUSIVE > 0) =>
            {
                Some(Self::CreateNew)
            }
            flags if flags & ffi::SQLITE_OPEN_CREATE > 0 => Some(Self::Create),
            flags if flags & ffi::SQLITE_OPEN_READWRITE > 0 => Some(Self::Write),
            flags if flags & ffi::SQLITE_OPEN_READONLY > 0 => Some(Self::Read),
            _ => None,
        }
    }
}
