use crate::hash::id_hash;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use vfs_common::digest::{Digest, Digestible};
use vfs_common::page::PageId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleNode {
    hash: Digest,
}

impl MerkleNode {
    pub fn new(hash: Digest) -> Self {
        Self { hash }
    }

    pub fn get_hash(&self) -> Digest {
        self.hash
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct NodeId(u32, u32);

impl NodeId {
    pub fn new(height: u32, width: u32) -> Self {
        Self(height, width)
    }

    pub fn from_page_id(p_id: PageId) -> Self {
        Self(0, p_id.get_id())
    }

    pub fn get_height(&self) -> u32 {
        self.0
    }

    pub fn get_width(&self) -> u32 {
        self.1
    }

    pub fn get_parent_id(&self) -> Self {
        let h = self.get_height();
        let w = self.get_width();
        Self(h + 1, w / 2)
    }

    pub fn get_sib_id(&self) -> NodeId {
        let h = self.get_height();
        let w = self.get_width();
        if w % 2 == 0 {
            NodeId(h, w + 1)
        } else {
            NodeId(h, w - 1)
        }
    }

    pub fn is_even(&self) -> bool {
        self.get_width() % 2 == 0
    }

    pub fn is_leaf(&self) -> bool {
        self.0 == 0
    }

    pub fn get_children(&self) -> Result<(Self, Self)> {
        let h = self.get_height();
        if h <= 0 {
            bail!("leaf node does not have children");
        }
        let w = self.get_width();
        Ok((Self(h - 1, 2 * w), Self(h - 1, 2 * w + 1)))
    }
}

impl Digestible for NodeId {
    fn to_digest(&self) -> Digest {
        id_hash(self.0, self.1)
    }
}

pub trait ReadInterface {
    fn get_node(&self, addr: &Digest) -> Result<Option<MerkleNode>>;
}

pub trait WriteInterface {
    fn write_node(&mut self, addr: &Digest, node: &MerkleNode) -> Result<()>;
}

pub trait MerkleNodeLoader {
    fn load_node(&self, id: &NodeId) -> Result<Option<MerkleNode>>;
}

impl<Interface: ReadInterface> MerkleNodeLoader for Interface {
    fn load_node(&self, id: &NodeId) -> Result<Option<MerkleNode>> {
        self.get_node(&id.to_digest())
    }
}
