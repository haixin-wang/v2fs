use super::non_leaf::ProofNonLeaf;
use crate::storage::NodeId;
use alloc::{boxed::Box, vec::Vec};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use vfs_common::digest::{Digest, Digestible};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) enum SubProof {
    Leaf(Digest),
    NonLeaf(Box<ProofNonLeaf>),
}

impl SubProof {
    pub(crate) fn from_hash(h: Digest) -> Self {
        Self::Leaf(h)
    }

    pub(crate) fn from_non_leaf(n: ProofNonLeaf) -> Self {
        Self::NonLeaf(Box::new(n))
    }

    pub(crate) fn search_prefix<'a>(
        &mut self,
        sub_root_id: NodeId,
        cur_path_rev: &mut Vec<(usize, NodeId)>,
    ) -> (*mut SubProof, NodeId) {
        match self {
            SubProof::Leaf(_) => (self as *mut _, sub_root_id),
            SubProof::NonLeaf(n) => {
                let (c_idx, id) = cur_path_rev.pop().expect("empty path");
                if c_idx == 0 {
                    let l_child = n
                        .get_l_child_mut()
                        .as_mut()
                        .expect("should have left child in sub-proof");
                    l_child.search_prefix(id, cur_path_rev)
                } else {
                    let r_child = n
                        .get_r_child_mut()
                        .as_mut()
                        .expect("should have right child in sub-proof");
                    r_child.search_prefix(id, cur_path_rev)
                }
            }
        }
    }

    pub(crate) fn value_hash(&self, cur_path_rev: &mut Vec<usize>) -> Result<Digest> {
        match self {
            SubProof::Leaf(l) => Ok(*l),
            SubProof::NonLeaf(n) => n.value_hash(cur_path_rev),
        }
    }
}

impl Digestible for SubProof {
    fn to_digest(&self) -> Digest {
        match self {
            SubProof::Leaf(l) => *l,
            SubProof::NonLeaf(n) => n.to_digest(),
        }
    }
}
