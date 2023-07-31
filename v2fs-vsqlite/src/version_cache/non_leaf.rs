use std::collections::HashSet;

use super::*;

#[derive(Clone)]
pub(crate) struct VCacheNonLeafNode {
    id: NodeId,
    hash: Digest,
    version: u32,
    idxes: HashSet<usize>,
    is_valid: bool,
}

impl VCacheNonLeafNode {
    pub(crate) fn new(id: NodeId, hash: Digest, version: u32, idxes: HashSet<usize>) -> Self {
        Self {
            id,
            hash,
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

    fn set_version(&mut self, version: u32) {
        self.version = version;
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.is_valid
    }

    pub(crate) fn get_set(&self) -> &HashSet<usize> {
        &self.idxes
    }

    pub(crate) fn unconfirm(&mut self) {
        self.is_valid = false;
    }

    pub(crate) fn validate(&mut self) {
        self.is_valid = true;
    }

    pub(crate) fn validate_with_version(&mut self, version: u32) {
        self.is_valid = true;
        self.set_version(version);
    }
}

impl Digestible for VCacheNonLeafNode {
    fn to_digest(&self) -> Digest {
        self.hash
    }
}
