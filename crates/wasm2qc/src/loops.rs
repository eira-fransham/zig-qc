//! Natural loops of a function body, and a layout of its blocks: each
//! block with several predecessors follows its immediate dominator (as in
//! Ramsey's "Beyond Relooper"), so that branches to it from the ends of its
//! dominator's branches fall through, and each loop's blocks are together
//! (so that it can be emitted as a QuakeC loop).

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
    /// The immediate dominator of each reachable block but the entry.
    idom: HashMap<Block, Block>,
}

/// Where a block is emitted: after a block's code (and the blocks after it),
/// or after a loop statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Anchor {
    Block(Block),
    Loop(Block),
}

/// Which blocks are emitted after each anchor, in order (see
/// [`Loops::layout`]).
#[derive(Default)]
pub struct Layout {
    after: HashMap<Anchor, Vec<Block>>,
}

impl Layout {
    pub fn after(&self, a: Anchor) -> &[Block] {
        self.after.get(&a).map_or(&[], Vec::as_slice)
    }
}

impl Loops {
    /// The loops of `body`, whose reachable blocks are `order` (in reverse
    /// postorder). None if the CFG is irreducible (which structured wasm
    /// control flow can't produce): then everything is left to `goto`s.
    pub fn new(body: &FunctionBody, order: &[Block]) -> Loops {
        let cfg = CFGInfo::new(body);
        let pos: HashMap<Block, usize> = order.iter().enumerate().map(|(i, &b)| (b, i)).collect();
        let idom: HashMap<Block, Block> = order
            .iter()
            .filter(|&&b| b != body.entry)
            .map(|&b| (b, cfg.domtree[b]))
            .collect();
        let mut latches: Vec<(Block, Vec<Block>)> = Vec::new();
        for &b in order {
            for t in targets(&body.blocks[b].terminator) {
                let h = t.block;
                if pos[&h] > pos[&b] {
                    continue;
                }
                // A retreating edge: in a reducible CFG, a back edge.
                if !cfg.dominates(h, b) {
                    return Loops {
                        idom,
                        ..Loops::default()
                    };
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
                    stack.extend(cfg.preds[x].iter().copied().filter(|p| pos.contains_key(p)));
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
        Loops {
            loops,
            innermost,
            idom,
        }
    }

    /// The header of the innermost loop containing `b`.
    pub fn innermost(&self, b: Block) -> Option<Block> {
        self.innermost.get(&b).copied()
    }

    /// The layout of `order`'s blocks (in reverse postorder), except the
    /// entry and those in `nested` (which are emitted inside their only
    /// predecessor). Each follows its immediate dominator's code, unless
    /// that's in a loop the block isn't in: then it follows the outermost
    /// such loop's statement. Every path to a block passes through its
    /// dominator, so this is where its predecessors are; a loop's blocks
    /// are all dominated by its header, so they're inside the loop.
    pub fn layout(&self, entry: Block, order: &[Block], nested: &HashSet<Block>) -> Layout {
        let mut after: HashMap<Anchor, Vec<Block>> = HashMap::new();
        for &m in order {
            if m == entry || nested.contains(&m) {
                continue;
            }
            let mut a = Anchor::Block(self.idom[&m]);
            loop {
                let l = match a {
                    Anchor::Block(b) => self.innermost(b),
                    Anchor::Loop(h) => self.loops[&h].parent,
                };
                match l {
                    Some(h) if !self.loops[&h].blocks.contains(&m) => a = Anchor::Loop(h),
                    _ => break,
                }
            }
            after.entry(a).or_default().push(m);
        }
        Layout { after }
    }
}
