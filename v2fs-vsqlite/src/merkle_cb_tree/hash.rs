use crate::{
    digest::{blake2, Digest, Digestible},
    PageId,
};

use super::proof::sub_proof::SubProof;

/// H(height||width)
#[inline]
pub(crate) fn id_hash(height: u32, width: u32) -> Digest {
    let mut state = blake2().to_state();
    state.update(&height.to_le_bytes());
    state.update(&width.to_le_bytes());
    Digest::from(state.finalize())
}

/// h = H(p_id||p_hash)
#[inline]
pub fn leaf_hash(p_id: &PageId, p_hash: &Digest) -> Digest {
    let mut state = blake2().to_state();
    state.update(&p_id.0.to_le_bytes());
    state.update(p_hash.as_bytes());
    Digest::from(state.finalize())
}

/// h = H(H(h1||h2)) or h = H(H(h1)) or h = H(H(h2)
#[inline]
pub(crate) fn nonleaf_hash(l_hash_opt: Option<Digest>, r_hash_opt: Option<Digest>) -> Digest {
    let mut state = blake2().to_state();
    let inner_hash = if let Some(l_hash) = l_hash_opt {
        if let Some(r_hash) = r_hash_opt {
            let mut inner_state = blake2().to_state();
            inner_state.update(l_hash.as_bytes());
            inner_state.update(r_hash.as_bytes());
            Digest::from(inner_state.finalize())
        } else {
            let mut inner_state = blake2().to_state();
            inner_state.update(l_hash.as_bytes());
            Digest::from(inner_state.finalize())
        }
    } else if let Some(r_hash) = r_hash_opt {
        let mut inner_state = blake2().to_state();
        inner_state.update(r_hash.as_bytes());
        Digest::from(inner_state.finalize())
    } else {
        let inner_state = blake2().to_state();
        Digest::from(inner_state.finalize())
    };

    state.update(inner_hash.as_bytes());
    Digest::from(state.finalize())
}

/// h = H(H(h1||h2)) or h = H(H(h1)) or h = H(H(h2)
#[inline]
pub(crate) fn proof_nonleaf_hash(
    l_child: &Option<Box<SubProof>>,
    r_child: &Option<Box<SubProof>>,
) -> Digest {
    let mut state = blake2().to_state();

    let inner_hash = if let Some(l_c) = l_child {
        if let Some(r_c) = r_child {
            let mut inner_state = blake2().to_state();
            inner_state.update(l_c.to_digest().as_bytes());
            inner_state.update(r_c.to_digest().as_bytes());
            Digest::from(inner_state.finalize())
        } else {
            let mut inner_state = blake2().to_state();
            inner_state.update(l_c.to_digest().as_bytes());
            Digest::from(inner_state.finalize())
        }
    } else if let Some(r_c) = r_child {
        let mut inner_state = blake2().to_state();
        inner_state.update(r_c.to_digest().as_bytes());
        Digest::from(inner_state.finalize())
    } else {
        let inner_state = blake2().to_state();
        Digest::from(inner_state.finalize())
    };
    state.update(inner_hash.as_bytes());
    Digest::from(state.finalize())
}
