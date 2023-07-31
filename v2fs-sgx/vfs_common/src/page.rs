use alloc::boxed::Box;
use crate::{digest::{Digest, Digestible}, PAGE_SIZE};
use serde::{Deserialize, Serialize};
use alloc::vec::Vec;

#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct PageId(pub u32);

impl PageId {
    pub fn get_id(&self) -> u32 {
        self.0
    }
    
    pub fn find_height(&self) -> u32 {
        let mut p_id_num = self.get_id();
        let mut height = 0;
        while p_id_num != 0 {
            height += 1;
            p_id_num /= 2;
        }
        height
    }
}

impl Digestible for PageId {
    fn to_digest(&self) -> Digest {
        self.0.to_digest()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Page {
    bytes: Box<[u8; PAGE_SIZE]>,
}

impl Page {
    pub fn new(bytes: Box<[u8; PAGE_SIZE]>) -> Self {
        Self { bytes }
    }

    pub fn get_bytes(&self) -> Vec<u8> {
        self.bytes.to_vec()
    }
}

impl Digestible for Page {
    fn to_digest(&self) -> Digest {
        self.bytes.to_digest()
    }
}

