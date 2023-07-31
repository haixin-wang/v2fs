use std::{
    collections::{
        hash_map::{DefaultHasher, RandomState},
        HashSet,
    },
    hash::{BuildHasher, Hash, Hasher},
};

use crate::PageId;

#[derive(Clone, Debug, Default)]
struct VersionMap {
    map: Vec<u32>,
}

impl VersionMap {
    pub fn new(size: usize) -> Self {
        Self { map: vec![0; size] }
    }

    fn set(&mut self, idx: usize, val: u32) {
        self.map[idx] = val;
    }

    fn get(&self, idx: usize) -> u32 {
        self.map[idx]
    }
}

#[derive(Clone, Debug)]
pub struct VersionBloomFilter {
    vmap: VersionMap,
    map_size: u64,
    hash_num: u32,
    hashers: [DefaultHasher; 2],
}

impl VersionBloomFilter {
    pub fn new(map_size: usize, hash_num: u32) -> Self {
        let hashers = [
            RandomState::new().build_hasher(),
            RandomState::new().build_hasher(),
        ];

        Self {
            vmap: VersionMap::new(map_size),
            map_size: map_size as u64,
            hash_num,
            hashers,
        }
    }

    fn get_map_val(&self, idx: usize) -> u32 {
        let map = &self.vmap;
        map.get(idx)
    }

    pub fn insert(&mut self, p_id: PageId, version: u32) {
        let (h1, h2) = self.hash_kernel(p_id);
        for i in 0..self.hash_num {
            let idx = self.get_idx(h1, h2, i as u64);
            self.vmap.set(idx, version);
        }
    }

    pub fn get_bf_pos(&self, p_id: PageId) -> HashSet<usize> {
        let mut set = HashSet::new();
        let (h1, h2) = self.hash_kernel(p_id);
        for i in 0..self.hash_num {
            let idx = self.get_idx(h1, h2, i as u64);
            set.insert(idx);
        }
        set
    }

    pub fn contains(&self, p_id: PageId, version: u32) -> bool {
        let (h1, h2) = self.hash_kernel(p_id);
        for i in 0..self.hash_num {
            let idx = self.get_idx(h1, h2, i as u64);

            let v_in_bf = self.get_map_val(idx);
            if v_in_bf <= version {
                // when equal, BF may contain, but no need to validate
                return false;
            }
        }
        true
    }

    pub fn contains_subroot(&self, idxes: &HashSet<usize>, version: u32) -> bool {
        for idx in idxes {
            let v_in_bf = self.get_map_val(*idx);
            if v_in_bf > version {
                return true;
            }
        }

        false
    }

    fn hash_kernel(&self, p_id: PageId) -> (u64, u64) {
        let hasher1 = &mut self.hashers[0].clone();
        let hasher2 = &mut self.hashers[1].clone();
        p_id.hash(hasher1);
        p_id.hash(hasher2);

        let hash1 = hasher1.finish();
        let hash2 = hasher2.finish();

        (hash1, hash2)
    }

    // g(x) = h1(x) + ih2(x)
    fn get_idx(&self, h1: u64, h2: u64, hash_id: u64) -> usize {
        (h1.wrapping_add((hash_id).wrapping_mul(h2)) % self.map_size) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vbf() {
        let mut vbf = VersionBloomFilter::new(100, 4);
        vbf.insert(PageId(1), 0);
        vbf.insert(PageId(2), 0);
        assert!(!vbf.contains(PageId(1), 1));
        vbf.insert(PageId(3), 0);
        assert!(!vbf.contains(PageId(1), 2));
        assert!(!vbf.contains(PageId(2), 1));
        assert!(!vbf.contains(PageId(3), 1));
        assert!(!vbf.contains(PageId(2), 2));
        assert!(!vbf.contains(PageId(2), 0));
        vbf.insert(PageId(4), 1);
        assert!(vbf.contains(PageId(4), 0));
    }
}
