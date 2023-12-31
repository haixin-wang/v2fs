use super::SmallCString;
use std::cell::RefCell;
use std::collections::BTreeMap;

/// Maps parameter names to parameter indices.
#[derive(Default, Clone, Debug)]
// BTreeMap seems to do better here unless we want to pull in a custom hash
// function.
pub(crate) struct ParamIndexCache(RefCell<BTreeMap<SmallCString, usize>>);

impl ParamIndexCache {
    pub fn get_or_insert_with<F>(&self, s: &str, func: F) -> Option<usize>
    where
        F: FnOnce(&std::ffi::CStr) -> Option<usize>,
    {
        let mut cache = self.0.borrow_mut();
        // Avoid entry API, needs allocation to test membership.
        if let Some(v) = cache.get(s) {
            return Some(*v);
        }
        // If there's an internal nul in the name it couldn't have been a
        // parameter, so early return here is ok.
        let name = SmallCString::new(s).ok()?;
        let val = func(&name)?;
        cache.insert(name, val);
        Some(val)
    }
}
