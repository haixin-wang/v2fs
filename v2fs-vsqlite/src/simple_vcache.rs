pub mod hash;
pub mod leaf;
pub mod non_leaf;

use lru::LruCache;

use crate::{
    digest::{Digest, Digestible},
    merkle_cb_tree::NodeId,
    vfs::PAGE_SIZE,
    PageId,
};

use self::{hash::merge_hash, leaf::SVCacheLeafNode, non_leaf::SVCacheNonLeafNode};

#[derive(Clone)]
pub(crate) enum SVCacheNode {
    Leaf(SVCacheLeafNode),
    NonLeaf(SVCacheNonLeafNode),
}

impl SVCacheNode {
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            SVCacheNode::Leaf(l) => l.is_valid(),
            SVCacheNode::NonLeaf(n) => n.is_valid(),
        }
    }

    fn validate(&mut self) {
        match self {
            SVCacheNode::Leaf(l) => l.validate(),
            SVCacheNode::NonLeaf(n) => n.validate(),
        }
    }

    pub(crate) fn get_id(&self) -> NodeId {
        match self {
            SVCacheNode::Leaf(l) => l.get_id(),
            SVCacheNode::NonLeaf(n) => n.get_id(),
        }
    }
}

impl Digestible for SVCacheNode {
    fn to_digest(&self) -> Digest {
        match self {
            SVCacheNode::Leaf(l) => l.to_digest(),
            SVCacheNode::NonLeaf(n) => n.to_digest(),
        }
    }
}

#[derive(Debug)]
pub struct SVCache {
    lru: LruCache<Digest, SVCacheNode>,
}

impl SVCache {
    pub fn new(cap: usize) -> Self {
        Self {
            lru: LruCache::<Digest, SVCacheNode>::new(cap),
        }
    }

    pub(crate) fn get_node(&mut self, key: &Digest) -> Option<&SVCacheNode> {
        self.lru.get(key)
    }

    pub(crate) fn get_node_mut(&mut self, key: &Digest) -> Option<&mut SVCacheNode> {
        self.lru.get_mut(key)
    }

    // push node into cache, if some node is popped out, remove all its parents
    pub(crate) fn push_node(&mut self, id: NodeId, node: SVCacheNode) {
        let hash = id.to_digest();
        if let Some((_, n)) = self.lru.push(hash, node) {
            let mut cur_id = n.get_id();
            let mut affected_pare_hashes = Vec::<Digest>::new();
            while let Some(p) = self.find_parent(cur_id) {
                match p {
                    SVCacheNode::Leaf(_) => {
                        panic!("impossible to be a leaf")
                    }
                    SVCacheNode::NonLeaf(n) => {
                        affected_pare_hashes.push(n.get_id().to_digest());
                        cur_id = n.get_id();
                    }
                }
            }
            for dig in affected_pare_hashes {
                self.lru.pop(&dig);
            }
        }
    }

    pub fn clear(&mut self) {
        self.lru.clear();
    }

    // change `is_valid` for all nodes to "false"
    pub fn unconfirm(&mut self) {
        for (_, cache_n) in self.lru.iter_mut() {
            match cache_n {
                SVCacheNode::Leaf(l) => {
                    l.unconfirm();
                }
                SVCacheNode::NonLeaf(n) => n.unconfirm(),
            }
        }
    }

    // change `is_valid` for all covered nodes of a sub-root to "true"
    pub(crate) fn confirm(&mut self, root_id: NodeId) {
        let mut covered_ids = Vec::new();
        let h = root_id.get_height();
        let w = root_id.get_width();
        for i in 0..h + 1 {
            let a = 2_i32.pow(h - i) as u32;
            for j in (w * a)..((w + 1) * a) {
                covered_ids.push(NodeId::new(i, j as u32));
            }
        }
        for id in covered_ids {
            if let Some(n) = self.get_node_mut(&id.to_digest()) {
                n.validate();
            } else {
                warn!("Cannot find a node during cache confirming");
            }
        }
    }

    // change `is_valid` for all covered nodes of a sub-root to "true" and set version
    pub(crate) fn confirm_with_version(&mut self, root_id: NodeId, version: u32) {
        let mut covered_ids = Vec::new();
        let h = root_id.get_height();
        let w = root_id.get_width();
        for i in 0..h + 1 {
            let a = 2_i32.pow(h - i) as u32;
            for j in (w * a)..((w + 1) * a) {
                covered_ids.push(NodeId::new(i, j as u32));
            }
        }

        for id in covered_ids {
            if let Some(n) = self.get_node_mut(&id.to_digest()) {
                match n {
                    SVCacheNode::Leaf(l) => {
                        l.validate_with_version(version);
                    }
                    SVCacheNode::NonLeaf(non) => {
                        non.validate();
                    }
                }
            } else {
                warn!("Cannot find a node during cache confirming");
            }
        }
    }

    pub fn cache_size_and_height(&self) -> (u32, u32) {
        let mut height = 0;
        let mut size = 0;
        for (_, n) in self.lru.iter() {
            match n {
                SVCacheNode::Leaf(_) => {
                    size += 4100;
                }
                SVCacheNode::NonLeaf(n) => {
                    size += 40;
                    let id = n.get_id();
                    let h = id.get_height();
                    if height < h {
                        height = h;
                    }
                }
            }
        }

        (size, height)
    }

    pub(crate) fn find_parent(&mut self, n_id: NodeId) -> Option<&SVCacheNode> {
        let height = n_id.get_height() + 1;
        let width = n_id.get_width() / 2;
        let parent_id = NodeId::new(height, width);
        self.get_node(&parent_id.to_digest())
    }

    pub(crate) fn find_sib(&mut self, id: NodeId) -> Option<&SVCacheNode> {
        let sib_key = id.get_sib_id().to_digest();
        if let Some(node) = self.get_node(&sib_key) {
            if node.is_valid() {
                return Some(node);
            }
        }
        None
    }

    pub(crate) fn has_sib(&self, id: NodeId) -> bool {
        let sib_key = id.get_sib_id().to_digest();
        self.lru.contains(&sib_key)
    }

    pub(crate) fn insert(
        &mut self,
        p_id: PageId,
        bytes: Box<[u8; PAGE_SIZE as usize]>,
        version: u32,
    ) {
        let new_n = SVCacheLeafNode::new(p_id, bytes, version);
        let mut cur_id = NodeId::from_page_id(p_id);
        let mut cur_hash = new_n.to_digest();
        self.push_node(cur_id, SVCacheNode::Leaf(new_n));

        while let Some(sib) = self.find_sib(cur_id) {
            let parent_id = cur_id.get_parent_id();
            let parent_hash = if cur_id.is_even() {
                merge_hash(&cur_hash, &sib.to_digest())
            } else {
                merge_hash(&sib.to_digest(), &cur_hash)
            };
            let parent = SVCacheNonLeafNode::new(parent_id, parent_hash);
            cur_hash = parent_hash;
            cur_id = parent_id;
            self.push_node(parent_id, SVCacheNode::NonLeaf(parent));
        }
    }
}
