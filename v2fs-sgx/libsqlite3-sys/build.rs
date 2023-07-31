use bindgen::callbacks::{IntKind, ParseCallbacks};
use std::env;
use std::path::Path;

use std::fs::OpenOptions;
use std::io::Write;

#[derive(Debug)]
struct SqliteTypeChooser;

impl ParseCallbacks for SqliteTypeChooser {
    fn int_macro(&self, _name: &str, value: i64) -> Option<IntKind> {
        if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
            Some(IntKind::I32)
        } else {
            None
        }
    }
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("bindgen.rs");

    let common_flags = vec![
        "-DSQLITE_CORE",
        "-DSQLITE_DEFAULT_FOREIGN_KEYS=1",
        "-DSQLITE_ENABLE_API_ARMOR",
        "-DSQLITE_ENABLE_COLUMN_METADATA",
        "-DSQLITE_ENABLE_DBSTAT_VTAB",
        "-DSQLITE_ENABLE_FTS3",
        "-DSQLITE_ENABLE_FTS3_PARENTHESIS",
        "-DSQLITE_ENABLE_FTS5",
        "-DSQLITE_ENABLE_JSON1",
        "-DSQLITE_ENABLE_LOAD_EXTENSION=1",
        "-DSQLITE_ENABLE_MEMORY_MANAGEMENT",
        "-DSQLITE_ENABLE_RTREE",
        "-DSQLITE_ENABLE_STAT2",
        "-DSQLITE_ENABLE_STAT4",
        "-DSQLITE_SOUNDEX",
        "-DSQLITE_THREADSAFE=1",
        "-DSQLITE_OS_OTHER=1",
        "-DSQLITE_TEMP_STORE=3",
        "-DSQLITE_MAX_PATHLEN=256",
        "-DSQLITE_OMIT_LOCALTIME",
    ];
    let compile_flags = vec![
        "-m64",
        "-nostdinc",
        "-fvisibility=hidden",
        "-fpie",
        "-fstack-protector",
        "-I/home/comp/hxwang/sgx_vsqlite/rust-sgx-sdk/common/inc",
        "-I/opt/intel/sgxsdk/include",
        "-I/opt/intel/sgxsdk/include/tlibc",
    ];

    // bindgen
    let lib_name = "sqlite3";
    let header_path = get_header_path();
    let src_path = get_src_path();

    let header: String = header_path.into();
    let mut output = Vec::new();

    bindgen::builder()
        .header(header.clone())
        .clang_args(&common_flags)
        .parse_callbacks(Box::new(SqliteTypeChooser))
        .rustfmt_bindings(true)
        .layout_tests(false)
        .generate()
        .unwrap_or_else(|_| panic!("could not run bindgen on header {}", header))
        .write(Box::new(&mut output))
        .expect("could not write output of bindgen");
    let mut output = String::from_utf8(output).expect("bindgen output was not UTF-8?!");

    if !output.contains("pub const SQLITE_DETERMINISTIC") {
        output.push_str("\npub const SQLITE_DETERMINISTIC: i32 = 2048;\n");
    }

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&out_path)
        .unwrap_or_else(|_| panic!("Could not write to {:?}", out_path));

    file.write_all(output.as_bytes())
        .unwrap_or_else(|_| panic!("Could not write to {:?}", out_path));

    // compile and generate static lib
    println!("cargo:rerun-if-changed=sqlite3/sqlite3.c");
    let mut cfg = cc::Build::new();

    cfg.file(src_path)
        .warnings(false)
        .no_default_flags(true)
        .flag("-O2");

    for flag in &common_flags {
        cfg.flag(*flag);
    }
    for flag in compile_flags {
        cfg.flag(flag);
    }
    cfg.compile(lib_name); // generate $(lib_name).a

    println!("cargo:lib_dir={out_dir}");
}

fn get_cur_path() -> String {
    env::var("CARGO_MANIFEST_DIR").unwrap()
}

fn get_header_path() -> String {
    let cur_dir = get_cur_path();
    format!("{}/sqlite3/sqlite3.h", cur_dir)
}

fn get_src_path() -> String {
    let cur_dir = get_cur_path();
    format!("{}/sqlite3/sqlite3.c", cur_dir)
}
