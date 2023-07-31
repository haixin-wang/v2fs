use crate::{
    proof::{non_leaf::ProofNonLeaf, sub_proof::SubProof, Proof},
    storage::{MerkleNodeLoader, NodeId},
};
use alloc::{boxed::Box, vec::Vec};
use anyhow::{bail, Result};
use vfs_common::{digest::Digest, page::PageId};

pub struct ReadContext<'a, L: MerkleNodeLoader> {
    node_loader: &'a L,
    root_id: Option<NodeId>,
    proof: Proof,
}

impl<'a, L: MerkleNodeLoader> ReadContext<'a, L> {
    pub fn new(node_loader: &'a L, root_id: Option<NodeId>) -> Result<Self> {
        match root_id {
            Some(id) => Ok(Self {
                node_loader,
                root_id: Some(id),
                proof: Proof::default(),
            }),
            None => {
                bail!("The merkle tree does not have root!");
            }
        }
    }

    pub fn into_proof(self) -> Proof {
        self.proof
    }

    pub fn query(&mut self, p_id: PageId) -> Result<Digest> {
        match self.proof.root.as_mut() {
            Some(root) => {
                let height = self.root_id.expect("Empty merkle tree").get_height();
                let mut path_rev = get_path_rev(p_id, height);
                let (sub_proof, sub_root_id) = root.search_prefix(NodeId::new(0, 1), &mut path_rev);
                let (v, p) = inner_query(self.node_loader, sub_root_id, p_id)?;
                unsafe {
                    *sub_proof = p;
                }
                Ok(v)
            }
            None => {
                let (v, p) = query_from_beginning(self.node_loader, self.root_id, p_id)?;
                self.proof = p;
                Ok(v)
            }
        }
    }
}

fn query_from_beginning(
    node_loader: &impl MerkleNodeLoader,
    root_id: Option<NodeId>,
    p_id: PageId,
) -> Result<(Digest, Proof)> {
    match root_id {
        Some(root_id) => {
            let (v, p) = inner_query(node_loader, root_id, p_id)?;
            Ok((v, Proof::from_subproof(p)))
        }
        None => bail!("The merkle tree is empty"),
    }
}

fn inner_query(
    node_loader: &impl MerkleNodeLoader,
    sub_root_id: NodeId,
    p_id: PageId,
) -> Result<(Digest, SubProof)> {
    let target_id = NodeId::from_page_id(p_id);
    let target_node = node_loader
        .load_node(&target_id)?
        .expect("impossible that cannot find the leaf");

    let query_val = target_node.get_hash();
    let height = sub_root_id.get_height();
    let mut cur_height = 0;
    let mut cur_id = target_id;

    let mut cur_proof = SubProof::from_hash(target_node.get_hash());

    while cur_height < height {
        let sib_n = node_loader.load_node(&cur_id.get_sib_id())?;
        let proof_leaf_sib = match sib_n {
            Some(n) => Some(Box::new(SubProof::from_hash(n.get_hash()))),
            None => None,
        };
        if cur_id.is_even() {
            let mut non_leaf = ProofNonLeaf {
                l_child: None,
                r_child: proof_leaf_sib,
            };
            *non_leaf.get_l_child_mut() = Some(Box::new(cur_proof));

            cur_proof = SubProof::from_non_leaf(non_leaf);
        } else {
            let mut non_leaf = ProofNonLeaf {
                l_child: proof_leaf_sib,
                r_child: None,
            };
            *non_leaf.get_r_child_mut() = Some(Box::new(cur_proof));

            cur_proof = SubProof::from_non_leaf(non_leaf);
        }

        cur_id = cur_id.get_parent_id();
        cur_height += 1;
    }

    Ok((query_val, cur_proof))
}

fn get_path_rev(p_id: PageId, height: u32) -> Vec<(usize, NodeId)> {
    let mut res = vec![];
    let mut cur_id = NodeId::from_page_id(p_id);
    let mut cur_height = 0;
    while cur_height < height {
        if cur_id.is_even() {
            res.push((0, cur_id));
        } else {
            res.push((1, cur_id));
        }
        cur_id = cur_id.get_parent_id();
        cur_height += 1;
    }

    res
}

pub(crate) fn get_idx_path_rev(p_id: PageId, height: u32) -> Vec<usize> {
    let mut res = vec![];
    let mut cur_num = p_id.get_id();
    let mut cur_height = 0;
    while cur_height < height {
        if cur_num % 2 == 0 {
            res.push(0);
        } else {
            res.push(1);
        }
        cur_num /= 2;
        cur_height += 1;
    }
    res
}
