#[macro_use]
extern crate tracing;

use anyhow::{bail, Result};
use std::{
    collections::HashSet,
    fs::File,
    io::{ErrorKind, Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
};
use v2fs_vsqlite::{
    digest::{Digest, Digestible},
    merkle_cb_tree::{read::ReadContext, NodeId, ReadInterface},
    utils::init_tracing_subscriber,
    vfs::{
        BOTH_CACHE, CONFIRM, END, MAIN_PATH, MERKLE_PATH, NO_CACHE, NO_FLAG, PAGE_SIZE, QUERY,
        YES_FLAG,
    },
    MerkleDB, PageId,
};

fn handle_sender(mut stream: TcpStream) -> Result<()> {
    let merkle_db = MerkleDB::open_read_only(Path::new(MERKLE_PATH))?;
    let root_id = merkle_db.get_root_id();
    let ctx = ReadContext::new(&merkle_db, root_id)?;

    let mut buf = [0; PAGE_SIZE as usize];
    let _bytes_read = stream.read(&mut buf)?;
    let flag = bincode::deserialize::<u32>(&buf)?;
    if flag == NO_CACHE {
        let _w_amt = stream.write(&YES_FLAG.to_le_bytes())?;
        // hand_shake finished
        handle_no_cache(&mut stream, ctx)?;
    } else if flag == BOTH_CACHE {
        let _w_amt = stream.write(&YES_FLAG.to_le_bytes())?;
        // hand_shake finished
        handle_both_cache(&mut stream, ctx, &merkle_db)?;
    }

    Ok(())
}

fn handle_both_cache(
    stream: &mut TcpStream,
    mut ctx: ReadContext<MerkleDB>,
    merkle_db: &MerkleDB,
) -> Result<()> {
    let mut queried_page_ids = HashSet::new();

    loop {
        let mut buff = [0; PAGE_SIZE as usize];
        let _bytes_read = stream.read(&mut buff)?;
        let (flag, p_id, digs) = bincode::deserialize::<(u32, PageId, Vec<Digest>)>(&buff)?;
        if flag == END {
            trace!("end flag received");
            // query finished, generate proof
            let p1 = ctx.into_proof();

            let bytes = bincode::serialize(&p1)?;
            let _w_amt = stream.write(&bytes)?;

            break;
        } else if flag == CONFIRM {
            trace!("confirm flag received, the id is {}", p_id);
            let (match_flag, pos) = confirm(p_id, &digs, merkle_db)?;

            if match_flag {
                // if match, return YES_FLAG, then (h, w)
                trace!("match, return YES_FLAG, then (h, w)");

                let _w_amt = stream.write(&YES_FLAG.to_le_bytes())?;
                let mut buff = [0; PAGE_SIZE as usize];
                let _bytes_read = stream.read(&mut buff)?;
                let _receipt = bincode::deserialize::<u32>(&buff)?;

                let transfer_data = pos;
                let bytes = bincode::serialize(&transfer_data).expect("failed to serialize");
                let _w_amt = stream.write(&bytes)?;

                if !queried_page_ids.contains(&p_id) {
                    ctx.query(p_id)?;
                }
                queried_page_ids.insert(p_id);
                
            } else {
                // if not match, return NO_FLAG, then bytes
                trace!("not match, return NO_FLAG, then bytes");
                let _w_amt = stream.write(&NO_FLAG.to_le_bytes())?;
                let mut buff = [0; PAGE_SIZE as usize];
                let _bytes_read = stream.read(&mut buff)?;
                let _receipt = bincode::deserialize::<u32>(&buff)?;
                let bytes = query_page(p_id);
                if !queried_page_ids.contains(&p_id) {
                    ctx.query(p_id)?;
                }
                queried_page_ids.insert(p_id);
                let _w_amt = stream.write(&bytes)?;
            }
        } else if flag == QUERY {
            trace!("query flag received, the id is {}", p_id);
            ctx.query(p_id)?;
            queried_page_ids.insert(p_id);
            let p_cont = query_page(p_id);
            let _w_amt = stream.write(&p_cont)?;
        } else {
            trace!("invalid signal received: {}, {}, {:?}", flag, p_id, digs);
            bail!("Invalid signal.");
        }  
    }

    Ok(())
}

fn confirm(p_id: PageId, digs: &Vec<Digest>, merkle_db: &MerkleDB) -> Result<(bool, (u32, u32))> {
    let mut cur_id = NodeId::from_page_id(p_id);
    let mut flag = false;
    let mut pos = (0, 0);
    for dig in digs {
        let n = merkle_db
            .get_node(&cur_id.to_digest())?
            .expect("node not exist");
        let hash = n.get_hash();
        if hash == *dig {
            pos = (cur_id.get_height(), cur_id.get_width());
            flag = true;
        } else {
            break;
        }
        cur_id = cur_id.get_parent_id();
    }
    Ok((flag, pos))
}

fn handle_no_cache(stream: &mut TcpStream, mut ctx: ReadContext<MerkleDB>) -> Result<()> {
    trace!("handle no cache");
    loop {
        let mut buff = [0; PAGE_SIZE as usize];
        let _bytes_read = stream.read(&mut buff)?;
        let (flag, p_id, digs) = bincode::deserialize::<(u32, PageId, Vec<Digest>)>(&buff)?;
        trace!("received flag: {}, p_id: {}, digs: {:?}", flag, p_id, digs);
        if flag == END {
            trace!("query finished, generate proof");
            // query finished, generate proof
            let p = ctx.into_proof();
            let bytes = bincode::serialize(&p)?;
            let _w_amt = stream.write(&bytes)?;
            break;
        } else if flag == QUERY {
            trace!("query page {}...", p_id);
            // query page         
            ctx.query(p_id)?;
            let p_cont = query_page(p_id);
            let _w_amt = stream.write(&p_cont)?;
            trace!("page bytes has been sent to user");
        } else {
            bail!("Invalid signal");
        }
    }

    Ok(())
}

fn query_page(p_id: PageId) -> [u8; PAGE_SIZE as usize] {
    let mut file = File::open(Path::new(MAIN_PATH)).expect("failed to open file");
    let ofst = p_id.get_id() as u64 * PAGE_SIZE as u64;
    let mut buf: [u8; PAGE_SIZE as usize] = [0; PAGE_SIZE as usize];
    match file.seek(SeekFrom::Start(ofst)) {
        Ok(o) => {
            if o != ofst {
                warn!("sqlite seek error");
            }
        }
        Err(_) => {
            warn!("sqlite seek error");
        }
    }
    if let Err(err) = file.read_exact(&mut buf) {
        let kind = err.kind();
        if kind == ErrorKind::UnexpectedEof {
            warn!("file length not enough");
        } else {
            warn!("sqlite io err");
        }
    }
    buf
}

fn main() -> Result<()> {
    init_tracing_subscriber("info")?;
    // Enable port 7878 binding
    let receiver_listener =
        TcpListener::bind("127.0.0.1:7878").expect("Failed and bind with the sender");
    // Getting a handle of the underlying thread.
    // listen to incoming connections messages and bind them to a sever socket address.
    for stream in receiver_listener.incoming() {
        let stream = stream.expect("failed");
        // let the receiver connect with the sender
        let _handle = thread::spawn(move || {
            //receiver failed to read from the stream
            handle_sender(stream).unwrap_or_else(|error| eprintln!("{:?}", error))
        });
    }
    Ok(())
}
