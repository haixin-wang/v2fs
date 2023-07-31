# V^2FS

An implementation of Verifiable Virtual Filesystem (V^2FS) based on SQLite database engine.

**WARNING**: This is an academic proof-of-concept prototype, and in particular has not received careful code review. The implementation is NOT ready for production use.

`v2fs-sgx` contains the source code for `V^2FS CI`, which maintains the database update securely using SGX. User-guide is contained in `v2fs-sgx/README.md`. It needs an SGX-enabled CPU for execution. If you do not have the SGX device, you may consider using MHT building simulation mentioned in `v2fs-vsqlite/README.md`.

`v2fs-vsqlite` provides the functionalities of query processing and verification. Please refer to `v2fs-vsqlite/README.md` for more detailed instructions to execute the code.

