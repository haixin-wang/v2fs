use super::{hash::leaf_hash, *};

#[derive(Clone)]
pub(crate) struct CacheLeafNode {
    id: NodeId,
    bytes: Box<[u8; PAGE_SIZE as usize]>,
    is_valid: bool,
}

impl CacheLeafNode {
    pub(crate) fn new(p_id: PageId, bytes: Box<[u8; PAGE_SIZE as usize]>) -> Self {
        Self {
            id: NodeId::new(0, p_id.get_id()),
            bytes,
            is_valid: true,
        }
    }

    pub(crate) fn get_id(&self) -> NodeId {
        self.id
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
}

impl Digestible for CacheLeafNode {
    fn to_digest(&self) -> Digest {
        leaf_hash(self.id.get_width(), &self.bytes.to_digest())
    }
}
