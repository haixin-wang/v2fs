use super::{
    hash::{leaf_hash, nonleaf_hash},
    MerkleNode, MerkleNodeLoader, NodeId,
};
use crate::{
    digest::{Digest, Digestible},
    PageId,
};
use anyhow::Result;
use std::collections::HashMap;

pub struct Apply {
    pub root_id: Option<NodeId>,
    pub nodes: HashMap<Digest, MerkleNode>,
}

impl Apply {
    fn set_root_id(&mut self, new_id: NodeId) {
        self.root_id = Some(new_id);
    }

    pub fn get_root_id(&self) -> Option<NodeId> {
        self.root_id
    }
}

pub struct WriteContext<'a, L: MerkleNodeLoader> {
    node_loader: &'a L,
    apply: Apply,
}

impl<'a, L: MerkleNodeLoader> WriteContext<'a, L> {
    pub fn new(node_loader: &'a L, root_id: Option<NodeId>) -> Self {
        Self {
            node_loader,
            apply: Apply {
                root_id,
                nodes: HashMap::new(),
            },
        }
    }

    pub(crate) fn get_height(&self) -> u32 {
        let root_id = self.apply.root_id;
        if let Some(id) = root_id {
            id.get_height()
        } else {
            0
        }
    }

    pub(crate) fn set_root_id(&mut self, new_id: NodeId) {
        self.apply.set_root_id(new_id);
    }

    pub fn changes(self) -> Apply {
        self.apply
    }

    fn get_node(&self, id: &NodeId) -> Result<Option<MerkleNode>> {
        Ok(match self.apply.nodes.get(&id.to_digest()) {
            Some(n) => Some(n.clone()),
            None => self.node_loader.load_node(id)?,
        })
    }

    fn write_node(&mut self, id: NodeId, n: MerkleNode) {
        self.apply.nodes.insert(id.to_digest(), n);
    }

    pub fn update(&mut self, p_hash: Digest, p_id: PageId) -> Result<()> {
        // insert if not exist
        let height = self.get_height();
        let id = NodeId::from_page_id(p_id);
        let leaf_hash = leaf_hash(&p_id, &p_hash);
        self.write_node(id, MerkleNode::new(leaf_hash));

        if height == 0 {
            if self.apply.root_id.is_none() || p_id.get_id() == 0 {
                self.set_root_id(id);
                return Ok(());
            } else {
                let root_id = self.apply.root_id.expect("Impossible");
                let cur_root = self.get_node(&root_id)?.expect("Cannot find cur root");
                let cur_root_hash = cur_root.get_hash();
                let non_leaf_hash = nonleaf_hash(Some(cur_root_hash), Some(leaf_hash));
                let new_root_id = id.get_parent_id();
                self.write_node(new_root_id, MerkleNode::new(non_leaf_hash));
                self.set_root_id(new_root_id);
                return Ok(());
            }
        }

        let mut cur_root_id = id;
        let mut cur_root_hash = leaf_hash;

        let p_id_target_height = find_height(p_id);
        let target_height = if p_id_target_height > height {
            // old tree become sub_tree, crate an entire path
            p_id_target_height
        } else {
            // insert/update to old tree
            height
        };

        while cur_root_id.get_height() < target_height {
            let sib_id = cur_root_id.get_sib_id();
            let sib_n = self.get_node(&sib_id)?;
            match sib_n {
                Some(sib) => {
                    let sib_hash = sib.get_hash();
                    if cur_root_id.is_even() {
                        cur_root_hash = nonleaf_hash(Some(cur_root_hash), Some(sib_hash));
                    } else {
                        cur_root_hash = nonleaf_hash(Some(sib_hash), Some(cur_root_hash));
                    }
                }
                None => {
                    if cur_root_id.is_even() {
                        cur_root_hash = nonleaf_hash(Some(cur_root_hash), None);
                    } else {
                        cur_root_hash = nonleaf_hash(None, Some(cur_root_hash));
                    }
                }
            }
            cur_root_id = cur_root_id.get_parent_id();
            self.write_node(cur_root_id, MerkleNode::new(cur_root_hash));
        }
        self.set_root_id(cur_root_id);

        Ok(())
    }
}

fn find_height(p_id: PageId) -> u32 {
    let mut p_id_num = p_id.get_id();
    let mut height = 0;
    while p_id_num != 0 {
        height += 1;
        p_id_num /= 2;
    }
    height
}

#[cfg(test)]
mod tests {
    use super::find_height;
    use crate::PageId;

    #[test]
    fn test_find_height() {
        assert_eq!(find_height(PageId(0)), 0);
        assert_eq!(find_height(PageId(1)), 1);
        assert_eq!(find_height(PageId(2)), 2);
        assert_eq!(find_height(PageId(3)), 2);
        assert_eq!(find_height(PageId(4)), 3);
        assert_eq!(find_height(PageId(5)), 3);
        assert_eq!(find_height(PageId(6)), 3);
        assert_eq!(find_height(PageId(7)), 3);
        assert_eq!(find_height(PageId(8)), 4);
        assert_eq!(find_height(PageId(15)), 4);
        assert_eq!(find_height(PageId(16)), 5);
        assert_eq!(find_height(PageId(31)), 5);
        assert_eq!(find_height(PageId(32)), 6);
    }
}
