use std::{env, path::PathBuf, process::Command};

fn main() {
    let sgx_sdk_dir =
        PathBuf::from(env::var("SGX_SDK").unwrap_or_else(|_| "/opt/sgxsdk".to_string()));
    let sgx_mode = env::var("SGX_MODE").unwrap_or_else(|_| "HW".to_string());
    let root_dir = {
        let cur_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        cur_dir.parent().unwrap().to_owned()
    };
    let rust_sgx_sdk_dir = root_dir.join("rust-sgx-sdk");
    let sgx_edl_file = root_dir.join("enclave/Enclave.edl");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let enclave_dir = root_dir.join("target").join(env::var("PROFILE").unwrap());

    // set cargo return-if
    println!("cargo:return-if-env-changed=SGX_MODE");
    println!(
        "cargo:return-if-env-changed={}",
        sgx_edl_file.to_string_lossy()
    );

    // set cargo rustc-env
    println!(
        "cargo:rustc-env=ENCLAVE_FILE_DIR={}",
        enclave_dir.to_string_lossy()
    );
    match sgx_mode.as_ref() {
        "SW" => {
            println!("cargo:rustc-env=ENCLAVE_FILE_NAME=libvsqlite_enclave_sim.signed.so");
        }
        _ => {
            println!("cargo:rustc-env=ENCLAVE_FILE_NAME=libvsqlite_enclave.signed.so");
        }
    }

    // generate Enclave_u src
    let status = Command::new(sgx_sdk_dir.join("bin/x64/sgx_edger8r"))
        .arg("--untrusted")
        .arg(sgx_edl_file)
        .arg("--search-path")
        .arg(sgx_sdk_dir.join("include"))
        .arg("--search-path")
        .arg(rust_sgx_sdk_dir.join("edl"))
        .arg("--untrusted-dir")
        .arg(&out_dir)
        .status()
        .expect("Failed to execute sgx_edger8r.");
    assert!(status.success(), "Failed to generate Enclave_u src.");

    // build Enclave_u
    let mut cc_builder = cc::Build::new();
    match env::var("PROFILE").unwrap().as_ref() {
        "debug" => {
            cc_builder.flag("-O0").flag("-g");
        }
        _ => {
            cc_builder.flag("-O2");
        }
    }
    cc_builder
        .no_default_flags(true)
        .file(out_dir.join("Enclave_u.c"))
        .flag("-fPIC")
        .flag("-Wno-attributes")
        .include(sgx_sdk_dir.join("include"))
        .include(rust_sgx_sdk_dir.join("edl"));
    cc_builder.compile("libEnclave_u.a");

    // generate bindings
    let bindings = bindgen::Builder::default()
        .header(out_dir.join("Enclave_u.h").to_string_lossy())
        .clang_arg(format!(
            "-I{}",
            sgx_sdk_dir.join("include").to_string_lossy()
        ))
        .clang_arg(format!(
            "-I{}",
            rust_sgx_sdk_dir.join("edl").to_string_lossy()
        ))
        .allowlist_recursively(false)
        .allowlist_function(".*_ecall")
        .allowlist_function("ecall_.*")
        .generate()
        .expect("Failed to generate bindings for Enclave_u.h");
    bindings
        .write_to_file(out_dir.join("enclave_ffi.rs"))
        .expect("Failed to write enclave_ffi.rs");

    // link lib
    println!("cargo:rustc-link-lib=static=Enclave_u");
    println!(
        "cargo:rustc-link-search=native={}",
        sgx_sdk_dir.join("lib64").to_string_lossy()
    );
    match sgx_mode.as_ref() {
        "SW" => {
            println!("cargo:rustc-link-lib=dylib=sgx_urts_sim");
            println!("cargo:rustc-link-lib=dylib=sgx_uae_service_sim");
        }
        _ => {
            println!("cargo:rustc-link-lib=dylib=sgx_urts");
            println!("cargo:rustc-link-lib=dylib=sgx_uae_service");
        }
    }
}
