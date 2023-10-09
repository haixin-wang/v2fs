use crate::{
    cache::{leaf::CacheLeafNode, Cache, CacheNode},
    digest::{Digest, Digestible},
    merkle_cb_tree::{write::WriteContext, NodeId, WriteInterface},
    simple_vcache::{SVCache, SVCacheNode},
    vbf::VersionBloomFilter,
    version_cache::{VCache, VCacheNode},
    vfs::{
        server_vfs::ServerFileState, user_vfs::UserFileState, GLOBAL_TS, MERKLE_PATH, QUERY,
        REMOTE_FLAG, TMP_FILE_PATH, TMP_FLAG, YES_FLAG,
    },
    MerkleDB, PageId,
};
use anyhow::{bail, Context, Result};
use libsqlite3_sys as ffi;
use std::{
    collections::HashMap,
    ffi::c_void,
    fs::File,
    io::{BufReader, ErrorKind, Read, Seek, SeekFrom, Write},
    mem::{self, MaybeUninit},
    net::TcpStream,
    os::raw::c_int,
    path::Path,
    slice,
};

use super::{FileData, Page, CONFIRM, MAIN_PATH, PAGE_SIZE};

unsafe fn s_get_file<'a>(ptr: *mut ffi::sqlite3_file) -> Result<&'a mut File> {
    let file_state = (ptr as *mut ServerFileState)
        .as_mut()
        .context("null pointer")?;
    let file = file_state.file.assume_init_mut();
    // let file = file_state.file.as_mut().context("File in server file state not exist")?;
    Ok(file)
}

unsafe fn u_get_file<'a>(ptr: *mut ffi::sqlite3_file) -> Result<&'a mut FileData> {
    let file_state = (ptr as *mut UserFileState)
        .as_mut()
        .context("null pointer")?;
    let file = file_state.tmp_file.assume_init_mut();
    // let file = file_state.file.as_mut().context("File in server file state not exist")?;
    Ok(file)
}

unsafe fn s_get_map<'a>(ptr: *mut ffi::sqlite3_file) -> Result<&'a mut HashMap<PageId, Digest>> {
    let file_state = (ptr as *mut ServerFileState)
        .as_mut()
        .context("null pointer")?;
    let map = &mut file_state.map;
    Ok(map)
}

unsafe fn s_get_vbf<'a>(ptr: *mut ffi::sqlite3_file) -> Result<&'a mut VersionBloomFilter> {
    let file_state = (ptr as *mut ServerFileState)
        .as_mut()
        .context("null pointer")?;
    let vbf = &mut file_state.vbf;
    Ok(vbf)
}

/// # Safety
///
/// Server reads data from a file.
pub unsafe extern "C" fn s_read(
    p_file: *mut ffi::sqlite3_file,
    z_buf: *mut c_void,
    i_amt: c_int,
    i_ofst: ffi::sqlite3_int64,
) -> c_int {
    // starting from ofst, read i_amt length bytes to z_buf
    trace!("read offset={} len={}", i_ofst, i_amt);

    let file = s_get_file(p_file).expect("failed to get file in ServerFileState");

    // move the cursor to the offset
    match file.seek(SeekFrom::Start(i_ofst as u64)) {
        Ok(o) => {
            if o != i_ofst as u64 {
                return ffi::SQLITE_IOERR_READ;
            }
        }
        Err(_) => {
            return ffi::SQLITE_IOERR_READ;
        }
    }
    let out = slice::from_raw_parts_mut(z_buf as *mut u8, i_amt as usize);

    if let Err(err) = file.read_exact(out) {
        let kind = err.kind();
        if kind == ErrorKind::UnexpectedEof {
            // if len not enough, sqlite will fill with 0s
            return ffi::SQLITE_IOERR_SHORT_READ;
        } else {
            return ffi::SQLITE_IOERR_READ;
        }
    }

    ffi::SQLITE_OK
}

/// # Safety
///
/// User reads data from a server.
pub unsafe extern "C" fn u_read(
    p_file: *mut ffi::sqlite3_file,
    z_buf: *mut c_void,
    i_amt: c_int,
    i_ofst: ffi::sqlite3_int64,
) -> c_int {
    // starting from ofst, read i_amt length bytes to z_buf
    trace!("read offset={} len={}", i_ofst, i_amt);

    let file_data = u_get_file(p_file).expect("failed to get file in ServerFileState");
    let file_id = file_data.get_id();

    if file_id == TMP_FLAG {
        trace!("read tmp file");
        let file = &mut file_data.file;
        // move the cursor to the offset
        match file.seek(SeekFrom::Start(i_ofst as u64)) {
            Ok(o) => {
                if o != i_ofst as u64 {
                    return ffi::SQLITE_IOERR_READ;
                }
            }
            Err(_) => {
                return ffi::SQLITE_IOERR_READ;
            }
        }
        let out = slice::from_raw_parts_mut(z_buf as *mut u8, i_amt as usize);

        if let Err(err) = file.read_exact(out) {
            let kind = err.kind();
            if kind == ErrorKind::UnexpectedEof {
                // if len not enough, sqlite will fill with 0s
                return ffi::SQLITE_IOERR_SHORT_READ;
            } else {
                return ffi::SQLITE_IOERR_READ;
            }
        }
    } else if file_id == REMOTE_FLAG {
        trace!("read remote content");
        let file_state = (p_file as *mut UserFileState)
            .as_mut()
            .context("null pointer")
            .expect("");
        let tp = &file_state.tp;
        let stream = &mut file_state.stream;
        let map = &mut file_state.map;
        let (ofst, p_ids) = compute_page_ids(i_ofst as u64, i_amt as u64);

        let start_p = i_ofst as usize - ofst as usize;
        let end_p = start_p + i_amt as usize - 1;
        debug!("i_amt: {}, i_ofst: {}", i_amt, i_ofst);
        debug!("start_p: {}, end_p: {}", start_p, end_p);

        let mut pages = Vec::new();
        match tp {
            crate::Type::None => {
                process_without_cache(stream, p_ids, &mut pages, map);
            }
            crate::Type::Intra => {
                let cache = &mut file_state.cache;
                process_intra_cache(cache, stream, p_ids, &mut pages, map);
            }
            crate::Type::Both => {
                let cache = &mut file_state.cache;
                process_both_cache(cache, stream, p_ids, &mut pages, map);
            }
            crate::Type::BothBloom => {
                let vcache = &mut file_state.vcache;
                let vbf = &file_state.vbf;
                process_both_bloom(vcache, stream, p_ids, &mut pages, map, vbf);
            }
            crate::Type::SimpleBloom => {
                let svcache = &mut file_state.svcache;
                let vbf = &file_state.vbf;
                process_simply_bloom(svcache, stream, p_ids, &mut pages, map, vbf);
            }
        }

        pages.sort();
        let pages_iter = pages.iter().map(|x| x.bytes.to_vec());
        // let pages_bytes = pages_iter.fold(Vec::<u8>::new(), |acc, x| -> Vec<u8> {debug!("{}", hex::encode(&x)); [acc, x].concat()});
        let pages_bytes = pages_iter.fold(Vec::<u8>::new(), |acc, x| [acc, x].concat());
        let mut pages_cont_slice = &pages_bytes[start_p..(end_p + 1)];

        let out = slice::from_raw_parts_mut(z_buf as *mut u8, i_amt as usize);

        if let Err(err) = pages_cont_slice.read_exact(out) {
            let kind = err.kind();
            if kind == ErrorKind::UnexpectedEof {
                // if len not enough, sqlite will fill with 0s
                return ffi::SQLITE_IOERR_SHORT_READ;
            } else {
                return ffi::SQLITE_IOERR_READ;
            }
        }
    } else {
        panic!("should have an id");
    }

    trace!("u_read finish");

    ffi::SQLITE_OK
}

fn process_without_cache(
    stream: &mut TcpStream,
    p_ids: Vec<PageId>,
    pages: &mut Vec<Page>,
    map: &mut HashMap<PageId, Digest>,
) {
    for p_id in p_ids {
        let bytes = require_page(p_id, stream);
        map.insert(p_id, bytes.to_digest());
        let page = Page::new(p_id, Box::new(bytes));
        pages.push(page);
    }
}

fn pid_to_key(p_id: PageId) -> Digest {
    let n_id = NodeId::from_page_id(p_id);
    n_id.to_digest()
}

fn process_intra_cache(
    cache: &mut Cache,
    stream: &mut TcpStream,
    p_ids: Vec<PageId>,
    pages: &mut Vec<Page>,
    map: &mut HashMap<PageId, Digest>,
) {
    for p_id in p_ids {
        let key = pid_to_key(p_id);
        if let Some(n) = cache.get_node(&key) {
            match n {
                CacheNode::Leaf(l) => {
                    pages.push(Page::new(p_id, l.get_bytes()));
                }
                CacheNode::NonLeaf(_) => {
                    panic!("Impossible to get non-leaf node when only intra-cache is allowed")
                }
            }
        } else {
            let bytes = require_page(p_id, stream);
            map.insert(p_id, bytes.to_digest());
            let bytes_ptr = Box::new(bytes);
            let page = Page::new(p_id, bytes_ptr.clone());
            pages.push(page);
            let new_n_id = NodeId::from_page_id(p_id);
            cache.push_node(
                new_n_id,
                CacheNode::Leaf(CacheLeafNode::new(p_id, bytes_ptr)),
            )
        }
    }
}

fn process_both_bloom(
    vcache: &mut VCache,
    stream: &mut TcpStream,
    p_ids: Vec<PageId>,
    pages: &mut Vec<Page>,
    map: &mut HashMap<PageId, Digest>,
    vbf: &VersionBloomFilter,
) {
    for p_id in p_ids {
        let key = pid_to_key(p_id);
        let n_opt = vcache.get_node(&key).cloned();
        if let Some(n) = n_opt {
            if n.is_valid() {
                trace!("the node is valid in cache");
                match n {
                    VCacheNode::Leaf(l) => {
                        let bytes_ptr = l.get_bytes();
                        let page = Page::new(p_id, bytes_ptr.clone());
                        pages.push(page);
                    }
                    VCacheNode::NonLeaf(_) => {
                        panic!("Impossible be a non-leaf node when check page")
                    }
                }
            } else {
                let (leaf_v, leaf_set) = match n.clone() {
                    VCacheNode::Leaf(l) => (l.get_version(), l.get_set().clone()),
                    VCacheNode::NonLeaf(_) => panic!("impossible to be a non-leaf"),
                };
                let mut path = vec![(leaf_v, leaf_set)];
                let mut cur_id = n.get_id();

                while vcache.has_sib(cur_id) {
                    let parent_opt = vcache.find_parent(cur_id);
                    if let Some(parent) = parent_opt {
                        let parent_v = parent.get_version();
                        let parent_set = parent.get_set().clone();
                        path.push((parent_v, parent_set));
                        cur_id = parent.get_id();
                    } else {
                        break;
                    }
                }

                let mut valid_flag = false;
                let mut cur_id = NodeId::from_page_id(p_id);
                let mut target_n_id = NodeId::new(0, 0); // place-holder
                for (v, set) in &path {
                    // bottom-up
                    if !vbf.contains_subroot(set, *v) {
                        target_n_id = cur_id;
                        valid_flag = true;
                    } else {
                        break;
                    }
                    cur_id = cur_id.get_parent_id();
                }

                if valid_flag {
                    match n {
                        VCacheNode::Leaf(l) => {
                            let bytes_ptr = l.get_bytes();
                            let page = Page::new(p_id, bytes_ptr.clone());
                            pages.push(page);
                        }
                        VCacheNode::NonLeaf(_) => {
                            panic!("Impossible be a non-leaf node when check page")
                        }
                    }
                    vcache.confirm(target_n_id);
                } else {
                    // page validate algo
                    match &n {
                        VCacheNode::Leaf(l) => {
                            // find path in cache
                            let leaf_dig = l.to_digest();
                            let leaf_bytes_dig = l.get_bytes().to_digest();
                            let mut path = vec![leaf_dig];
                            let bytes_ptr = l.get_bytes();
                            let mut cur_id = n.get_id();
                            while vcache.has_sib(cur_id) {
                                let parent_opt = vcache.find_parent(cur_id);
                                if let Some(parent) = parent_opt {
                                    path.push(parent.to_digest());
                                    cur_id = parent.get_id();
                                } else {
                                    break;
                                }
                            }

                            // send path to server
                            trace!("send confirm request to server: {}", p_id);
                            let transfer_data = (CONFIRM, p_id, path);
                            let bytes =
                                bincode::serialize(&transfer_data).expect("failed to serialize");
                            let _w_amt = stream.write(&bytes).expect("failed to write");

                            // confirm or require page after receiving info from server
                            let mut buffer = [0; PAGE_SIZE as usize];
                            let _bytes_read =
                                stream.read(&mut buffer).expect("failed to read stream");
                            let resp = bincode::deserialize::<u32>(&buffer)
                                .expect("failed to deserialize bincode");

                            if resp == YES_FLAG {
                                // will receive (NodeId)
                                let _w_amt = stream
                                    .write(&YES_FLAG.to_le_bytes())
                                    .expect("failed to write");
                                let mut buff = [0; PAGE_SIZE as usize];
                                let _bytes_read =
                                    stream.read(&mut buff).expect("failed to read stream");
                                let (h, w) = bincode::deserialize::<(u32, u32)>(&buff)
                                    .expect("failed to deserialize bincde");
                                let cache_n_id = NodeId::new(h, w);
                                map.insert(p_id, leaf_bytes_dig);

                                unsafe {
                                    let cur_v = GLOBAL_TS;
                                    vcache.confirm_with_version(cache_n_id, cur_v);
                                }

                                let page = Page::new(p_id, bytes_ptr.clone());
                                pages.push(page);
                            } else {
                                let _w_amt = stream
                                    .write(&YES_FLAG.to_le_bytes())
                                    .expect("failed to write");
                                // will receive page
                                let mut buff = [0; PAGE_SIZE as usize];
                                let _bytes_read =
                                    stream.read(&mut buff).expect("failed to read stream");
                                let bytes: [u8; PAGE_SIZE as usize] = buff;
                                map.insert(p_id, bytes.to_digest());
                                let bytes_ptr = Box::new(bytes);
                                let page = Page::new(p_id, bytes_ptr.clone());
                                pages.push(page);

                                unsafe {
                                    let cur_v = GLOBAL_TS;
                                    let idxes = vbf.get_bf_pos(p_id);
                                    vcache.insert(p_id, bytes_ptr, cur_v, idxes);
                                }
                            }
                        }
                        VCacheNode::NonLeaf(_) => {
                            panic!("Impossible be a non-leaf node when confirm page")
                        }
                    }
                }
            }
        } else {
            debug!("Page not exist in cache, require from remote");
            let bytes = require_page(p_id, stream);
            map.insert(p_id, bytes.to_digest());
            let bytes_ptr = Box::new(bytes);
            let page = Page::new(p_id, bytes_ptr.clone());
            pages.push(page);
            unsafe {
                let version = GLOBAL_TS;
                let idxes = vbf.get_bf_pos(p_id);
                vcache.insert(p_id, bytes_ptr, version, idxes);
            }
        }
    }
}

fn process_simply_bloom(
    svcache: &mut SVCache,
    stream: &mut TcpStream,
    p_ids: Vec<PageId>,
    pages: &mut Vec<Page>,
    map: &mut HashMap<PageId, Digest>,
    vbf: &VersionBloomFilter,
) {
    for p_id in p_ids {
        let key = pid_to_key(p_id);
        let n_opt = svcache.get_node(&key).cloned();
        if let Some(n) = n_opt {
            if n.is_valid() {
                trace!("the node is valid in cache");
                match n {
                    SVCacheNode::Leaf(l) => {
                        let bytes_ptr = l.get_bytes();
                        let page = Page::new(p_id, bytes_ptr.clone());
                        pages.push(page);
                    }
                    SVCacheNode::NonLeaf(_) => {
                        panic!("Impossible be a non-leaf node when check page")
                    }
                }
            } else {
                let leaf_v = match n.clone() {
                    SVCacheNode::Leaf(l) => l.get_version(),
                    SVCacheNode::NonLeaf(_) => panic!("impossible to be a non-leaf"),
                };
                let cur_id = n.get_id();

                if !vbf.contains(p_id, leaf_v) {
                    // directly use
                    match n {
                        SVCacheNode::Leaf(l) => {
                            let bytes_ptr = l.get_bytes();
                            let page = Page::new(p_id, bytes_ptr.clone());
                            pages.push(page);
                        }
                        SVCacheNode::NonLeaf(_) => {
                            panic!("Impossible be a non-leaf node when check page")
                        }
                    }
                    svcache.confirm(cur_id);
                } else {
                    // page validate
                    match &n {
                        SVCacheNode::Leaf(l) => {
                            // find path in cache
                            let leaf_dig = l.to_digest();
                            let leaf_bytes_dig = l.get_bytes().to_digest();
                            let mut path = vec![leaf_dig];
                            let bytes_ptr = l.get_bytes();
                            let mut cur_id = n.get_id();
                            while svcache.has_sib(cur_id) {
                                let parent_opt = svcache.find_parent(cur_id);
                                if let Some(parent) = parent_opt {
                                    path.push(parent.to_digest());
                                    cur_id = parent.get_id();
                                } else {
                                    break;
                                }
                            }

                            // send path to server
                            trace!("send confirm request to server: {}", p_id);
                            let transfer_data = (CONFIRM, p_id, path);
                            let bytes =
                                bincode::serialize(&transfer_data).expect("failed to serialize");
                            let _w_amt = stream.write(&bytes).expect("failed to write");

                            // confirm or require page after receiving info from server
                            let mut buffer = [0; PAGE_SIZE as usize];
                            let _bytes_read =
                                stream.read(&mut buffer).expect("failed to read stream");
                            let resp = bincode::deserialize::<u32>(&buffer)
                                .expect("failed to deserialize bincode");

                            if resp == YES_FLAG {
                                // will receive (NodeId)
                                let _w_amt = stream
                                    .write(&YES_FLAG.to_le_bytes())
                                    .expect("failed to write");
                                let mut buff = [0; PAGE_SIZE as usize];
                                let _bytes_read =
                                    stream.read(&mut buff).expect("failed to read stream");
                                let (h, w) = bincode::deserialize::<(u32, u32)>(&buff)
                                    .expect("failed to deserialize bincde");
                                let cache_n_id = NodeId::new(h, w);
                                map.insert(p_id, leaf_bytes_dig);

                                unsafe {
                                    let cur_v = GLOBAL_TS;
                                    svcache.confirm_with_version(cache_n_id, cur_v);
                                }

                                let page = Page::new(p_id, bytes_ptr.clone());
                                pages.push(page);
                            } else {
                                let _w_amt = stream
                                    .write(&YES_FLAG.to_le_bytes())
                                    .expect("failed to write");
                                // will receive page
                                let mut buff = [0; PAGE_SIZE as usize];
                                let _bytes_read =
                                    stream.read(&mut buff).expect("failed to read stream");
                                let bytes: [u8; PAGE_SIZE as usize] = buff;
                                map.insert(p_id, bytes.to_digest());
                                let bytes_ptr = Box::new(bytes);
                                let page = Page::new(p_id, bytes_ptr.clone());
                                pages.push(page);

                                unsafe {
                                    let cur_v = GLOBAL_TS;
                                    svcache.insert(p_id, bytes_ptr, cur_v);
                                }
                            }
                        }
                        SVCacheNode::NonLeaf(_) => {
                            panic!("Impossible be a non-leaf node when confirm page")
                        }
                    }
                }
            }
        } else {
            debug!("Page not exist in cache, require from remote");
            let bytes = require_page(p_id, stream);
            map.insert(p_id, bytes.to_digest());
            let bytes_ptr = Box::new(bytes);
            let page = Page::new(p_id, bytes_ptr.clone());
            pages.push(page);
            unsafe {
                let version = GLOBAL_TS;
                svcache.insert(p_id, bytes_ptr, version);
            }
        }
    }
}

fn process_both_cache(
    cache: &mut Cache,
    stream: &mut TcpStream,
    p_ids: Vec<PageId>,
    pages: &mut Vec<Page>,
    map: &mut HashMap<PageId, Digest>,
) {
    trace!("process both cache");
    for p_id in p_ids {
        let key = pid_to_key(p_id);
        if let Some(n) = cache.get_node(&key) {
            if n.is_valid() {
                trace!("the node is valid in cache");
                match n {
                    CacheNode::Leaf(l) => {
                        let bytes_ptr = l.get_bytes();
                        let page = Page::new(p_id, bytes_ptr.clone());
                        pages.push(page);
                    }
                    CacheNode::NonLeaf(_) => {
                        panic!("Impossible be a non-leaf node when check page")
                    }
                }
            } else {
                trace!("the node is unknown in cache");
                match n {
                    CacheNode::Leaf(l) => {
                        // find path in cache
                        let leaf_dig = l.to_digest();
                        let leaf_bytes_dig = l.get_bytes().to_digest();
                        let mut path = vec![leaf_dig];
                        let bytes_ptr = l.get_bytes();

                        let mut cur_n = n.clone();
                        while cache.has_sib(cur_n.get_id()) {
                            let parent_opt = cache.find_parent(cur_n.get_id());
                            if let Some(parent) = parent_opt {
                                path.push(parent.to_digest());
                                cur_n = parent.clone();
                            } else {
                                break;
                            }
                        }

                        // send path to server
                        trace!("send confirm request to server: {}", p_id);
                        let transfer_data = (CONFIRM, p_id, path);
                        let bytes =
                            bincode::serialize(&transfer_data).expect("failed to serialize");
                        let _w_amt = stream.write(&bytes).expect("failed to write");

                        // confirm or require page after receiving info from server
                        let mut buffer = [0; PAGE_SIZE as usize];
                        let _bytes_read = stream.read(&mut buffer).expect("failed to read stream");
                        let resp = bincode::deserialize::<u32>(&buffer)
                            .expect("failed to deserialize bincode");

                        if resp == YES_FLAG {
                            // will receive (NodeId)
                            let _w_amt = stream
                                .write(&YES_FLAG.to_le_bytes())
                                .expect("failed to write");
                            let mut buff = [0; PAGE_SIZE as usize];
                            let _bytes_read =
                                stream.read(&mut buff).expect("failed to read stream");
                            let (h, w) = bincode::deserialize::<(u32, u32)>(&buff)
                                .expect("failed to deserialize bincde");
                            let cache_n_id = NodeId::new(h, w);
                            map.insert(p_id, leaf_bytes_dig);

                            cache.confirm(cache_n_id);

                            let page = Page::new(p_id, bytes_ptr.clone());
                            pages.push(page);
                        } else {
                            let _w_amt = stream
                                .write(&YES_FLAG.to_le_bytes())
                                .expect("failed to write");
                            // will receive page
                            let mut buff = [0; PAGE_SIZE as usize];
                            let _bytes_read =
                                stream.read(&mut buff).expect("failed to read stream");
                            let bytes: [u8; PAGE_SIZE as usize] = buff;
                            map.insert(p_id, bytes.to_digest());
                            let bytes_ptr = Box::new(bytes);
                            let page = Page::new(p_id, bytes_ptr.clone());
                            pages.push(page);
                            cache.insert(p_id, bytes_ptr);
                        }
                    }
                    CacheNode::NonLeaf(_) => {
                        panic!("Impossible be a non-leaf node when confirm page")
                    }
                }
            }
        } else {
            debug!("Page not exist in cache, require from remote");
            let bytes = require_page(p_id, stream);
            map.insert(p_id, bytes.to_digest());
            let bytes_ptr = Box::new(bytes);
            let page = Page::new(p_id, bytes_ptr.clone());
            pages.push(page);
            cache.insert(p_id, bytes_ptr);
        }
    }
}

fn require_page(pid: PageId, stream: &mut TcpStream) -> [u8; PAGE_SIZE as usize] {
    debug!("required page id: {:?}", pid);
    let transfer_data: (u32, PageId, Vec<Digest>) = (QUERY, pid, vec![]);
    let bytes = bincode::serialize(&transfer_data).expect("failed to serialize");
    let _w_amt = stream.write(&bytes).expect("failed to write");
    let mut reader = BufReader::new(stream);
    let mut buffer = [0; PAGE_SIZE as usize];
    let read_len = reader.read(&mut buffer).expect("failed to read buffer");
    debug!("read length: {}", read_len);
    let p_cont: [u8; PAGE_SIZE as usize] = buffer;
    debug!("user has received page bytes");
    p_cont
}

fn compute_page_ids(ofst: u64, len: u64) -> (u64, Vec<PageId>) {
    let start_page = ofst / PAGE_SIZE as u64;
    let start_point = start_page * PAGE_SIZE as u64;
    // let end_page = (ofst + len) / PAGE_SIZE as u64;
    let end_page = (ofst + len - 1) / PAGE_SIZE as u64; // ATTENTION PLEASE
    let mut res = Vec::new();
    for i in start_page..(end_page + 1) {
        res.push(PageId(i as u32));
    }
    (start_point, res)
}

// build the merkle tree from scratch
pub fn build_merkle_tree() -> Result<()> {
    info!("building merkle tree...");
    let mut file = File::open(Path::new(MAIN_PATH)).expect("failed to open file");
    let file_len = file.metadata().expect("Failed to get metadata").len();
    let mut merkle_db =
        MerkleDB::create_new(Path::new(MERKLE_PATH)).expect("failed to open or create merkle db");
    let root_id = merkle_db.get_root_id();
    let mut ctx = WriteContext::new(&merkle_db, root_id);
    let mut ofset: u64 = 0;
    let mut p_id_num = 0;

    loop {
        trace!("updating for page: {}", p_id_num);
        let mut buf: [u8; PAGE_SIZE as usize] = [0; PAGE_SIZE as usize];
        match file.seek(SeekFrom::Start(ofset)) {
            Ok(o) => {
                if o != ofset {
                    bail!("seek position not correct");
                }
            }
            Err(_) => {
                bail!("sqlite io error write happened");
            }
        }

        if let Err(err) = file.read_exact(&mut buf) {
            let kind = err.kind();
            if kind == ErrorKind::UnexpectedEof {
                warn!("file length not enough");
            } else {
                warn!("sqlite io err");
                bail!("sqlite io error");
            }
        }
        ctx.update(buf.to_digest(), PageId(p_id_num))
            .expect("Failed to update merkle tree");

        ofset += PAGE_SIZE as u64;
        p_id_num += 1;
        if ofset >= file_len {
            break;
        }
    }

    let changes = ctx.changes();
    let new_root_id = changes.root_id;
    for (addr, node) in changes.nodes {
        merkle_db.write_node(&addr, &node)?;
    }

    merkle_db.update_param(new_root_id)?;
    merkle_db.close();
    info!("build merkle tree finished.");
    Ok(())
}

fn check_dig(dig: &Digest) -> bool {
    let buf = [0_u8; PAGE_SIZE as usize];
    *dig == buf.to_digest()
}

unsafe fn update_merkle_tree(
    ofset: u64,
    page_ids: Vec<PageId>,
    p_file: *mut ffi::sqlite3_file,
) -> c_int {
    trace!("updating merkle tree");
    let mut file = File::open(Path::new(MAIN_PATH)).expect("failed to open file");
    let map = s_get_map(p_file).expect("Cannot get the map");
    let mut ofset = ofset;
    for p_id in page_ids {
        let mut buf: [u8; PAGE_SIZE as usize] = [0; PAGE_SIZE as usize];
        match file.seek(SeekFrom::Start(ofset)) {
            Ok(o) => {
                if o != ofset {
                    return ffi::SQLITE_IOERR_WRITE;
                }
            }
            Err(_) => {
                return ffi::SQLITE_IOERR_WRITE;
            }
        }

        if let Err(err) = file.read_exact(&mut buf) {
            let kind = err.kind();
            if kind == ErrorKind::UnexpectedEof {
                trace!("file length not enough");
            } else {
                return ffi::SQLITE_IOERR_READ;
            }
        }
        let dig = buf.to_digest();
        if !check_dig(&dig) {
            map.insert(p_id, buf.to_digest());
        }
        ofset += PAGE_SIZE as u64;
    }
    drop(file);
    ffi::SQLITE_OK
}

/// # Safety
///
/// Server writes data to a file.
pub unsafe extern "C" fn s_write(
    p_file: *mut ffi::sqlite3_file,
    z: *const c_void, // buffer
    i_amt: c_int,
    i_ofst: ffi::sqlite3_int64,
) -> c_int {
    trace!("server write offset={} len={}", i_ofst, i_amt);

    let file = s_get_file(p_file).expect("failed to get file in ServerFileState");

    // move the cursor to the offset
    match file.seek(SeekFrom::Start(i_ofst as u64)) {
        Ok(o) => {
            if o != i_ofst as u64 {
                return ffi::SQLITE_IOERR_READ;
            }
        }
        Err(_) => {
            return ffi::SQLITE_IOERR_READ;
        }
    }
    let data = slice::from_raw_parts(z as *const u8, i_amt as usize);
    if let Err(_err) = file.write_all(data) {
        return ffi::SQLITE_IOERR_WRITE;
    }

    let (ofset, page_ids) = compute_page_ids(i_ofst as u64, i_amt as u64);
    let version = GLOBAL_TS;
    let vbf = s_get_vbf(p_file).expect("failed to get file in ServerFileState");
    for p_id in &page_ids {
        vbf.insert(*p_id, version);
    }

    update_merkle_tree(ofset, page_ids, p_file)
}

/// # Safety
///
/// User writes data to a file, should never reach here except for tmp files.
pub unsafe extern "C" fn u_write(
    p_file: *mut ffi::sqlite3_file,
    z: *const c_void, // buffer
    i_amt: c_int,
    i_ofst: ffi::sqlite3_int64,
) -> c_int {
    trace!("user write offset={} len={}", i_ofst, i_amt);
    let file_data = u_get_file(p_file).expect("failed to get file in ServerFileState");

    let file = &mut file_data.file;
    trace!("u_write file: {:?}", file);
    // move the cursor to the offset
    match file.seek(SeekFrom::Start(i_ofst as u64)) {
        Ok(o) => {
            if o != i_ofst as u64 {
                return ffi::SQLITE_IOERR_READ;
            }
        }
        Err(_) => {
            return ffi::SQLITE_IOERR_READ;
        }
    }
    let data = slice::from_raw_parts(z as *mut u8, i_amt as usize);
    if let Err(_err) = file.write_all(data) {
        return ffi::SQLITE_IOERR_WRITE;
    }
    trace!("write succeeds");
    ffi::SQLITE_OK
}

/// # Safety
///
/// Server truncates the file.
pub unsafe extern "C" fn s_truncate(
    p_file: *mut ffi::sqlite3_file,
    size: ffi::sqlite3_int64,
) -> c_int {
    trace!("truncate");
    let file = s_get_file(p_file).expect("failed to get file in ServerFileState");

    if file.set_len(size as u64).is_err() {
        return ffi::SQLITE_IOERR_TRUNCATE;
    }

    ffi::SQLITE_OK
}

/// # Safety
///
/// User truncates the file, should never reached here.
pub unsafe extern "C" fn u_truncate(
    _p_file: *mut ffi::sqlite3_file,
    _size: ffi::sqlite3_int64,
) -> c_int {
    panic!("Should never reach user i/o truncate");
}

/// # Safety
///
/// Server persists changes to the file.
pub unsafe extern "C" fn s_sync(p_file: *mut ffi::sqlite3_file, _flags: c_int) -> c_int {
    trace!("s_sync");
    let file = s_get_file(p_file).expect("failed to get file in ServerFileState");
    if file.flush().is_err() {
        return ffi::SQLITE_IOERR_FSYNC;
    }

    ffi::SQLITE_OK
}

/// # Safety
///
/// User persists changes to the file.
pub unsafe extern "C" fn u_sync(_p_file: *mut ffi::sqlite3_file, _flags: c_int) -> c_int {
    trace!("u_sync");
    ffi::SQLITE_OK
}

/// # Safety
///
/// Server returns the current file-size of the file.
pub unsafe extern "C" fn s_file_size(
    p_file: *mut ffi::sqlite3_file,
    p_size: *mut ffi::sqlite3_int64,
) -> c_int {
    trace!("file_size");
    let file = s_get_file(p_file).expect("failed to get file in ServerFileState");
    let len = file.metadata().expect("failed to query metadata").len();
    let p_size: &mut ffi::sqlite3_int64 = p_size.as_mut().expect("null pointer");
    *p_size = len as ffi::sqlite3_int64;

    ffi::SQLITE_OK
}

/// # Safety
///
/// User returns the current file-size of the file.
pub unsafe extern "C" fn u_file_size(
    _p_file: *mut ffi::sqlite3_file,
    p_size: *mut ffi::sqlite3_int64,
) -> c_int {
    trace!("require user file size");
    // todo: should be implemented by remote request
    // file size should be written to chain, or a merkle proof should be returned
    let file = File::open(Path::new(MAIN_PATH)).expect("Failed to open file");
    let len = file.metadata().expect("failed to query metadata").len();
    let p_size: &mut ffi::sqlite3_int64 = p_size.as_mut().expect("null pointer");
    *p_size = len as ffi::sqlite3_int64;

    // file.sync_all().expect("err happened while closing the db file"); // time consuming since db file is large
    ffi::SQLITE_OK
}

/// # Safety
///
/// Close a file.
pub unsafe extern "C" fn s_close(p_file: *mut ffi::sqlite3_file) -> c_int {
    trace!("close");
    if let Some(file_state) = (p_file as *mut ServerFileState).as_mut() {
        let file = file_state.file.assume_init_mut();
        trace!("close file {:?}", file);

        let old_file_opt = mem::replace(&mut file_state.file, MaybeUninit::uninit());
        // file_state.file = None;
        let old_file_data = old_file_opt.assume_init();
        let old_file = old_file_data;
        drop(old_file);
    }
    let path = Path::new(TMP_FILE_PATH);
    if path.exists() {
        std::fs::remove_file(path).expect("cannot remove tmp file");
    }
    ffi::SQLITE_OK
}

/// # Safety
///
/// Close a file.
pub unsafe extern "C" fn u_close(p_file: *mut ffi::sqlite3_file) -> c_int {
    trace!("u_close");
    if let Some(file_state) = (p_file as *mut UserFileState).as_mut() {
        let file_data = file_state.tmp_file.assume_init_mut();
        debug!("close: {:?}", file_data);
        let file_path = &file_data.name;
        let path = Path::new(file_path);
        if path.exists() {
            std::fs::remove_file(path).expect("cannot remove tmp file");
        }
        let old_file_opt = mem::replace(&mut file_state.tmp_file, MaybeUninit::uninit());
        let old_file_data = old_file_opt.assume_init();
        let old_file = old_file_data.file;
        drop(old_file);
    }

    trace!("u_close succeeds");
    ffi::SQLITE_OK
}

/// # Safety
///
/// Lock a file.
pub unsafe extern "C" fn lock(_p_file: *mut ffi::sqlite3_file, _e_lock: c_int) -> c_int {
    trace!("lock");
    // TODO: implement locking
    ffi::SQLITE_OK
}

/// # Safety
///
/// Unlock a file.
pub unsafe extern "C" fn unlock(_p_file: *mut ffi::sqlite3_file, _e_lock: c_int) -> c_int {
    trace!("unlock");
    // TODO: implement unlocking
    ffi::SQLITE_OK
}

/// # Safety
///
/// Check if another file-handle holds a RESERVED lock on a file.
pub unsafe extern "C" fn check_reserved_lock(
    _p_file: *mut ffi::sqlite3_file,
    p_res_out: *mut c_int,
) -> c_int {
    trace!("check_reserved_lock");
    match p_res_out.as_mut() {
        Some(p_res_out) => {
            *p_res_out = false as i32;
        }
        None => {
            return ffi::SQLITE_IOERR_CHECKRESERVEDLOCK;
        }
    }

    // TODO: implement locking
    ffi::SQLITE_OK
}

/// # Safety
///
/// File control method. For custom operations on an mem-file.
pub unsafe extern "C" fn file_control(
    _p_file: *mut ffi::sqlite3_file,
    op: c_int,
    _p_arg: *mut c_void,
) -> c_int {
    trace!("file_control op={}", op);
    ffi::SQLITE_NOTFOUND
}

/// # Safety
///
/// Return the sector-size in bytes for a file.
pub unsafe extern "C" fn sector_size(_p_file: *mut ffi::sqlite3_file) -> c_int {
    trace!("sector_size");

    1024
}

/// # Safety
///
/// Return the device characteristic flags supported by a file.
pub unsafe extern "C" fn device_characteristics(_p_file: *mut ffi::sqlite3_file) -> c_int {
    trace!("device_characteristics");
    // For now, simply copied from [memfs] without putting in a lot of thought.
    // [memfs]: (https://github.com/sqlite/sqlite/blob/a959bf53110bfada67a3a52187acd57aa2f34e19/ext/misc/memvfs.c#L271-L276)

    // writes of any size are atomic
    ffi::SQLITE_IOCAP_ATOMIC |
        // after reboot following a crash or power loss, the only bytes in a file that were written
        // at the application level might have changed and that adjacent bytes, even bytes within
        // the same sector are guaranteed to be unchanged
        ffi::SQLITE_IOCAP_POWERSAFE_OVERWRITE |
        // when data is appended to a file, the data is appended first then the size of the file is
        // extended, never the other way around
        ffi::SQLITE_IOCAP_SAFE_APPEND |
        // information is written to disk in the same order as calls to xWrite()
        ffi::SQLITE_IOCAP_SEQUENTIAL
}

/// # Safety
///
/// Create a shared memory file mapping.
pub unsafe extern "C" fn shm_map(
    _p_file: *mut ffi::sqlite3_file,
    i_pg: i32,
    pgsz: i32,
    b_extend: i32,
    _pp: *mut *mut c_void,
) -> i32 {
    trace!("shm_map pg={} sz={} extend={}", i_pg, pgsz, b_extend);

    ffi::SQLITE_IOERR_SHMMAP
}

/// # Safety
///
/// Perform locking on a shared-memory segment.
pub unsafe extern "C" fn shm_lock(
    _p_file: *mut ffi::sqlite3_file,
    _offset: i32,
    _n: i32,
    _flags: i32,
) -> i32 {
    trace!("shm_lock");

    ffi::SQLITE_IOERR_SHMLOCK
}

/// # Safety
///
/// Memory barrier operation on shared memory.
pub unsafe extern "C" fn shm_barrier(_p_file: *mut ffi::sqlite3_file) {
    trace!("shm_barrier");
}

/// # Safety
///
/// Unmap a shared memory segment.
pub unsafe extern "C" fn shm_unmap(_p_file: *mut ffi::sqlite3_file, _delete_flags: i32) -> i32 {
    trace!("shm_unmap");

    ffi::SQLITE_OK
}

/// # Safety
///
/// Fetch a page of a memory-mapped file.
pub unsafe extern "C" fn mem_fetch(
    _p_file: *mut ffi::sqlite3_file,
    i_ofst: i64,
    i_amt: i32,
    _pp: *mut *mut c_void,
) -> i32 {
    trace!("mem_fetch offset={} len={}", i_ofst, i_amt);

    ffi::SQLITE_ERROR
}

/// # Safety
///
/// Release a memory-mapped page.
pub unsafe extern "C" fn mem_unfetch(
    _p_file: *mut ffi::sqlite3_file,
    i_ofst: i64,
    _p_page: *mut c_void,
) -> i32 {
    trace!("mem_unfetch offset={}", i_ofst);

    ffi::SQLITE_OK
}
