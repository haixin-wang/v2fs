use anyhow::Result;
use sgx_types::sgx_status_t;
use vfs_common::page::PageId;
use vfs_common::digest::{Digest, DIGEST_LEN, Digestible};
use vfs_common::SGX_VFS;
use merkle_tree::{storage::{MerkleNode, NodeId}, proof::Proof, hash::{leaf_hash, nonleaf_hash}};
use hashbrown::{HashMap, HashSet};
use alloc::{vec::Vec, collections::vec_deque::VecDeque};
use crate::vfs::server_vfs::{server_vfs_state, CachePage};


extern "C" {
    fn ocall_get_read_proof_len(
        retval: *mut i32, 
        ptr: *const u8, 
        len: usize, 
        proof_len: *mut usize,
    ) -> sgx_status_t; 

    fn ocall_get_read_proof(
        retval: *mut i32, 
        ptr: *const u8, 
        len: usize, 
        proof_ptr: *mut u8, 
        proof_len: usize
    ) -> sgx_status_t; 

    fn ocall_get_read_proof_with_len(
        retval: *mut i32, 
        ptr: *const u8, 
        len: usize, 
        proof_ptr: *mut u8, 
        predicated_p_len: usize,
        real_p_len: *mut usize,
    ) -> sgx_status_t; 

    fn ocall_get_merkle_root(
        retval: *mut i32,
        ptr: *mut u8, 
        len: usize
    ) -> sgx_status_t;

    fn ocall_get_node(
        retval: *mut i32,
        id_ptr: *const u8, 
        id_len: usize,
        ptr: *mut u8,
        len: usize
    ) -> sgx_status_t;

    fn ocall_update_merkle_db(
        retval: *mut i32,
        ptr: *const u8, 
        len: usize,
    ) -> sgx_status_t;
}

pub(crate) fn verify_then_update() -> Result<()> {
    let (read_map, write_map) = 
    unsafe {
        let name = std::ffi::CString::new(SGX_VFS).unwrap();
        let p_vfs = libsqlite3_sys::sqlite3_vfs_find(name.as_ptr());
        let state = server_vfs_state(p_vfs).expect("null pointer");
        let s_vfs = &mut state.vfs;
        (&mut s_vfs.read_map, &mut s_vfs.write_map)
    };

    let (root_id, root_hash) = get_origin_root()?;
    let len = read_map.len();

    if len > 0 {
        if let Some(r_id) = root_id {
            verify_read_map(read_map, root_hash, r_id).unwrap();
        }
    }
    
    verify_write_map_base(write_map, root_id).unwrap();
    println!("Verification succeeds.");
    Ok(())
}

fn get_origin_root() -> Result<(Option<NodeId>, Digest)> {
    let tuple = (Some(NodeId::new(0, 0)), Digest::default());
    let bytes = match postcard::to_allocvec(&tuple) {
        Ok(buf) => buf,
        Err(e) => {
            bail!("failed to cast root info to bytes, reason: {:?}", e);
        }
    };
    let len = bytes.len();
    let mut info_buf = vec![0 as u8; len];
    let mut retval = 0;
    let sgx_ret = unsafe {
        ocall_get_merkle_root(&mut retval as *mut _, info_buf.as_mut_ptr() as *mut _, len)
    };
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_err happened");
    }
    Ok(postcard::from_bytes::<(Option<NodeId>, Digest)>(&info_buf[..]).unwrap())
}

fn verify_write_map_base(
    write_map: &mut HashMap<PageId, CachePage>, 
    old_root_id: Option<NodeId>
) -> Result<()> {
    let mut modif_hashes = Vec::new();
    for (p_id, cache_p) in write_map.drain() {
        modif_hashes.push((p_id, cache_p.to_digest()));
    }
    modif_hashes.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));
    let mut proof = HashMap::<NodeId, Digest>::new();
    let mut height = 0;
    if let Some(r_id) = old_root_id {
        verify_read(&mut proof, &modif_hashes, r_id)?;
        height = r_id.get_height();
    }

    cal_new_root(&modif_hashes, &mut proof, height)?;

    // whether verify write map is an optimization
    // update_merkle_db(&modif_hashes)?;

    Ok(())
}

fn update_merkle_db(modif_hashes: &Vec<(PageId, Digest)>) -> Result<()> {
    let bytes = match postcard::to_allocvec(&modif_hashes) {
        Ok(buf) => buf,
        Err(_) => {
            bail!("postcard serialize for Vec<(PageId, Digest)> failed");
        }
    };
    let mut retval: i32 = 0;
    let sgx_ret = unsafe {
        ocall_update_merkle_db(&mut retval as *mut _, bytes.as_ptr(), bytes.len())
    };
    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        bail!("sgx_err happened");
    }
    Ok(())
}

fn verify_read(
    proof: &mut HashMap<NodeId, Digest>,
    modif: &Vec<(PageId, Digest)>,
    r_id: NodeId,
) -> Result<()> {
    let mut visited = HashSet::<NodeId>::new();
    let mut queue = VecDeque::<NodeId>::new();
    for (p_id, _) in modif {
        queue.push_back(NodeId::from_page_id(*p_id));
    }

    let mut cur_id;
    while let Some(n_id) = queue.pop_front() {
        cur_id = n_id;
        visited.insert(cur_id);
        let sib_id = cur_id.get_sib_id();

        let cur_bytes = match postcard::to_allocvec(&cur_id) {
            Ok(buf) => buf,
            Err(e) => {
                bail!("failed to cast node id to bytes, reason: {:?}", e);
            }
        };
        let mut retval: i32 = 0;
        let node_len = node_len()?;
        let mut node_buf = vec![0 as u8; node_len];
        let sgx_ret = unsafe {
            ocall_get_node(&mut retval as *mut _, cur_bytes.as_ptr(), cur_bytes.len(), node_buf.as_mut_ptr() as *mut _, node_len)
        };
        if sgx_ret != sgx_status_t::SGX_SUCCESS {
            bail!("sgx_err happened");
        }
        let cur_n = postcard::from_bytes::<Option<MerkleNode>>(&node_buf[..]).unwrap();
        if let Some(n) = cur_n {
            proof.insert(cur_id, n.get_hash());
        }

        let sib_bytes = match postcard::to_allocvec(&sib_id) {
            Ok(buf) => buf,
            Err(e) => {
                bail!("failed to cast node id to bytes, reason: {:?}", e);
            }
        };
        let mut sib_buf = vec![0 as u8; node_len];
        let sgx_ret = unsafe {
            ocall_get_node(&mut retval as *mut _, sib_bytes.as_ptr(), sib_bytes.len(), sib_buf.as_mut_ptr() as *mut _, node_len)
        };
        if sgx_ret != sgx_status_t::SGX_SUCCESS {
            bail!("sgx_err happened");
        }
        let sib_n = postcard::from_bytes::<Option<MerkleNode>>(&sib_buf[..]).unwrap();
        if let Some(n) = sib_n {
            proof.insert(sib_id, n.get_hash());
        }

        let parent_id = cur_id.get_parent_id();
        if !queue.contains(&parent_id) {
            queue.push_back(parent_id);
        }

        if !cur_id.is_leaf() {
            if let Some(cur_hash) = proof.get(&cur_id) {
                let (l_id, r_id) = cur_id.get_children()?;
                let l_hash = proof.get(&l_id);
                let r_hash = proof.get(&r_id);
                if *cur_hash != nonleaf_hash(l_hash.copied(), r_hash.copied()) {
                    bail!("verification failed at node {:?}", cur_id);
                }
            }
        }
        if cur_id.get_height() == r_id.get_height() {
            break;
        }
    }
    for n_id in visited.drain() {
        proof.remove(&n_id);
    }

    Ok(())
}


// calculate the new root hash and id
fn cal_new_root(
    modif: &Vec<(PageId, Digest)>,
    proof: &mut HashMap<NodeId, Digest>,
    height: u32
) -> Result<()> {
    let mut queue = VecDeque::<NodeId>::new();
    let mut max_pid = PageId(0);
    for (p_id, dig) in modif {
        if *p_id > max_pid {
            max_pid = *p_id;
        }
        queue.push_back(NodeId::from_page_id(*p_id));
        proof.insert(NodeId::from_page_id(*p_id), leaf_hash(p_id, dig));
    }
    let p_id_target_height = max_pid.find_height();

    let target_height = if p_id_target_height > height {
        p_id_target_height
    } else {
        height
    };

    let mut root_id = NodeId::from_page_id(PageId(0));
    let mut root_hash = Digest::default();

    while let Some(n_id) = queue.pop_front() {
        let cur_id = n_id;
        let cur_hash = proof.get(&cur_id).copied();
        let sib_id = cur_id.get_sib_id();
        let sib_hash = proof.get(&sib_id).copied();
        if cur_id.is_even() {
            root_hash = nonleaf_hash(cur_hash, sib_hash);
        } else {
            root_hash = nonleaf_hash(sib_hash, cur_hash);
        }
        root_id = cur_id.get_parent_id();
        queue.push_back(root_id);
        proof.insert(root_id, root_hash);
        if root_id.get_height() >= target_height {
            break;
        }
    }

    // sign root_hash and id then publish it
    // for dbg only
    println!("dbg: sgx computed new root id: {:?}", root_id);
    println!("dbg: sgx computed new root hash: {:?}", root_hash);
    
    Ok(())
}

fn node_len() -> Result<usize> {
    let n = Some(MerkleNode::new(Digest::default()));
    let bytes = match postcard::to_allocvec(&n) {
        Ok(buf) => buf,
        Err(e) => {
            bail!("failed to cast node to bytes, reason: {:?}", e);
        }
    };
    Ok(bytes.len())
}


fn verify_read_map(
    read_map: &mut HashMap<PageId, CachePage>, 
    old_root_hash: Digest, 
    old_r_id: NodeId
) -> Result<()> {
    let mut p_ids_to_verify = vec![];
    let mut p_hashes = vec![];
    for (p_id, cache_p) in read_map.drain() {
        p_ids_to_verify.push(p_id);
        p_hashes.push((p_id, cache_p.to_digest()));
    }

    let bytes = match postcard::to_allocvec(&p_ids_to_verify) {
        Ok(buf) => buf,
        Err(_) => {
            bail!("postcard serialize for Vec<PageId> failed");
        }
    };

    let predicated_p_len = predicate_proof_len(p_ids_to_verify.len(), old_r_id);
    println!("dbg: predicated proof len: {}", predicated_p_len);

    let mut retval: i32 = 0;
    let mut proof_buf = vec![0 as u8; predicated_p_len];
    let mut p_len = 0;
    let sgx_ret = unsafe {
        ocall_get_read_proof_with_len(&mut retval as *mut _, bytes.as_ptr(), bytes.len(), proof_buf.as_mut_ptr() as *mut _, predicated_p_len, &mut p_len as *mut usize)
    };

    if sgx_ret != sgx_status_t::SGX_SUCCESS {
        bail!("sgx_err happened");
    }
    let proof = postcard::from_bytes::<Proof>(&proof_buf[..p_len]).unwrap();
    let computed_root_hash = proof.root_hash()?;
    if computed_root_hash != old_root_hash {
        bail!("verification failed, the re-constructed root hash not matched");
    }

    for (p_id, dig) in p_hashes {
        let leaf_hash = leaf_hash(&p_id, &dig);
        proof.verify_val(leaf_hash, p_id, old_r_id.get_height())?;
    }

    Ok(())
}

fn predicate_proof_len(num_p: usize, root_id: NodeId) -> usize {
    let height = root_id.get_height();
    let num_dig = (height + 2) as usize * num_p;
    num_dig * DIGEST_LEN
}
