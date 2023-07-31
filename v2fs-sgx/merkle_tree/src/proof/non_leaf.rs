use crate::hash::proof_nonleaf_hash;
use alloc::{boxed::Box, vec::Vec};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use vfs_common::digest::{Digest, Digestible};

use super::sub_proof::SubProof;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct ProofNonLeaf {
    pub(crate) l_child: Option<Box<SubProof>>,
    pub(crate) r_child: Option<Box<SubProof>>,
}

impl ProofNonLeaf {
    pub(crate) fn get_l_child_mut(&mut self) -> &mut Option<Box<SubProof>> {
        &mut self.l_child
    }

    pub(crate) fn get_r_child_mut(&mut self) -> &mut Option<Box<SubProof>> {
        &mut self.r_child
    }

    pub(crate) fn value_hash(&self, cur_path_rev: &mut Vec<usize>) -> Result<Digest> {
        let c_idx = cur_path_rev.pop().expect("empty index path");
        if c_idx == 0 {
            let l_c = self
                .l_child
                .as_ref()
                .expect("the proof node doesn't have left child");
            l_c.value_hash(cur_path_rev)
        } else {
            let r_c = self
                .r_child
                .as_ref()
                .expect("the proof node doesn't have right node");
            r_c.value_hash(cur_path_rev)
        }
    }
}

impl Digestible for ProofNonLeaf {
    fn to_digest(&self) -> Digest {
        proof_nonleaf_hash(&self.l_child, &self.r_child)
    }
}
