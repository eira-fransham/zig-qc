//! Sharing QuakeC locals between values whose lifetimes don't overlap.
//!
//! The emitter gives each SSA value that needs one its own local, and
//! records where each local is defined and read, block by block (in the
//! SSA form's blocks, in reverse postorder). This computes liveness over
//! those blocks, builds the interference graph and colours it: locals of the
//! same type that never hold a needed value at the same time get the same
//! colour, so they can be one QuakeC local.
//!
//! Each local is assigned once, except the locals of block parameters, which
//! are assigned on every edge into their block (in the predecessor's
//! terminator). Those edge copies are a parallel copy that the emitter
//! sequentializes using the original names, so a destination must not share
//! a local with anything another copy on the same edge reads.

use crate::qc::Ty;

/// A local to colour.
pub struct Var {
    pub ty: Ty,
    /// A parameter: it keeps its own local, which others may share.
    pub param: bool,
}

/// A definition or use of a local, in program order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Use(usize),
    Def(usize),
}

/// One edge's parallel copy: the destination, the local the source is a
/// plain copy of (if it is one), and every local the source reads.
pub struct Copy {
    pub dest: usize,
    pub plain: Option<usize>,
    pub reads: Vec<usize>,
}

/// What a block does with locals.
#[derive(Default)]
pub struct Block {
    /// Events before the terminator.
    pub events: Vec<Event>,
    /// Locals the terminator reads (its condition and edge copy sources).
    pub term_uses: Vec<usize>,
    /// Locals the terminator assigns (block parameters of successors).
    pub term_defs: Vec<usize>,
    /// The parallel copy of each outgoing edge.
    pub edges: Vec<Vec<Copy>>,
    /// Successor blocks (indices into the block list).
    pub succs: Vec<usize>,
}

#[derive(Clone)]
struct BitSet(Vec<u64>);

impl BitSet {
    fn new(n: usize) -> Self {
        Self(vec![0; n.div_ceil(64)])
    }
    fn insert(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }
    fn remove(&mut self, i: usize) {
        self.0[i / 64] &= !(1 << (i % 64));
    }
    /// `self |= other`, returning whether anything changed.
    fn union(&mut self, other: &BitSet) -> bool {
        let mut changed = false;
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            let n = *a | b;
            changed |= n != *a;
            *a = n;
        }
        changed
    }
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(w, &bits)| {
            (0..64).filter(move |b| bits & (1 << b) != 0).map(move |b| w * 64 + b)
        })
    }
}

/// Run `f` on each event of `b` backwards, with the locals live just after
/// it. Returns the locals live on entry to `b`.
fn scan(b: &Block, live_out: &BitSet, mut f: impl FnMut(Event, &BitSet)) -> BitSet {
    let mut live = live_out.clone();
    // The terminator reads everything before assigning anything.
    for &d in &b.term_defs {
        f(Event::Def(d), &live);
    }
    for &d in &b.term_defs {
        live.remove(d);
    }
    for &u in &b.term_uses {
        live.insert(u);
    }
    for &e in b.events.iter().rev() {
        f(e, &live);
        match e {
            Event::Def(d) => live.remove(d),
            Event::Use(u) => live.insert(u),
        }
    }
    live
}

/// The colour of each local. Parameter `i` (the `i`th `param` in `vars`)
/// gets colour `i`; other colours follow, each holding one type.
pub fn colour(vars: &[Var], blocks: &[Block]) -> Vec<usize> {
    let n = vars.len();

    // Liveness.
    let mut live_in = vec![BitSet::new(n); blocks.len()];
    let mut live_out = vec![BitSet::new(n); blocks.len()];
    loop {
        let mut changed = false;
        for i in (0..blocks.len()).rev() {
            let mut out = BitSet::new(n);
            for &s in &blocks[i].succs {
                out.union(&live_in[s]);
            }
            live_out[i].union(&out);
            let inn = scan(&blocks[i], &live_out[i], |_, _| {});
            changed |= live_in[i].union(&inn);
        }
        if !changed {
            break;
        }
    }

    // Interference.
    let mut adj = vec![BitSet::new(n); n];
    let mut add = |a: usize, b: usize| {
        if a != b {
            adj[a].insert(b);
            adj[b].insert(a);
        }
    };
    let mut hints: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, b) in blocks.iter().enumerate() {
        // A copy `d = x` doesn't make `d` interfere with `x`: they hold the
        // same value. (Only if every edge assigning `d` copies `x`.)
        let plain = |d: usize, x: usize| {
            let mut copies = b.edges.iter().flatten().filter(|c| c.dest == d).peekable();
            copies.peek().is_some() && copies.all(|c| c.plain == Some(x))
        };
        scan(b, &live_out[i], |e, live| {
            if let Event::Def(d) = e {
                for x in live.iter() {
                    if !plain(d, x) {
                        add(d, x);
                    }
                }
            }
        });
        // Parameters on entry are defined together.
        if i == 0 {
            let params: Vec<usize> = (0..n).filter(|&v| vars[v].param).collect();
            for &p in &params {
                for x in live_in[0].iter() {
                    add(p, x);
                }
            }
        }
        for edge in &b.edges {
            for (j, c) in edge.iter().enumerate() {
                for (k, other) in edge.iter().enumerate() {
                    if j != k {
                        for &r in &other.reads {
                            add(c.dest, r);
                        }
                        add(c.dest, other.dest);
                    }
                }
                if let Some(x) = c.plain {
                    hints[c.dest].push(x);
                    hints[x].push(c.dest);
                }
            }
        }
    }

    // Colour parameters first, then everything else in order of first
    // definition (blocks are in reverse postorder, so definitions come
    // before the blocks they dominate).
    let mut order: Vec<usize> = Vec::new();
    let mut seen = vec![false; n];
    let mut visit = |v: usize, order: &mut Vec<usize>| {
        if !seen[v] {
            seen[v] = true;
            order.push(v);
        }
    };
    for (v, var) in vars.iter().enumerate() {
        if var.param {
            visit(v, &mut order);
        }
    }
    for b in blocks {
        for &e in &b.events {
            if let Event::Def(d) = e {
                visit(d, &mut order);
            }
        }
        for &d in &b.term_defs {
            visit(d, &mut order);
        }
    }
    for v in 0..n {
        visit(v, &mut order);
    }

    const NONE: usize = usize::MAX;
    let mut colour = vec![NONE; n];
    let mut colour_ty: Vec<Ty> = Vec::new();
    for v in order {
        let free = |c: usize, colour: &[usize]| {
            colour_ty[c] == vars[v].ty && !adj[v].iter().any(|x| colour[x] == c)
        };
        if vars[v].param {
            colour[v] = colour_ty.len();
            colour_ty.push(vars[v].ty);
            continue;
        }
        let hinted = hints[v]
            .iter()
            .map(|&h| colour[h])
            .find(|&c| c != NONE && free(c, &colour));
        let c = hinted
            .or_else(|| (0..colour_ty.len()).find(|&c| free(c, &colour)))
            .unwrap_or_else(|| {
                colour_ty.push(vars[v].ty);
                colour_ty.len() - 1
            });
        colour[v] = c;
    }
    colour
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qc::QcType;

    const F: Ty = Ty::Known(QcType::Float);
    const E: Ty = Ty::Known(QcType::Entity);

    fn var(ty: Ty) -> Var {
        Var { ty, param: false }
    }

    use Event::{Def, Use};

    #[test]
    fn disjoint_share_overlapping_dont() {
        // a = ..; use a; b = ..; use b; c = ..; d = ..; use c; use d
        let vars = [var(F), var(F), var(F), var(F)];
        let b = Block {
            events: vec![Def(0), Use(0), Def(1), Use(1), Def(2), Def(3), Use(2), Use(3)],
            ..Default::default()
        };
        let c = colour(&vars, &[b]);
        assert_eq!(c[0], c[1]);
        assert_eq!(c[0], c[2]);
        assert_ne!(c[2], c[3]);
    }

    #[test]
    fn types_never_share() {
        let vars = [var(F), var(E)];
        let b = Block {
            events: vec![Def(0), Use(0), Def(1), Use(1)],
            ..Default::default()
        };
        let c = colour(&vars, &[b]);
        assert_ne!(c[0], c[1]);
    }

    #[test]
    fn live_around_loop() {
        // b0: x = ..; -> b1. b1: y = ..; use y; -> b1, b2. b2: use x.
        let vars = [var(F), var(F)];
        let blocks = [
            Block {
                events: vec![Def(0)],
                succs: vec![1],
                ..Default::default()
            },
            Block {
                events: vec![Def(1), Use(1)],
                succs: vec![1, 2],
                ..Default::default()
            },
            Block {
                events: vec![Use(0)],
                ..Default::default()
            },
        ];
        let c = colour(&vars, &blocks);
        assert_ne!(c[0], c[1]);
    }

    #[test]
    fn parameters_are_reused_after_their_last_use() {
        let vars = [
            Var { ty: F, param: true },
            var(F),
        ];
        let b = Block {
            events: vec![Use(0), Def(1), Use(1)],
            ..Default::default()
        };
        let c = colour(&vars, &[b]);
        assert_eq!(c, vec![0, 0]);
    }

    #[test]
    fn plain_copies_are_coalesced() {
        // b0: x = ..; p = x (edge) -> b1. b1: use p.
        let vars = [var(F), var(F)];
        let blocks = [
            Block {
                events: vec![Def(0)],
                term_uses: vec![0],
                term_defs: vec![1],
                edges: vec![vec![Copy {
                    dest: 1,
                    plain: Some(0),
                    reads: vec![0],
                }]],
                succs: vec![1],
            },
            Block {
                events: vec![Use(1)],
                ..Default::default()
            },
        ];
        let c = colour(&vars, &blocks);
        assert_eq!(c[0], c[1]);
    }

    #[test]
    fn swap_keeps_copies_apart() {
        // A loop swapping two parameters: (p, q) = (q + 1, p). Each source
        // is read by the other copy, so no destination may share with it.
        let vars = [var(F), var(F), var(F)];
        let blocks = [
            Block {
                events: vec![Use(0), Use(1), Def(2), Use(2)],
                term_uses: vec![2, 0],
                term_defs: vec![0, 1],
                edges: vec![vec![
                    Copy {
                        dest: 0,
                        plain: None,
                        reads: vec![2],
                    },
                    Copy {
                        dest: 1,
                        plain: Some(0),
                        reads: vec![0],
                    },
                ]],
                succs: vec![0],
            },
        ];
        let c = colour(&vars, &blocks);
        assert_ne!(c[0], c[1]);
        assert_ne!(c[1], c[2]);
    }
}
