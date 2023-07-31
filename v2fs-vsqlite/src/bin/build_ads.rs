use anyhow::Result;
use v2fs_vsqlite::{utils::init_tracing_subscriber, vfs::io::build_merkle_tree};

fn main() -> Result<()> {
    init_tracing_subscriber("trace")?;
    build_merkle_tree()?;
    Ok(())
}