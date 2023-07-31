use crate::{
    digest::Digest,
    merkle_cb_tree::{hash::leaf_hash, proof::Proof},
    utils::compare_with_root,
    PageId,
};
use anyhow::Result;
use std::collections::HashMap;

pub(crate) fn verify(height: u32, proof: &Proof, map: &HashMap<PageId, Digest>) -> Result<()> {
    if !map.is_empty() {
        let computed_root_hash = proof.root_hash()?;
        compare_with_root(computed_root_hash)?;
        for (p_id, dig) in map.iter() {
            let target_hash = leaf_hash(p_id, dig);
            proof.verify_val(target_hash, *p_id, height)?;
        }
    }
    Ok(())
}
