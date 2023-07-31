use super::{hash::leaf_hash, *};
use std::collections::HashSet;

#[derive(Clone)]
pub(crate) struct VCacheLeafNode {
    id: NodeId,
    bytes: Box<[u8; PAGE_SIZE as usize]>,
    version: u32,
    idxes: HashSet<usize>,
    is_valid: bool,
}

impl VCacheLeafNode {
    pub(crate) fn new(
        p_id: PageId,
        bytes: Box<[u8; PAGE_SIZE as usize]>,
        version: u32,
        idxes: HashSet<usize>,
    ) -> Self {
        Self {
            id: NodeId::new(0, p_id.get_id()),
            bytes,
            version,
            idxes,
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

    pub(crate) fn get_set(&self) -> &HashSet<usize> {
        &self.idxes
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

impl Digestible for VCacheLeafNode {
    fn to_digest(&self) -> Digest {
        leaf_hash(self.id.get_width(), &self.bytes.to_digest())
    }
}
