use super::{
    read::ReadContext,
    write::{Apply, WriteContext},
    MerkleNode, MerkleNodeLoader, NodeId,
};
use crate::{
    digest::{Digest, Digestible},
    merkle_cb_tree::hash::leaf_hash,
    // merkle_cb_tree::hash::nonleaf_hash,
    utils::init_tracing_subscriber,
    PageId,
};
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct TestTree {
    root_id: Option<NodeId>,
    nodes: HashMap<Digest, MerkleNode>,
}

impl MerkleNodeLoader for TestTree {
    fn load_node(&self, id: &NodeId) -> Result<Option<MerkleNode>> {
        Ok(self.nodes.get(&id.to_digest()).cloned())
    }
}

impl TestTree {
    pub fn new() -> Self {
        Self {
            root_id: None,
            nodes: HashMap::new(),
        }
    }

    fn get_height(&self) -> Option<u32> {
        if let Some(id) = self.root_id {
            Some(id.get_height())
        } else {
            None
        }
    }

    fn apply(&mut self, apply: Apply) {
        self.root_id = apply.root_id;
        self.nodes.extend(apply.nodes.into_iter());
    }
}

fn build_tree() -> TestTree {
    let mut merkle_tree = TestTree::new();
    let mut ctx = WriteContext::new(&merkle_tree, None);
    ctx.update("old_page0".to_digest(), PageId(0)).unwrap();
    ctx.update("old_page1".to_digest(), PageId(1)).unwrap();
    ctx.update("old_page2".to_digest(), PageId(2)).unwrap();
    ctx.update("old_page3".to_digest(), PageId(3)).unwrap();
    ctx.update("old_page4".to_digest(), PageId(4)).unwrap();
    ctx.update("old_page5".to_digest(), PageId(5)).unwrap();
    ctx.update("old_page6".to_digest(), PageId(6)).unwrap();
    ctx.update("old_page7".to_digest(), PageId(7)).unwrap();
    ctx.update("old_page8".to_digest(), PageId(8)).unwrap();

    let changes = ctx.changes();
    merkle_tree.apply(changes);
    merkle_tree
}

// #[test]
// fn test_insert() -> Result<()> {
//     init_tracing_subscriber("info")?;

//     let merkle_tree = build_tree();

//     assert_eq!(merkle_tree.root_id, Some(NodeId::new(4, 0)));
//     assert_eq!(20, merkle_tree.nodes.len());

//     let hash0 = leaf_hash(&PageId(0), &"old_page0".to_digest());
//     let hash1 = leaf_hash(&PageId(1), &"old_page1".to_digest());
//     let hash2 = leaf_hash(&PageId(2), &"old_page2".to_digest());
//     let hash3 = leaf_hash(&PageId(3), &"old_page3".to_digest());
//     let hash4 = leaf_hash(&PageId(4), &"old_page4".to_digest());
//     let hash5 = leaf_hash(&PageId(5), &"old_page5".to_digest());
//     let hash6 = leaf_hash(&PageId(6), &"old_page6".to_digest());
//     let hash7 = leaf_hash(&PageId(7), &"old_page7".to_digest());
//     let hash8 = leaf_hash(&PageId(8), &"old_page8".to_digest());

//     let hash01 = nonleaf_hash(Some(hash0), Some(hash1));
//     let hash23 = nonleaf_hash(Some(hash2), Some(hash3));
//     let hash45 = nonleaf_hash(Some(hash4), Some(hash5));
//     let hash67 = nonleaf_hash(Some(hash6), Some(hash7));
//     let hash89 = nonleaf_hash(Some(hash8), None);

//     let hash03 = nonleaf_hash(Some(hash01), Some(hash23));
//     let hash47 = nonleaf_hash(Some(hash45), Some(hash67));
//     let hash811 = nonleaf_hash(Some(hash89), None);

//     let hash07 = nonleaf_hash(Some(hash03), Some(hash47));
//     let hash815 = nonleaf_hash(Some(hash811), None);

//     let hash015 = nonleaf_hash(Some(hash07), Some(hash815));

//     let n = merkle_tree.load_node(&NodeId::new(0, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash0);
//     let n = merkle_tree.load_node(&NodeId::new(0, 1))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash1);
//     let n = merkle_tree.load_node(&NodeId::new(0, 2))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash2);
//     let n = merkle_tree.load_node(&NodeId::new(0, 3))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash3);
//     let n = merkle_tree.load_node(&NodeId::new(0, 4))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash4);
//     let n = merkle_tree.load_node(&NodeId::new(0, 5))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash5);
//     let n = merkle_tree.load_node(&NodeId::new(0, 6))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash6);
//     let n = merkle_tree.load_node(&NodeId::new(0, 7))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash7);
//     let n = merkle_tree.load_node(&NodeId::new(0, 8))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash8);

//     let n = merkle_tree.load_node(&NodeId::new(1, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash01);
//     let n = merkle_tree.load_node(&NodeId::new(1, 1))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash23);
//     let n = merkle_tree.load_node(&NodeId::new(1, 2))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash45);
//     let n = merkle_tree.load_node(&NodeId::new(1, 3))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash67);
//     let n = merkle_tree.load_node(&NodeId::new(1, 4))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash89);

//     let n = merkle_tree.load_node(&NodeId::new(2, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash03);
//     let n = merkle_tree.load_node(&NodeId::new(2, 1))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash47);
//     let n = merkle_tree.load_node(&NodeId::new(2, 2))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash811);

//     let n = merkle_tree.load_node(&NodeId::new(3, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash07);
//     let n = merkle_tree.load_node(&NodeId::new(3, 1))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash815);

//     let n = merkle_tree.load_node(&NodeId::new(4, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash015);

//     Ok(())
// }

// #[test]
// fn test_update() -> Result<()> {
//     init_tracing_subscriber("info")?;
//     let mut merkle_tree = build_tree();
//     let root_id = merkle_tree.root_id;
//     let mut ctx = WriteContext::new(&merkle_tree, root_id);
//     ctx.update("new_page1".to_digest(), PageId(1)).unwrap();
//     ctx.update("new_page3".to_digest(), PageId(3)).unwrap();
//     ctx.update("new_page5".to_digest(), PageId(5)).unwrap();
//     let changes = ctx.changes();
//     merkle_tree.apply(changes);

//     assert_eq!(merkle_tree.root_id, Some(NodeId::new(4, 0)));
//     assert_eq!(20, merkle_tree.nodes.len());

//     let hash0 = leaf_hash(&PageId(0), &"old_page0".to_digest());
//     let hash1 = leaf_hash(&PageId(1), &"new_page1".to_digest());
//     let hash2 = leaf_hash(&PageId(2), &"old_page2".to_digest());
//     let hash3 = leaf_hash(&PageId(3), &"new_page3".to_digest());
//     let hash4 = leaf_hash(&PageId(4), &"old_page4".to_digest());
//     let hash5 = leaf_hash(&PageId(5), &"new_page5".to_digest());
//     let hash6 = leaf_hash(&PageId(6), &"old_page6".to_digest());
//     let hash7 = leaf_hash(&PageId(7), &"old_page7".to_digest());
//     let hash8 = leaf_hash(&PageId(8), &"old_page8".to_digest());

//     let hash01 = nonleaf_hash(Some(hash0), Some(hash1));
//     let hash23 = nonleaf_hash(Some(hash2), Some(hash3));
//     let hash45 = nonleaf_hash(Some(hash4), Some(hash5));
//     let hash67 = nonleaf_hash(Some(hash6), Some(hash7));
//     let hash89 = nonleaf_hash(Some(hash8), None);

//     let hash03 = nonleaf_hash(Some(hash01), Some(hash23));
//     let hash47 = nonleaf_hash(Some(hash45), Some(hash67));
//     let hash811 = nonleaf_hash(Some(hash89), None);

//     let hash07 = nonleaf_hash(Some(hash03), Some(hash47));
//     let hash815 = nonleaf_hash(Some(hash811), None);

//     let hash015 = nonleaf_hash(Some(hash07), Some(hash815));

//     let n = merkle_tree.load_node(&NodeId::new(0, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash0);
//     let n = merkle_tree.load_node(&NodeId::new(0, 1))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash1);
//     let n = merkle_tree.load_node(&NodeId::new(0, 2))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash2);
//     let n = merkle_tree.load_node(&NodeId::new(0, 3))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash3);
//     let n = merkle_tree.load_node(&NodeId::new(0, 4))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash4);
//     let n = merkle_tree.load_node(&NodeId::new(0, 5))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash5);
//     let n = merkle_tree.load_node(&NodeId::new(0, 6))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash6);
//     let n = merkle_tree.load_node(&NodeId::new(0, 7))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash7);
//     let n = merkle_tree.load_node(&NodeId::new(0, 8))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash8);

//     let n = merkle_tree.load_node(&NodeId::new(1, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash01);
//     let n = merkle_tree.load_node(&NodeId::new(1, 1))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash23);
//     let n = merkle_tree.load_node(&NodeId::new(1, 2))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash45);
//     let n = merkle_tree.load_node(&NodeId::new(1, 3))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash67);
//     let n = merkle_tree.load_node(&NodeId::new(1, 4))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash89);

//     let n = merkle_tree.load_node(&NodeId::new(2, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash03);
//     let n = merkle_tree.load_node(&NodeId::new(2, 1))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash47);
//     let n = merkle_tree.load_node(&NodeId::new(2, 2))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash811);

//     let n = merkle_tree.load_node(&NodeId::new(3, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash07);
//     let n = merkle_tree.load_node(&NodeId::new(3, 1))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash815);

//     let n = merkle_tree.load_node(&NodeId::new(4, 0))?;
//     assert_eq!(n.expect("Can't find the node").get_hash(), hash015);

//     Ok(())
// }

#[test]
fn test_read() -> Result<()> {
    init_tracing_subscriber("info")?;
    let mut merkle_tree = build_tree();
    let mut ctx = ReadContext::new(&merkle_tree, merkle_tree.root_id)?;
    let tree_height = merkle_tree.get_height().expect("empty tree");
    let v0 = ctx.query(PageId(0))?;
    assert_eq!(leaf_hash(&PageId(0), &"old_page0".to_digest()), v0);
    let v1 = ctx.query(PageId(1))?;
    assert_eq!(leaf_hash(&PageId(1), &"old_page1".to_digest()), v1);
    let v2 = ctx.query(PageId(2))?;
    assert_eq!(leaf_hash(&PageId(2), &"old_page2".to_digest()), v2);
    let v3 = ctx.query(PageId(3))?;
    assert_eq!(leaf_hash(&PageId(3), &"old_page3".to_digest()), v3);
    let v4 = ctx.query(PageId(4))?;
    assert_eq!(leaf_hash(&PageId(4), &"old_page4".to_digest()), v4);
    let v5 = ctx.query(PageId(5))?;
    assert_eq!(leaf_hash(&PageId(5), &"old_page5".to_digest()), v5);
    let v6 = ctx.query(PageId(6))?;
    assert_eq!(leaf_hash(&PageId(6), &"old_page6".to_digest()), v6);
    let v7 = ctx.query(PageId(7))?;
    assert_eq!(leaf_hash(&PageId(7), &"old_page7".to_digest()), v7);
    let v8 = ctx.query(PageId(8))?;
    assert_eq!(leaf_hash(&PageId(8), &"old_page8".to_digest()), v8);

    let p = ctx.into_proof();

    // step1: check the computed root hash of query proof
    assert_eq!(
        merkle_tree
            .load_node(&merkle_tree.root_id.unwrap())?
            .unwrap()
            .get_hash(),
        p.root_hash()?
    );
    // step2: check the value with returned result
    p.verify_val(v0, PageId(0), tree_height)?;
    p.verify_val(v1, PageId(1), tree_height)?;
    p.verify_val(v2, PageId(2), tree_height)?;
    p.verify_val(v3, PageId(3), tree_height)?;
    p.verify_val(v4, PageId(4), tree_height)?;
    p.verify_val(v5, PageId(5), tree_height)?;
    p.verify_val(v6, PageId(6), tree_height)?;
    p.verify_val(v7, PageId(7), tree_height)?;
    p.verify_val(v8, PageId(8), tree_height)?;

    // update
    let mut ctx = WriteContext::new(&merkle_tree, merkle_tree.root_id);
    ctx.update("new_page1".to_digest(), PageId(1)).unwrap();
    ctx.update("new_page3".to_digest(), PageId(3)).unwrap();
    ctx.update("new_page5".to_digest(), PageId(5)).unwrap();
    ctx.update("new_page7".to_digest(), PageId(7)).unwrap();
    let changes = ctx.changes();
    merkle_tree.apply(changes);

    // read
    let mut ctx = ReadContext::new(&merkle_tree, merkle_tree.root_id)?;
    let tree_height = merkle_tree.get_height().expect("empty tree");
    let v0 = ctx.query(PageId(0))?;
    assert_eq!(leaf_hash(&PageId(0), &"old_page0".to_digest()), v0);
    let v1 = ctx.query(PageId(1))?;
    assert_eq!(leaf_hash(&PageId(1), &"new_page1".to_digest()), v1);
    let v2 = ctx.query(PageId(2))?;
    assert_eq!(leaf_hash(&PageId(2), &"old_page2".to_digest()), v2);
    let v3 = ctx.query(PageId(3))?;
    assert_eq!(leaf_hash(&PageId(3), &"new_page3".to_digest()), v3);
    let v4 = ctx.query(PageId(4))?;
    assert_eq!(leaf_hash(&PageId(4), &"old_page4".to_digest()), v4);
    let v5 = ctx.query(PageId(5))?;
    assert_eq!(leaf_hash(&PageId(5), &"new_page5".to_digest()), v5);
    let v6 = ctx.query(PageId(6))?;
    assert_eq!(leaf_hash(&PageId(6), &"old_page6".to_digest()), v6);
    let v7 = ctx.query(PageId(7))?;
    assert_eq!(leaf_hash(&PageId(7), &"new_page7".to_digest()), v7);
    let v8 = ctx.query(PageId(8))?;
    assert_eq!(leaf_hash(&PageId(8), &"old_page8".to_digest()), v8);

    let p = ctx.into_proof();

    // step1: check the computed root hash of query proof
    assert_eq!(
        merkle_tree
            .load_node(&merkle_tree.root_id.unwrap())?
            .unwrap()
            .get_hash(),
        p.root_hash()?
    );
    // step2: check the value with returned result
    p.verify_val(v0, PageId(0), tree_height)?;
    p.verify_val(v1, PageId(1), tree_height)?;
    p.verify_val(v2, PageId(2), tree_height)?;
    p.verify_val(v3, PageId(3), tree_height)?;
    p.verify_val(v4, PageId(4), tree_height)?;
    p.verify_val(v5, PageId(5), tree_height)?;
    p.verify_val(v6, PageId(6), tree_height)?;
    p.verify_val(v7, PageId(7), tree_height)?;
    p.verify_val(v8, PageId(8), tree_height)?;

    Ok(())
}

#[test]
fn test_sha256() {
    let a = [1, 2, 3, 4, 5];
    let input_slice = &a[1..3];
    let hash = input_slice.to_digest();
    println!("{:?}", hash);
    assert_eq!(1, 1);
}
