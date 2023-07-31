#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(not(target_env = "sgx"))]
#[macro_use]
extern crate sgx_tstd as std;

pub use libsqlite3_sys as ffi;

use std::cell::RefCell;
use std::default::Default;
use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_char, c_int};
use std::string::String;

use std::path::Path;
use std::result;
use std::str;
use std::sync::atomic::Ordering;
use std::sync::{Arc, SgxMutex};

use crate::cache::StatementCache;
use crate::inner_connection::{InnerConnection, BYPASS_SQLITE_INIT};
use crate::raw_statement::RawStatement;
use crate::types::ValueRef;

pub use crate::cache::CachedStatement;
pub use crate::column::Column;
pub use crate::error::Error;
pub use crate::ffi::ErrorCode;
pub use crate::params::{params_from_iter, Params, ParamsFromIter};
pub use crate::row::{AndThenRows, Map, MappedRows, Row, RowIndex, Rows};
pub use crate::statement::{Statement, StatementStatus};
pub use crate::transaction::{DropBehavior, Savepoint, Transaction, TransactionBehavior};
pub use crate::types::ToSql;
pub use crate::version::*;

mod busy;
mod cache;
mod column;
pub mod config;
mod error;
mod inner_connection;
mod params;
mod pragma;
mod raw_statement;
mod row;
mod statement;
mod transaction;
pub mod types;
mod version;

pub(crate) mod util;
pub(crate) use util::SmallCString;

// Number of cached prepared statements we'll hold on to.
const STATEMENT_CACHE_DEFAULT_CAPACITY: usize = 16;
/// To be used when your statement has no [parameter][sqlite-varparam].
///
/// [sqlite-varparam]: https://sqlite.org/lang_expr.html#varparam
///
/// This is deprecated in favor of using an empty array literal.
#[deprecated = "Use an empty array instead; `stmt.execute(NO_PARAMS)` => `stmt.execute([])`"]
pub const NO_PARAMS: &[&dyn ToSql] = &[];

/// A macro making it more convenient to longer lists of
/// parameters as a `&[&dyn ToSql]`.
///
/// # Example
///
/// ```rust,no_run
/// # use rusqlite::{Result, Connection, params};
///
/// struct Person {
///     name: String,
///     age_in_years: u8,
///     data: Option<Vec<u8>>,
/// }
///
/// fn add_person(conn: &Connection, person: &Person) -> Result<()> {
///     conn.execute(
///         "INSERT INTO person(name, age_in_years, data) VALUES (?1, ?2, ?3)",
///         params![person.name, person.age_in_years, person.data],
///     )?;
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! params {
    () => {
        &[] as &[&dyn $crate::ToSql]
    };
    ($($param:expr),+ $(,)?) => {
        &[$(&$param as &dyn $crate::ToSql),+] as &[&dyn $crate::ToSql]
    };
}

/// A macro making it more convenient to pass lists of named parameters
/// as a `&[(&str, &dyn ToSql)]`.
///
/// # Example
///
/// ```rust,no_run
/// # use rusqlite::{Result, Connection, named_params};
///
/// struct Person {
///     name: String,
///     age_in_years: u8,
///     data: Option<Vec<u8>>,
/// }
///
/// fn add_person(conn: &Connection, person: &Person) -> Result<()> {
///     conn.execute(
///         "INSERT INTO person (name, age_in_years, data)
///          VALUES (:name, :age, :data)",
///         named_params! {
///             ":name": person.name,
///             ":age": person.age_in_years,
///             ":data": person.data,
///         },
///     )?;
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! named_params {
    () => {
        &[] as &[(&str, &dyn $crate::ToSql)]
    };
    // Note: It's a lot more work to support this as part of the same macro as
    // `params!`, unfortunately.
    ($($param_name:literal: $param_val:expr),+ $(,)?) => {
        &[$(($param_name, &$param_val as &dyn $crate::ToSql)),+] as &[(&str, &dyn $crate::ToSql)]
    };
}

/// A typedef of the result returned by many methods.
pub type Result<T, E = Error> = result::Result<T, E>;

/// See the [method documentation](#tymethod.optional).
pub trait OptionalExtension<T> {
    /// Converts a `Result<T>` into a `Result<Option<T>>`.
    ///
    /// By default, Rusqlite treats 0 rows being returned from a query that is
    /// expected to return 1 row as an error. This method will
    /// handle that error, and give you back an `Option<T>` instead.
    fn optional(self) -> Result<Option<T>>;
}

impl<T> OptionalExtension<T> for Result<T> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

unsafe fn errmsg_to_string(errmsg: *const c_char) -> String {
    let c_slice = CStr::from_ptr(errmsg).to_bytes();
    String::from_utf8_lossy(c_slice).into_owned()
}

fn str_to_cstring(s: &str) -> Result<SmallCString> {
    Ok(SmallCString::new(s)?)
}

/// Returns `Ok((string ptr, len as c_int, SQLITE_STATIC | SQLITE_TRANSIENT))`
/// normally.
/// Returns error if the string is too large for sqlite.
/// The `sqlite3_destructor_type` item is always `SQLITE_TRANSIENT` unless
/// the string was empty (in which case it's `SQLITE_STATIC`, and the ptr is
/// static).
fn str_for_sqlite(s: &[u8]) -> Result<(*const c_char, c_int, ffi::sqlite3_destructor_type)> {
    let len = len_as_c_int(s.len())?;
    let (ptr, dtor_info) = if len != 0 {
        (s.as_ptr().cast::<c_char>(), ffi::SQLITE_TRANSIENT())
    } else {
        // Return a pointer guaranteed to live forever
        ("".as_ptr().cast::<c_char>(), ffi::SQLITE_STATIC())
    };
    Ok((ptr, len, dtor_info))
}

// Helper to cast to c_int safely, returning the correct error type if the cast
// failed.
fn len_as_c_int(len: usize) -> Result<c_int> {
    if len >= (c_int::MAX as usize) {
        Err(Error::SqliteFailure(
            ffi::Error::new(ffi::SQLITE_TOOBIG),
            None,
        ))
    } else {
        Ok(len as c_int)
    }
}

#[cfg(unix)]
fn path_to_cstring(p: &Path) -> Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    Ok(CString::new(p.as_os_str().as_bytes())?)
}

/// Name for a database within a SQLite connection.
#[derive(Copy, Clone, Debug)]
pub enum DatabaseName<'a> {
    /// The main database.
    Main,

    /// The temporary database (e.g., any "CREATE TEMPORARY TABLE" tables).
    Temp,

    /// A database that has been attached via "ATTACH DATABASE ...".
    Attached(&'a str),
}

/// Shorthand for [`DatabaseName::Main`].
pub const MAIN_DB: DatabaseName<'static> = DatabaseName::Main;

/// Shorthand for [`DatabaseName::Temp`].
pub const TEMP_DB: DatabaseName<'static> = DatabaseName::Temp;

// Currently DatabaseName is only used by the backup and blob mods, so hide
// this (private) impl to avoid dead code warnings.
impl DatabaseName<'_> {
    #[inline]
    fn as_cstring(&self) -> Result<SmallCString> {
        use self::DatabaseName::{Attached, Main, Temp};
        match *self {
            Main => str_to_cstring("main"),
            Temp => str_to_cstring("temp"),
            Attached(s) => str_to_cstring(s),
        }
    }
}

/// A connection to a SQLite database.
pub struct Connection {
    db: RefCell<InnerConnection>,
    cache: StatementCache,
}

unsafe impl Send for Connection {}

impl Drop for Connection {
    #[inline]
    fn drop(&mut self) {
        self.flush_prepared_statement_cache();
    }
}

impl Connection {
    /// Open a new connection to a SQLite database. If a database does not exist
    /// at the path, one is created.
    ///
    /// ```rust,no_run
    /// # use rusqlite::{Connection, Result};
    /// fn open_my_db() -> Result<()> {
    ///     let path = "./my_db.db3";
    ///     let db = Connection::open(path)?;
    ///     // Use the database somehow...
    ///     println!("{}", db.is_autocommit());
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Flags
    ///
    /// `Connection::open(path)` is equivalent to using
    /// [`Connection::open_with_flags`] with the default [`OpenFlags`]. That is,
    /// it's equivalent to:
    ///
    /// ```ignore
    /// Connection::open_with_flags(
    ///     path,
    ///     OpenFlags::SQLITE_OPEN_READ_WRITE
    ///         | OpenFlags::SQLITE_OPEN_CREATE
    ///         | OpenFlags::SQLITE_OPEN_URI
    ///         | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    /// )
    /// ```
    ///
    /// These flags have the following effects:
    ///
    /// - Open the database for both reading or writing.
    /// - Create the database if one does not exist at the path.
    /// - Allow the filename to be interpreted as a URI (see <https://www.sqlite.org/uri.html#uri_filenames_in_sqlite>
    ///   for details).
    /// - Disables the use of a per-connection mutex.
    ///
    ///     Rusqlite enforces thread-safety at compile time, so additional
    ///     locking is not needed and provides no benefit. (See the
    ///     documentation on [`OpenFlags::SQLITE_OPEN_FULL_MUTEX`] for some
    ///     additional discussion about this).
    ///
    /// Most of these are also the default settings for the C API, although
    /// technically the default locking behavior is controlled by the flags used
    /// when compiling SQLite -- rather than let it vary, we choose `NO_MUTEX`
    /// because it's a fairly clearly the best choice for users of this library.
    ///
    /// # Failure
    ///
    /// Will return `Err` if `path` cannot be converted to a C-compatible string
    /// or if the underlying SQLite open call fails.
    #[inline]
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Connection> {
        let flags = OpenFlags::default();
        Connection::open_with_flags(path, flags)
    }

    /// Open a new connection to an in-memory SQLite database.
    ///
    /// # Failure
    ///
    /// Will return `Err` if the underlying SQLite open call fails.
    #[inline]
    pub fn open_in_memory() -> Result<Connection> {
        let flags = OpenFlags::default();
        Connection::open_in_memory_with_flags(flags)
    }

    /// Open a new connection to a SQLite database.
    ///
    /// [Database Connection](http://www.sqlite.org/c3ref/open.html) for a description of valid
    /// flag combinations.
    ///
    /// # Failure
    ///
    /// Will return `Err` if `path` cannot be converted to a C-compatible
    /// string or if the underlying SQLite open call fails.
    #[inline]
    pub fn open_with_flags<P: AsRef<Path>>(path: P, flags: OpenFlags) -> Result<Connection> {
        let c_path = path_to_cstring(path.as_ref())?;
        InnerConnection::open_with_flags(&c_path, flags, None).map(|db| Connection {
            db: RefCell::new(db),
            cache: StatementCache::with_capacity(STATEMENT_CACHE_DEFAULT_CAPACITY),
        })
    }

    /// Open a new connection to a SQLite database using the specific flags and
    /// vfs name.
    ///
    /// [Database Connection](http://www.sqlite.org/c3ref/open.html) for a description of valid
    /// flag combinations.
    ///
    /// # Failure
    ///
    /// Will return `Err` if either `path` or `vfs` cannot be converted to a
    /// C-compatible string or if the underlying SQLite open call fails.
    #[inline]
    pub fn open_with_flags_and_vfs<P: AsRef<Path>>(
        path: P,
        flags: OpenFlags,
        vfs: &str,
    ) -> Result<Connection> {
        let c_path = path_to_cstring(path.as_ref())?;
        let c_vfs = str_to_cstring(vfs)?;
        InnerConnection::open_with_flags(&c_path, flags, Some(&c_vfs)).map(|db| Connection {
            db: RefCell::new(db),
            cache: StatementCache::with_capacity(STATEMENT_CACHE_DEFAULT_CAPACITY),
        })
    }

    /// Open a new connection to an in-memory SQLite database.
    ///
    /// [Database Connection](http://www.sqlite.org/c3ref/open.html) for a description of valid
    /// flag combinations.
    ///
    /// # Failure
    ///
    /// Will return `Err` if the underlying SQLite open call fails.
    #[inline]
    pub fn open_in_memory_with_flags(flags: OpenFlags) -> Result<Connection> {
        Connection::open_with_flags(":memory:", flags)
    }

    /// Open a new connection to an in-memory SQLite database using the specific
    /// flags and vfs name.
    ///
    /// [Database Connection](http://www.sqlite.org/c3ref/open.html) for a description of valid
    /// flag combinations.
    ///
    /// # Failure
    ///
    /// Will return `Err` if `vfs` cannot be converted to a C-compatible
    /// string or if the underlying SQLite open call fails.
    #[inline]
    pub fn open_in_memory_with_flags_and_vfs(flags: OpenFlags, vfs: &str) -> Result<Connection> {
        Connection::open_with_flags_and_vfs(":memory:", flags, vfs)
    }

    /// Convenience method to run multiple SQL statements (that cannot take any
    /// parameters).
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// # use rusqlite::{Connection, Result};
    /// fn create_tables(conn: &Connection) -> Result<()> {
    ///     conn.execute_batch(
    ///         "BEGIN;
    ///          CREATE TABLE foo(x INTEGER);
    ///          CREATE TABLE bar(y TEXT);
    ///          COMMIT;",
    ///     )
    /// }
    /// ```
    ///
    /// # Failure
    ///
    /// Will return `Err` if `sql` cannot be converted to a C-compatible string
    /// or if the underlying SQLite call fails.
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        let mut sql = sql;
        while !sql.is_empty() {
            let stmt = self.prepare(sql)?;
            if !stmt.stmt.is_null() && stmt.step()? && cfg!(feature = "extra_check") {
                // Some PRAGMA may return rows
                return Err(Error::ExecuteReturnedResults);
            }
            let tail = stmt.stmt.tail();
            if tail == 0 || tail >= sql.len() {
                break;
            }
            sql = &sql[tail..];
        }
        Ok(())
    }

    /// Convenience method to prepare and execute a single SQL statement.
    ///
    /// On success, returns the number of rows that were changed or inserted or
    /// deleted (via `sqlite3_changes`).
    ///
    /// ## Example
    ///
    /// ### With positional params
    ///
    /// ```rust,no_run
    /// # use rusqlite::{Connection};
    /// fn update_rows(conn: &Connection) {
    ///     match conn.execute("UPDATE foo SET bar = 'baz' WHERE qux = ?1", [1i32]) {
    ///         Ok(updated) => println!("{} rows were updated", updated),
    ///         Err(err) => println!("update failed: {}", err),
    ///     }
    /// }
    /// ```
    ///
    /// ### With positional params of varying types
    ///
    /// ```rust,no_run
    /// # use rusqlite::{params, Connection};
    /// fn update_rows(conn: &Connection) {
    ///     match conn.execute(
    ///         "UPDATE foo SET bar = 'baz' WHERE qux = ?1 AND quux = ?2",
    ///         params![1i32, 1.5f64],
    ///     ) {
    ///         Ok(updated) => println!("{} rows were updated", updated),
    ///         Err(err) => println!("update failed: {}", err),
    ///     }
    /// }
    /// ```
    ///
    /// ### With named params
    ///
    /// ```rust,no_run
    /// # use rusqlite::{Connection, Result};
    /// fn insert(conn: &Connection) -> Result<usize> {
    ///     conn.execute(
    ///         "INSERT INTO test (name) VALUES (:name)",
    ///         &[(":name", "one")],
    ///     )
    /// }
    /// ```
    ///
    /// # Failure
    ///
    /// Will return `Err` if `sql` cannot be converted to a C-compatible string
    /// or if the underlying SQLite call fails.
    #[inline]
    pub fn execute<P: Params>(&self, sql: &str, params: P) -> Result<usize> {
        self.prepare(sql)
            .and_then(|mut stmt| stmt.check_no_tail().and_then(|_| stmt.execute(params)))
    }

    /// Returns the path to the database file, if one exists and is known.
    ///
    /// Returns `Some("")` for a temporary or in-memory database.
    ///
    /// Note that in some cases [PRAGMA
    /// database_list](https://sqlite.org/pragma.html#pragma_database_list) is
    /// likely to be more robust.
    #[inline]
    pub fn path(&self) -> Option<&str> {
        unsafe {
            let db = self.handle();
            let db_name = DatabaseName::Main.as_cstring().unwrap();
            let db_filename = ffi::sqlite3_db_filename(db, db_name.as_ptr());
            if db_filename.is_null() {
                None
            } else {
                CStr::from_ptr(db_filename).to_str().ok()
            }
        }
    }

    /// Convenience method to prepare and execute a single SQL statement with
    /// named parameter(s).
    ///
    /// On success, returns the number of rows that were changed or inserted or
    /// deleted (via `sqlite3_changes`).
    ///
    /// # Failure
    ///
    /// Will return `Err` if `sql` cannot be converted to a C-compatible string
    /// or if the underlying SQLite call fails.
    #[deprecated = "You can use `execute` with named params now."]
    pub fn execute_named(&self, sql: &str, params: &[(&str, &dyn ToSql)]) -> Result<usize> {
        // This function itself is deprecated, so it's fine
        #![allow(deprecated)]
        self.prepare(sql).and_then(|mut stmt| {
            stmt.check_no_tail()
                .and_then(|_| stmt.execute_named(params))
        })
    }

    /// Get the SQLite rowid of the most recent successful INSERT.
    ///
    /// Uses [sqlite3_last_insert_rowid](https://www.sqlite.org/c3ref/last_insert_rowid.html) under
    /// the hood.
    #[inline]
    pub fn last_insert_rowid(&self) -> i64 {
        self.db.borrow_mut().last_insert_rowid()
    }

    /// Convenience method to execute a query that is expected to return a
    /// single row.
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// # use rusqlite::{Result, Connection};
    /// fn preferred_locale(conn: &Connection) -> Result<String> {
    ///     conn.query_row(
    ///         "SELECT value FROM preferences WHERE name='locale'",
    ///         [],
    ///         |row| row.get(0),
    ///     )
    /// }
    /// ```
    ///
    /// If the query returns more than one row, all rows except the first are
    /// ignored.
    ///
    /// Returns `Err(QueryReturnedNoRows)` if no results are returned. If the
    /// query truly is optional, you can call `.optional()` on the result of
    /// this to get a `Result<Option<T>>`.
    ///
    /// # Failure
    ///
    /// Will return `Err` if `sql` cannot be converted to a C-compatible string
    /// or if the underlying SQLite call fails.
    #[inline]
    pub fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> Result<T>,
    {
        let mut stmt = self.prepare(sql)?;
        stmt.check_no_tail()?;
        stmt.query_row(params, f)
    }

    /// Convenience method to execute a query with named parameter(s) that is
    /// expected to return a single row.
    ///
    /// If the query returns more than one row, all rows except the first are
    /// ignored.
    ///
    /// Returns `Err(QueryReturnedNoRows)` if no results are returned. If the
    /// query truly is optional, you can call `.optional()` on the result of
    /// this to get a `Result<Option<T>>`.
    ///
    /// # Failure
    ///
    /// Will return `Err` if `sql` cannot be converted to a C-compatible string
    /// or if the underlying SQLite call fails.
    #[deprecated = "You can use `query_row` with named params now."]
    pub fn query_row_named<T, F>(&self, sql: &str, params: &[(&str, &dyn ToSql)], f: F) -> Result<T>
    where
        F: FnOnce(&Row<'_>) -> Result<T>,
    {
        self.query_row(sql, params, f)
    }

    /// Convenience method to execute a query that is expected to return a
    /// single row, and execute a mapping via `f` on that returned row with
    /// the possibility of failure. The `Result` type of `f` must implement
    /// `std::convert::From<Error>`.
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// # use rusqlite::{Result, Connection};
    /// fn preferred_locale(conn: &Connection) -> Result<String> {
    ///     conn.query_row_and_then(
    ///         "SELECT value FROM preferences WHERE name='locale'",
    ///         [],
    ///         |row| row.get(0),
    ///     )
    /// }
    /// ```
    ///
    /// If the query returns more than one row, all rows except the first are
    /// ignored.
    ///
    /// # Failure
    ///
    /// Will return `Err` if `sql` cannot be converted to a C-compatible string
    /// or if the underlying SQLite call fails.
    #[inline]
    pub fn query_row_and_then<T, E, P, F>(&self, sql: &str, params: P, f: F) -> Result<T, E>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> Result<T, E>,
        E: From<Error>,
    {
        let mut stmt = self.prepare(sql)?;
        stmt.check_no_tail()?;
        let mut rows = stmt.query(params)?;

        rows.get_expected_row().map_err(E::from).and_then(f)
    }

    /// Prepare a SQL statement for execution.
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// # use rusqlite::{Connection, Result};
    /// fn insert_new_people(conn: &Connection) -> Result<()> {
    ///     let mut stmt = conn.prepare("INSERT INTO People (name) VALUES (?1)")?;
    ///     stmt.execute(["Joe Smith"])?;
    ///     stmt.execute(["Bob Jones"])?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Failure
    ///
    /// Will return `Err` if `sql` cannot be converted to a C-compatible string
    /// or if the underlying SQLite call fails.
    #[inline]
    pub fn prepare(&self, sql: &str) -> Result<Statement<'_>> {
        self.db.borrow_mut().prepare(self, sql)
    }

    /// Close the SQLite connection.
    ///
    /// This is functionally equivalent to the `Drop` implementation for
    /// `Connection` except that on failure, it returns an error and the
    /// connection itself (presumably so closing can be attempted again).
    ///
    /// # Failure
    ///
    /// Will return `Err` if the underlying SQLite call fails.
    #[inline]
    pub fn close(self) -> Result<(), (Connection, Error)> {
        self.flush_prepared_statement_cache();
        let r = self.db.borrow_mut().close();
        r.map_err(move |err| (self, err))
    }

    /// Get access to the underlying SQLite database connection handle.
    ///
    /// # Warning
    ///
    /// You should not need to use this function. If you do need to, please
    /// [open an issue on the rusqlite repository](https://github.com/rusqlite/rusqlite/issues) and describe
    /// your use case.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it gives you raw access
    /// to the SQLite connection, and what you do with it could impact the
    /// safety of this `Connection`.
    #[inline]
    pub unsafe fn handle(&self) -> *mut ffi::sqlite3 {
        self.db.borrow().db()
    }

    /// Create a `Connection` from a raw handle.
    ///
    /// The underlying SQLite database connection handle will not be closed when
    /// the returned connection is dropped/closed.
    ///
    /// # Safety
    ///
    /// This function is unsafe because improper use may impact the Connection.
    #[inline]
    pub unsafe fn from_handle(db: *mut ffi::sqlite3) -> Result<Connection> {
        let db = InnerConnection::new(db, false);
        Ok(Connection {
            db: RefCell::new(db),
            cache: StatementCache::with_capacity(STATEMENT_CACHE_DEFAULT_CAPACITY),
        })
    }

    /// Get access to a handle that can be used to interrupt long running
    /// queries from another thread.
    #[inline]
    pub fn get_interrupt_handle(&self) -> InterruptHandle {
        self.db.borrow().get_interrupt_handle()
    }

    #[inline]
    fn decode_result(&self, code: c_int) -> Result<()> {
        self.db.borrow().decode_result(code)
    }

    /// Return the number of rows modified, inserted or deleted by the most
    /// recently completed INSERT, UPDATE or DELETE statement on the database
    /// connection.
    ///
    /// See <https://www.sqlite.org/c3ref/changes.html>
    #[inline]
    pub fn changes(&self) -> u64 {
        self.db.borrow().changes()
    }

    /// Test for auto-commit mode.
    /// Autocommit mode is on by default.
    #[inline]
    pub fn is_autocommit(&self) -> bool {
        self.db.borrow().is_autocommit()
    }

    /// Determine if all associated prepared statements have been reset.
    #[inline]
    pub fn is_busy(&self) -> bool {
        self.db.borrow().is_busy()
    }

    /// Flush caches to disk mid-transaction
    pub fn cache_flush(&self) -> Result<()> {
        self.db.borrow_mut().cache_flush()
    }

    /// Determine if a database is read-only
    pub fn is_readonly(&self, db_name: DatabaseName<'_>) -> Result<bool> {
        self.db.borrow().db_readonly(db_name)
    }
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("path", &self.path())
            .finish()
    }
}

/// Batch iterator
/// ```rust
/// use rusqlite::{Batch, Connection, Result};
///
/// fn main() -> Result<()> {
///     let conn = Connection::open_in_memory()?;
///     let sql = r"
///     CREATE TABLE tbl1 (col);
///     CREATE TABLE tbl2 (col);
///     ";
///     let mut batch = Batch::new(&conn, sql);
///     while let Some(mut stmt) = batch.next()? {
///         stmt.execute([])?;
///     }
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct Batch<'conn, 'sql> {
    conn: &'conn Connection,
    sql: &'sql str,
    tail: usize,
}

impl<'conn, 'sql> Batch<'conn, 'sql> {
    /// Constructor
    pub fn new(conn: &'conn Connection, sql: &'sql str) -> Batch<'conn, 'sql> {
        Batch { conn, sql, tail: 0 }
    }

    /// Iterates on each batch statements.
    ///
    /// Returns `Ok(None)` when batch is completed.
    #[allow(clippy::should_implement_trait)] // fallible iterator
    pub fn next(&mut self) -> Result<Option<Statement<'conn>>> {
        while self.tail < self.sql.len() {
            let sql = &self.sql[self.tail..];
            let next = self.conn.prepare(sql)?;
            let tail = next.stmt.tail();
            if tail == 0 {
                self.tail = self.sql.len();
            } else {
                self.tail += tail;
            }
            if next.stmt.is_null() {
                continue;
            }
            return Ok(Some(next));
        }
        Ok(None)
    }
}

impl<'conn> Iterator for Batch<'conn, '_> {
    type Item = Result<Statement<'conn>>;

    fn next(&mut self) -> Option<Result<Statement<'conn>>> {
        self.next().transpose()
    }
}

bitflags::bitflags! {
    /// Flags for opening SQLite database connections. See
    /// [sqlite3_open_v2](http://www.sqlite.org/c3ref/open.html) for details.
    ///
    /// The default open flags are `SQLITE_OPEN_READ_WRITE | SQLITE_OPEN_CREATE
    /// | SQLITE_OPEN_URI | SQLITE_OPEN_NO_MUTEX`. See [`Connection::open`] for
    /// some discussion about these flags.
    #[repr(C)]
    pub struct OpenFlags: ::std::os::raw::c_int {
        /// The database is opened in read-only mode.
        /// If the database does not already exist, an error is returned.
        const SQLITE_OPEN_READ_ONLY = ffi::SQLITE_OPEN_READONLY;
        /// The database is opened for reading and writing if possible,
        /// or reading only if the file is write protected by the operating system.
        /// In either case the database must already exist, otherwise an error is returned.
        const SQLITE_OPEN_READ_WRITE = ffi::SQLITE_OPEN_READWRITE;
        /// The database is created if it does not already exist
        const SQLITE_OPEN_CREATE = ffi::SQLITE_OPEN_CREATE;
        /// The filename can be interpreted as a URI if this flag is set.
        const SQLITE_OPEN_URI = ffi::SQLITE_OPEN_URI;
        /// The database will be opened as an in-memory database.
        const SQLITE_OPEN_MEMORY = ffi::SQLITE_OPEN_MEMORY;
        /// The new database connection will not use a per-connection mutex (the
        /// connection will use the "multi-thread" threading mode, in SQLite
        /// parlance).
        ///
        /// This is used by default, as proper `Send`/`Sync` usage (in
        /// particular, the fact that [`Connection`] does not implement `Sync`)
        /// ensures thread-safety without the need to perform locking around all
        /// calls.
        const SQLITE_OPEN_NO_MUTEX = ffi::SQLITE_OPEN_NOMUTEX;
        /// The new database connection will use a per-connection mutex -- the
        /// "serialized" threading mode, in SQLite parlance.
        ///
        /// # Caveats
        ///
        /// This flag should probably never be used with `rusqlite`, as we
        /// ensure thread-safety statically (we implement [`Send`] and not
        /// [`Sync`]). That said
        ///
        /// Critically, even if this flag is used, the [`Connection`] is not
        /// safe to use across multiple threads simultaneously. To access a
        /// database from multiple threads, you should either create multiple
        /// connections, one for each thread (if you have very many threads,
        /// wrapping the `rusqlite::Connection` in a mutex is also reasonable).
        ///
        /// This is both because of the additional per-connection state stored
        /// by `rusqlite` (for example, the prepared statement cache), and
        /// because not all of SQLites functions are fully thread safe, even in
        /// serialized/`SQLITE_OPEN_FULLMUTEX` mode.
        ///
        /// All that said, it's fairly harmless to enable this flag with
        /// `rusqlite`, it will just slow things down while providing no
        /// benefit.
        const SQLITE_OPEN_FULL_MUTEX = ffi::SQLITE_OPEN_FULLMUTEX;
        /// The database is opened with shared cache enabled.
        ///
        /// This is frequently useful for in-memory connections, but note that
        /// broadly speaking it's discouraged by SQLite itself, which states
        /// "Any use of shared cache is discouraged" in the official
        /// [documentation](https://www.sqlite.org/c3ref/enable_shared_cache.html).
        const SQLITE_OPEN_SHARED_CACHE = 0x0002_0000;
        /// The database is opened shared cache disabled.
        const SQLITE_OPEN_PRIVATE_CACHE = 0x0004_0000;
        /// The database filename is not allowed to be a symbolic link. (3.31.0)
        const SQLITE_OPEN_NOFOLLOW = 0x0100_0000;
        /// Extended result codes. (3.37.0)
        const SQLITE_OPEN_EXRESCODE = 0x0200_0000;
    }
}

impl Default for OpenFlags {
    #[inline]
    fn default() -> OpenFlags {
        // Note: update the `Connection::open` and top-level `OpenFlags` docs if
        // you change these.
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI
    }
}

/// rusqlite's check for a safe SQLite threading mode requires SQLite 3.7.0 or
/// later. If you are running against a SQLite older than that, rusqlite
/// attempts to ensure safety by performing configuration and initialization of
/// SQLite itself the first time you
/// attempt to open a connection. By default, rusqlite panics if that
/// initialization fails, since that could mean SQLite has been initialized in
/// single-thread mode.
///
/// If you are encountering that panic _and_ can ensure that SQLite has been
/// initialized in either multi-thread or serialized mode, call this function
/// prior to attempting to open a connection and rusqlite's initialization
/// process will by skipped.
///
/// # Safety
///
/// This function is unsafe because if you call it and SQLite has actually been
/// configured to run in single-thread mode,
/// you may encounter memory errors or data corruption or any number of terrible
/// things that should not be possible when you're using Rust.
pub unsafe fn bypass_sqlite_initialization() {
    BYPASS_SQLITE_INIT.store(true, Ordering::Relaxed);
}

/// Allows interrupting a long-running computation.
pub struct InterruptHandle {
    db_lock: Arc<SgxMutex<*mut ffi::sqlite3>>,
}

unsafe impl Send for InterruptHandle {}
unsafe impl Sync for InterruptHandle {}

impl InterruptHandle {
    /// Interrupt the query currently executing on another thread. This will
    /// cause that query to fail with a `SQLITE3_INTERRUPT` error.
    pub fn interrupt(&self) {
        let db_handle = self.db_lock.lock().unwrap();
        if !db_handle.is_null() {
            unsafe { ffi::sqlite3_interrupt(*db_handle) }
        }
    }
}
