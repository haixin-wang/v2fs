# V^2FS -- Query Processing

## Build Project

* Install Rust from <https://rustup.rs>.
* Optional: Update MERKLE_PATH and MAIN_PATH at `./src/vfs.rs` to your preferred path, or keep them as the default values.
* Run `cargo test` for unit test.
* Run `cargo build --release` to build the binaries, which will be located at `./target/release/`folder.

## Build Merkle Hash Tree


### Option 1: using the default database and MHT
* We provide a test database and MHT at `./db/sqlite_db/test.db` and `./db/merkle_db/merkle_test`, which contains the tables in TPC-H benchmark with a small scale factor. You can directly use them for query processing without any other setting.

### Option 2: simulate the MHT building
* If you want to use your own SQLite database, update MAIN_PATH and MERKLE_PATH as your target database and then run `./target/release/build_ads`.

### Option 3: using SGX to securely build MHT
* If you have SGX-enabled CPU, you can use v2fs_sgx project to build the MHT securely.


## Query Processing & Verification
* Put your SQLite queries inside a .txt file seperated by `;`, or you can use our provided test queries at `./query/test_wkld.txt`, which contains several TPC-H queries.
* Run `./target/release/server` to start the server
* Use `client` to process queries & verify results. You need to specifiy the following parameters:
* * `-c`: cache size in MB, default value is `500`.
* * `-o`: optimization level, `0` means no optimization; `1` means applying intra-query cache; `2` means applying inter-query cache; `3` means applying inter-query cache with versioned bloom filter.
* * `-w`: path for query workload, default value is `./query/test_wkld.txt`.
* * `-m`: slot of versioned bloom filter, default value is `10000`.
* * `-h`: hash number for versioned bloom filter, default value is 5.

For example:
```
./target/release/client -c 500 -o 3 -w ./query/test_wkld.txt -m 10000 -h 5
```

Run `./target/release/client --help` for more information.


