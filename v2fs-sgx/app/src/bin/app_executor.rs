#[macro_use]
extern crate tracing;
extern crate sgx_types;
extern crate sgx_urts;
use app::init_enclave;
use sgx_types::*;
use app::{init_tracing_subscriber, Time};
use std::{fs::File, io::{BufReader, BufRead}};
use anyhow::{bail, Result};
use structopt::StructOpt;

extern "C" {
    pub fn ecall_exec(
        eid: sgx_enclave_id_t,
        retval: *mut sgx_status_t,
        stmt: *const u8,
        len: usize,
    ) -> sgx_status_t;
}

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(short, long, default_value = "./cmds/test_wkld.txt")]
    workload_path: String,
}

fn main() {
    init_tracing_subscriber("info").unwrap();

    let opts = Opt::from_args();
    let wkld_path = opts.workload_path;

    let stmts = load_stmts(wkld_path);
    execute_sql(&stmts).unwrap();
}

fn load_stmts(wkld_path: String) -> Vec<String> {
    let mut vec = Vec::new();
    let file = File::open(wkld_path).expect("failed to open file");
    let reader = BufReader::new(file);

    for line in reader.lines() {
        vec.push(line.unwrap());
    }

    vec
}


fn execute_sql(stmts: &Vec<String>) -> Result<()> {
    let enclave = match init_enclave() {
        Ok(r) => {
            println!("[+] Init Enclave Successful {}!", r.geteid());
            r
        }
        Err(x) => {
            println!("[-] Init Enclave Failed {}!", x.as_str());
            bail!("Failed to init enclave");
        }
    };

    let mut retval = sgx_status_t::SGX_SUCCESS;

    let bytes = match postcard::to_allocvec(&stmts) {
        Ok(buf) => buf,
        Err(_) => {
            bail!("postcard serialize for Vec<PageId> failed");
        }
    };

    let num = stmts.len() as f64;
    let timer = howlong::ProcessCPUTimer::new();

    let result = unsafe {
        ecall_exec(
            enclave.geteid(),
            &mut retval,
            bytes.as_ptr() as *const u8,
            bytes.len(),
        )
    };
    match result {
        sgx_status_t::SGX_SUCCESS => {}
        _ => {
            println!("[-] ECALL Enclave Failed {}!", result.as_str());
            bail!("Failed to call ecall");
        }
    }
    let time = Time::from(timer.elapsed());
    let avg_t = time.real as f64 / (num * 1000.0);
    info!("average time per update: {}ms", avg_t);
    
    println!("[+] ecall_exec success...");
    enclave.destroy();
    Ok(())
}
