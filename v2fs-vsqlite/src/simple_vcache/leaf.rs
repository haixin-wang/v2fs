use crate::{
    digest::{Digest, Digestible},
    merkle_cb_tree::NodeId,
    vfs::PAGE_SIZE,
    PageId,
};

use super::hash::leaf_hash;

#[derive(Clone)]
pub(crate) struct SVCacheLeafNode {
    id: NodeId,
    bytes: Box<[u8; PAGE_SIZE as usize]>,
    version: u32,
    is_valid: bool,
}

impl SVCacheLeafNode {
    pub(crate) fn new(p_id: PageId, bytes: Box<[u8; PAGE_SIZE as usize]>, version: u32) -> Self {
        Self {
            id: NodeId::new(0, p_id.get_id()),
            bytes,
            version,
            is_valid: true,
        }
    }

    pub(crate) fn get_id(&self) -> NodeId {
        self.id
    }

    pub(crate) fn get_version(&self) -> u32 {
        self.version
    }

    pub(crate) fn get_bytes(&self) -> Box<[u8; PAGE_SIZE as usize]> {
        self.bytes.clone()
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub(crate) fn unconfirm(&mut self) {
        self.is_valid = false;
    }

    pub(crate) fn validate(&mut self) {
        self.is_valid = true;
    }

    pub(crate) fn validate_with_version(&mut self, version: u32) {
        self.is_valid = true;
        self.version = version;
    }
}

impl Digestible for SVCacheLeafNode {
    fn to_digest(&self) -> Digest {
        leaf_hash(self.id.get_width(), &self.bytes.to_digest())
    }
}
