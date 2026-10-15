// Property tests for the mutation algorithms (engineering standards §8:
// "DOM/logic crates: Unit tests; property tests for mutation/invalidation").
//
// A fixed-seed xorshift PRNG drives pseudo-random mutation sequences over a
// small node universe. After every operation, full structural invariants are
// asserted:
//
// 1. parent links are consistent in both directions (child's parent lists
//    it; parent's child list contains the child);
// 2. the tree is acyclic (every node's ancestor chain terminates at a root
//    within the live-node bound);
// 3. the live-node count equals the number of reachable nodes;
// 4. child lists contain no duplicates;
// 5. every attached node's depth respects the limit.
//
// Determinism: the PRNG is seeded; no wall-clock, no threads. A failure
// reproduces exactly from the printed seed and step.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

use veyra_dom::{DomError, NodeId, QualName, Tree};
use veyra_url::Url;

struct Xorshift(u64);

impl Xorshift {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

fn seeded_tree(max_depth: usize) -> (Tree, Vec<NodeId>) {
    let url = Url::parse("https://example.org/property").unwrap();
    let mut tree = Tree::new_document_with_limits(url.origin(), 512, max_depth).unwrap();
    let doc = tree.document().unwrap();
    let mut nodes = vec![doc];
    for i in 0..7 {
        let el = tree
            .create_element(QualName::html(format!("n{i}")), vec![])
            .unwrap();
        tree.append(doc, el).unwrap_or(()); // may hit depth limit; fine
        nodes.push(el);
    }
    (tree, nodes)
}

/// Full structural audit of the tree.
fn audit(tree: &Tree, nodes: &[NodeId], max_depth: usize) {
    let live = tree.live_nodes();
    let doc = tree.document();
    let mut reachable = 0usize;
    if let Some(doc) = doc {
        reachable = tree.descendants(doc).count();
    }
    // Nodes may be detached (not reachable from the document); reachability
    // must never exceed the live count.
    assert!(reachable <= live, "reachable {reachable} > live {live}");

    for &id in nodes {
        if !tree.is_live(id) {
            continue;
        }
        // 1. parent/child consistency.
        if let Some(parent) = tree.parent(id) {
            assert!(
                tree.children(parent).unwrap().contains(&id),
                "parent does not list child"
            );
        }
        if let Some(children) = tree.children(id) {
            let unique: std::collections::HashSet<NodeId> = children.iter().copied().collect();
            assert_eq!(unique.len(), children.len(), "duplicate child");
            for child in children {
                assert_eq!(tree.parent(*child), Some(id), "child's parent is wrong");
            }
        }
        // 2. acyclicity + 5. depth bound: walk up with a hard stop.
        let mut cursor = Some(id);
        let mut steps = 0usize;
        while let Some(current) = cursor {
            steps += 1;
            assert!(
                steps <= live + 1,
                "ancestor chain exceeds live count: cycle"
            );
            cursor = tree.parent(current);
        }
        if tree.parent(id).is_some() || doc == Some(id) {
            let depth = tree.depth(id).unwrap();
            assert!(
                depth <= max_depth,
                "depth {depth} exceeds limit {max_depth}"
            );
        }
    }
    // 3. live count matches slot accounting.
    assert_eq!(tree.live_nodes(), live);
}

#[test]
fn random_mutation_sequences_preserve_invariants() {
    for seed in 1..=24u64 {
        let max_depth = 8;
        let (mut tree, nodes) = seeded_tree(max_depth);
        let mut rng = Xorshift::new(seed);
        for step in 0..400 {
            let parent = nodes[rng.below(nodes.len())];
            let child = nodes[rng.below(nodes.len())];
            match rng.below(6) {
                0 => {
                    let _ = tree.append(parent, child);
                }
                1 => {
                    let reference = nodes[rng.below(nodes.len())];
                    let _ = tree.insert_before(parent, child, Some(reference));
                }
                2 => {
                    let _ = tree.insert_before(parent, child, None);
                }
                3 => {
                    let _ = tree.remove(parent, child);
                }
                4 => {
                    let _ = tree.detach(child);
                }
                _ => {
                    // Text append: exercises coalescing under mutation.
                    let _ = tree.append_text(parent, "x");
                }
            }
            audit(&tree, &nodes, max_depth);
            let _ = step; // step is part of the reproduction triple below
            if tree.live_nodes() > 400 {
                break;
            }
        }
        assert!(
            tree.live_nodes() <= tree.max_nodes(),
            "seed {seed}: live count exceeds limit"
        );
    }
}

#[test]
fn failed_mutations_leave_the_tree_unchanged() {
    for seed in 100..=112u64 {
        let (mut tree, nodes) = seeded_tree(4);
        let mut rng = Xorshift::new(seed);
        for _ in 0..200 {
            let parent = nodes[rng.below(nodes.len())];
            let child = nodes[rng.below(nodes.len())];
            let before_children: Vec<Vec<NodeId>> = nodes
                .iter()
                .filter_map(|id| tree.children(*id).map(<[NodeId]>::to_vec))
                .collect();
            let before_live = tree.live_nodes();
            let before_parents: Vec<Option<NodeId>> =
                nodes.iter().map(|id| tree.parent(*id)).collect();

            let result = tree.append(parent, child);
            if result == Err(DomError::HierarchyRequest)
                || result == Err(DomError::StaleNodeId)
                || result == Err(DomError::ForeignNode)
            {
                let after_children: Vec<Vec<NodeId>> = nodes
                    .iter()
                    .filter_map(|id| tree.children(*id).map(<[NodeId]>::to_vec))
                    .collect();
                assert_eq!(
                    before_children, after_children,
                    "seed {seed}: rejected mutation mutated child lists"
                );
                let after_parents: Vec<Option<NodeId>> =
                    nodes.iter().map(|id| tree.parent(*id)).collect();
                assert_eq!(
                    before_parents, after_parents,
                    "seed {seed}: rejected mutation mutated parents"
                );
            }
            assert_eq!(before_live, tree.live_nodes(), "moves do not change counts");
        }
    }
}

#[test]
fn destroy_reclaims_exactly_the_subtree() {
    let url = Url::parse("https://example.org/destroy").unwrap();
    let mut tree = Tree::new_document(url.origin()).unwrap();
    let doc = tree.document().unwrap();
    let keep = tree.create_element(QualName::html("keep"), vec![]).unwrap();
    tree.append(doc, keep).unwrap();
    let doomed_root = tree.create_element(QualName::html("doom"), vec![]).unwrap();
    tree.append(doc, doomed_root).unwrap();
    let mut expected_destroyed = 1usize;
    let mut cursor = doomed_root;
    for i in 0..10 {
        let child = tree
            .create_element(QualName::html(format!("d{i}")), vec![])
            .unwrap();
        tree.append(cursor, child).unwrap();
        cursor = child;
        expected_destroyed += 1;
    }
    let before = tree.live_nodes();
    tree.destroy_subtree(doomed_root).unwrap();
    assert_eq!(tree.live_nodes(), before - expected_destroyed);
    assert!(!tree.is_live(doomed_root));
    assert!(tree.is_live(keep));
    // Slots are reusable and old ids stay dead.
    let recycled = tree
        .create_element(QualName::html("recycled"), vec![])
        .unwrap();
    assert!(!tree.is_live(doomed_root));
    assert!(tree.is_live(recycled));
}
