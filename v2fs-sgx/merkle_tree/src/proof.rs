use crate::{proof::sub_proof::SubProof, read::get_idx_path_rev};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use vfs_common::{
    digest::{Digest, Digestible},
    page::PageId,
};

pub(crate) mod non_leaf;
pub(crate) mod sub_proof;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Proof {
    pub(crate) root: Option<SubProof>,
}

impl Proof {
    pub(crate) fn from_subproof(sub_p: SubProof) -> Self {
        Self { root: Some(sub_p) }
    }

    fn value_hash(&self, p_id: PageId, height: u32) -> Result<Digest> {
        let mut path_rev = get_idx_path_rev(p_id, height);
        match self.root.as_ref() {
            None => {
                bail!("Proof is none")
            }
            Some(root) => root.value_hash(&mut path_rev),
        }
    }

    // target_hash = H(p_id||p_hash)
    pub fn verify_val(&self, target_hash: Digest, p_id: PageId, height: u32) -> Result<()> {
        let hash_in_proof = self.value_hash(p_id, height)?;
        anyhow::ensure!(
            target_hash == hash_in_proof,
            "Page hash not matched! The mismatched page id is {:?}, the target hash is {:?}, the computed hash is {:?}.",
            p_id,
            target_hash,
            hash_in_proof
        );
        Ok(())
    }

    pub fn root_hash(&self) -> Result<Digest> {
        match self.root.as_ref() {
            Some(root) => Ok(root.to_digest()),
            None => bail!("empty proof"),
        }
    }
}
