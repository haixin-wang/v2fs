# V^2FS -- Database Maintenance with SGX
The project needs SGX-enabled CPU for execution. If you do not have one, you may consider using MHT building simulation mentioned in `v2fs-vsqlite/README.md`.

* Make sure you have SGX installed properly.
* Optional: Update MERKLE_PATH and MAIN_PATH at `./src/vfs.rs` to your preferred path, or keep them as the default values.
* Update the following lines to the corresponding paths on your machine:
* * `line 1` of `./Makefile`
* * `line 21` of `./app/src/lib.rs`
* * `line 53` of `./libsqlite3-sys/build.rs`
* Put your commands inside a .txt file seperated by `\n`, or you can use our provided test queries at `./cmds/test_wkld.txt`, which contains commands to create a test table and insert some records.
* Run `make clean`, then `make`.
* Run `./target/release/app_executor`. The SQLite database and Merkle tree will be generated to MAIN_PATH and MERKLE_PATH, respectively.