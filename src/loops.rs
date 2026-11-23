//! Natural loops of a function body, and a layout of its blocks that keeps
//! each loop's blocks together (so that it can be emitted as a QuakeC loop).

use std::collections::{HashMap, HashSet};

use waffle::cfg::CFGInfo;
use waffle::{Block, FunctionBody};

use crate::infer::targets;

/// A natural loop: the blocks that can reach one of its back edges without
/// passing through its header.
pub struct Loop {
    pub blocks: HashSet<Block>,
    /// The sources of the back edges to the header.
    pub latches: Vec<Block>,
    /// The header of the innermost loop containing this one.
    pub parent: Option<Block>,
}

/// The loops of a function body, by header.
#[derive(Default)]
pub struct Loops {
    pub loops: HashMap<Block, Loop>,
    /// The header of the innermost loop containing each block in a loop.
    innermost: HashMap<Block, Block>,
}

/// The layout of the blocks emitted at one level: blocks, and loops (whose
/// items start with their header).
pub enum Item {
    Block(Block),
    Loop(Block, Vec<Item>),
}

impl Item {
    /// Where control goes on entering this item.
    pub fn entry(&self) -> Block {
        match self {
            Item::Block(b) | Item::Loop(b, _) => *b,
        }
    }
}

impl Loops {
    /// The loops of `body`, whose reachable blocks are `order` (in reverse
    /// postorder). None if the CFG is irreducible (which structured wasm
    /// control flow can't produce): then everything is left to `goto`s.
    pub fn new(body: &FunctionBody, order: &[Block]) -> Loops {
        let cfg = CFGInfo::new(body);
        let pos: HashMap<Block, usize> = order.iter().enumerate().map(|(i, &b)| (b, i)).collect();
        let mut latches: Vec<(Block, Vec<Block>)> = Vec::new();
        for &b in order {
            for t in targets(&body.blocks[b].terminator) {
                let h = t.block;
                if pos[&h] > pos[&b] {
                    continue;
                }
                // A retreating edge: in a reducible CFG, a back edge.
                if !cfg.dominates(h, b) {
                    return Loops::default();
                }
                match latches.iter_mut().find(|(x, _)| *x == h) {
                    Some((_, ls)) if ls.contains(&b) => {}
                    Some((_, ls)) => ls.push(b),
                    None => latches.push((h, vec![b])),
                }
            }
        }
        let mut loops = HashMap::new();
        for (h, ls) in latches {
            let mut blocks = HashSet::from([h]);
            let mut stack: Vec<Block> = ls.clone();
            while let Some(x) = stack.pop() {
                if blocks.insert(x) {
                    stack.extend(
                        cfg.preds[x]
                            .iter()
                            .copied()
                            .filter(|p| pos.contains_key(p)),
                    );
                }
            }
            loops.insert(
                h,
                Loop {
                    blocks,
                    latches: ls,
                    parent: None,
                },
            );
        }
        // The innermost loop containing a block is the smallest one.
        let mut innermost: HashMap<Block, Block> = HashMap::new();
        for (&h, l) in &loops {
            for &b in &l.blocks {
                let smaller = innermost
                    .get(&b)
                    .is_none_or(|&i| loops[&i].blocks.len() > l.blocks.len());
                if smaller {
                    innermost.insert(b, h);
                }
            }
        }
        let parents: Vec<(Block, Option<Block>)> = loops
            .iter()
            .map(|(&h, l)| {
                let parent = loops
                    .iter()
                    .filter(|&(&o, ol)| o != h && ol.blocks.contains(&h))
                    .min_by_key(|(_, ol)| ol.blocks.len())
                    .map(|(&o, _)| o);
                debug_assert!(parent.is_none_or(|p| loops[&p].blocks.len() > l.blocks.len()));
                (h, parent)
            })
            .collect();
        for (h, p) in parents {
            loops.get_mut(&h).unwrap().parent = p;
        }
        Loops { loops, innermost }
    }

    /// The header of the innermost loop containing `b`.
    pub fn innermost(&self, b: Block) -> Option<Block> {
        self.innermost.get(&b).copied()
    }

    /// The layout of `order`'s blocks, except those in `nested` (which are
    /// emitted inside their predecessor): in reverse postorder, but with
    /// each loop's blocks together, where its header is. In a reducible CFG,
    /// every edge into a loop then comes from an earlier item (to its
    /// header), and every edge out of it goes to a later one.
    pub fn layout(&self, order: &[Block], nested: &HashSet<Block>) -> Vec<Item> {
        self.region(order, None, nested)
    }

    fn region(&self, blocks: &[Block], current: Option<Block>, nested: &HashSet<Block>) -> Vec<Item> {
        let mut items = Vec::new();
        for &b in blocks {
            if self.innermost(b) == current {
                if !nested.contains(&b) {
                    items.push(Item::Block(b));
                }
            } else if let Some(l) = self.loops.get(&b)
                && l.parent == current
            {
                let inner: Vec<Block> = blocks
                    .iter()
                    .copied()
                    .filter(|x| l.blocks.contains(x))
                    .collect();
                items.push(Item::Loop(b, self.region(&inner, Some(b), nested)));
            }
        }
        items
    }
}
