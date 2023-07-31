use crate::digest::{blake2, Digest};

/// H(pid||p_hash)
#[inline]
pub(crate) fn leaf_hash(p_id: u32, p_hash: &Digest) -> Digest {
    let mut state = blake2().to_state();
    state.update(&p_id.to_le_bytes());
    state.update(p_hash.as_bytes());
    Digest::from(state.finalize())
}

/// H(H(l_hash||r_hash))
#[inline]
pub(crate) fn merge_hash(l_hash: &Digest, r_hash: &Digest) -> Digest {
    let mut state = blake2().to_state();
    state.update(l_hash.as_bytes());
    state.update(r_hash.as_bytes());
    let inner_hash = Digest::from(state.finalize());
    let mut state = blake2().to_state();
    state.update(inner_hash.as_bytes());
    Digest::from(state.finalize())
}
