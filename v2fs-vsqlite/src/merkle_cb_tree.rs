use crate::{
    digest::{Digest, Digestible},
    PageId,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};

use self::hash::id_hash;

pub mod hash;
pub mod proof;
pub mod read;
pub mod write;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct NodeId(u32, u32);

impl NodeId {
    pub(crate) fn new(height: u32, width: u32) -> Self {
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

    pub fn get_sib_id(&self) -> Self {
        let h = self.get_height();
        let w = self.get_width();
        if w % 2 == 0 {
            Self(h, w + 1)
        } else {
            Self(h, w - 1)
        }
    }

    pub(crate) fn is_even(&self) -> bool {
        self.get_width() % 2 == 0
    }
}

impl Digestible for NodeId {
    fn to_digest(&self) -> Digest {
        id_hash(self.0, self.1)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleNode {
    hash: Digest,
}

impl MerkleNode {
    fn new(hash: Digest) -> Self {
        Self { hash }
    }

    pub fn get_hash(&self) -> Digest {
        self.hash
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

#[cfg(test)]
pub mod tests;
