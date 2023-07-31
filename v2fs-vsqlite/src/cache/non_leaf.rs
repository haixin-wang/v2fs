use super::*;

#[derive(Clone)]
pub(crate) struct CacheNonLeafNode {
    id: NodeId,
    hash: Digest,
    is_valid: bool,
}

impl CacheNonLeafNode {
    pub(crate) fn new(id: NodeId, hash: Digest) -> Self {
        Self {
            id,
            hash,
            is_valid: true,
        }
    }

    pub(crate) fn get_id(&self) -> NodeId {
        self.id
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

impl Digestible for CacheNonLeafNode {
    fn to_digest(&self) -> Digest {
        self.hash
    }
}
