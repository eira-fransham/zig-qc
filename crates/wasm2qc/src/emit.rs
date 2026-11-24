//! Emission of one function instance from SSA form to QuakeC.
//!
//! A block with several predecessors follows its immediate dominator's
//! code, so that branches to it from the ends of the dominator's branches
//! fall through, and each natural loop's blocks are together (see
//! [`crate::loops`]); each starts with a label (kept only if something
//! jumps to it). A block with a single incoming edge (see
//! [`nested_blocks`]) is instead emitted inside its predecessor's
//! terminator: in a branch of its `if` or an arm of its `switch`, or straight
//! after an unconditional branch. Loops become `while (c)`, `do ... while
//! (c)` or `while (1)` (see [`Emitter::loop_item`]), and branches to the
//! innermost loop's header or exit `continue` or `break`. Branches that
//! can't be expressed that way are `goto`s. Block parameters are QuakeC locals,
//! assigned on each incoming edge. Values with a single use in the same
//! block are inlined into that use, unless a side effect in between could
//! change what they read; other values are assigned to a local. Calls are
//! always assigned, since QuakeC return values live in `OFS_RETURN` and are
//! clobbered by later calls.

use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use waffle::entity::EntityRef;
use waffle::{Block, Func, FunctionBody, Operator, Terminator, Type, Value, ValueDef};

use crate::alloc;
use crate::data::{DataKind, StaticData};
use crate::infer::{
    self, Infer, Instance, Kind, Site, TERM_COND, args, const_val, targets, term_arg, term_ret,
};
use crate::loops::{Anchor, Layout, Loop, Loops};
use crate::qc::{BinOp, Expr, Function, QcType, Sig, Stmt, Ty, UnOp};
use crate::translate::{Ctx, FuncKind, Intrinsic};

/// Largest integer magnitude exactly representable as an f32.
const MAX_EXACT_INT: i64 = 1 << 24;

struct Emitter<'a, 'm> {
    ctx: &'a mut Ctx<'m>,
    inf: &'a mut Infer,
    instances: &'a [Instance],
    inst: &'a Instance,
    sigs: &'a HashMap<String, Sig>,
    body: &'m FunctionBody,
    out: Vec<Stmt>,
    /// Locals (and their types), other than parameters.
    locals: Vec<(String, Ty)>,
    /// The local holding each value assigned to one.
    names: HashMap<Value, String>,
    /// Values computed but not yet emitted (see the module docs).
    pending: HashMap<Value, Expr>,
    /// Number of uses of each value, and the block of its last use.
    uses: HashMap<Value, (usize, Block)>,
    block: Block,
    next_temp: usize,
    gotos: HashSet<String>,
    /// Where each block defines and reads locals (see [`crate::alloc`]).
    rec: HashMap<Block, Rec>,
    /// Whether the terminator is being emitted.
    in_term: bool,
    /// Variable shifts that are only used by a bit test (see `bit_test`),
    /// emitted as part of it.
    bit_tests: HashSet<Value>,
    /// Block parameters known to be non-negative (see [`nonneg`]).
    nonneg: HashSet<Value>,
    /// Blocks emitted inside their only predecessor (see [`nested_blocks`]).
    nested: HashSet<Block>,
    /// The blocks each value is used in.
    use_blocks: HashMap<Value, HashSet<Block>>,
    loops: std::rc::Rc<Loops>,
    layout: std::rc::Rc<Layout>,
    /// The loops being emitted, innermost last.
    loop_ctx: Vec<LoopCtx>,
    /// The number of `switch` arms being emitted (which capture `break`).
    switch_depth: usize,
}

/// A loop being emitted.
struct LoopCtx {
    header: Block,
    /// Where `break` goes: the block after the loop.
    follow: Option<Block>,
    /// A `do ... while` loop, whose `continue` (in fteqcc) jumps to the top
    /// of its body, skipping the condition. Never used.
    do_while: bool,
    /// `switch_depth` outside the loop: a `break` inside a `switch` only
    /// leaves the `switch`.
    switch_depth: usize,
    /// For a `do ... while`, its latch, whose branch is the loop's test.
    latch: Option<Block>,
    /// The `do ... while`'s condition and exit copies, once emitted.
    test: Option<(Expr, Vec<Stmt>)>,
}

/// An edge copy: destination, plain source local, locals read.
type EdgeCopy = (String, Option<String>, Vec<String>);

/// What a block does with locals, by name (see [`alloc::Block`]).
#[derive(Default)]
struct Rec {
    /// `(is a definition, local)`, in program order.
    events: Vec<(bool, String)>,
    term_uses: Vec<String>,
    term_defs: Vec<String>,
    /// Each edge's copies.
    edges: Vec<Vec<EdgeCopy>>,
}

/// Emit instance `i` of `instances` as a QuakeC function.
pub fn emit(
    ctx: &mut Ctx<'_>,
    inf: &mut Infer,
    instances: &[Instance],
    i: usize,
    sigs: &HashMap<String, Sig>,
) -> Result<((Func, usize), Function)> {
    let inst = &instances[i];
    let module = ctx.module;
    let body = module.funcs[inst.func].body().expect("local function");
    let mut e = Emitter {
        ctx,
        inf,
        instances,
        inst,
        sigs,
        body,
        out: Vec::new(),
        locals: Vec::new(),
        names: HashMap::new(),
        pending: HashMap::new(),
        uses: HashMap::new(),
        block: body.entry,
        next_temp: 0,
        gotos: HashSet::new(),
        rec: HashMap::new(),
        in_term: false,
        bit_tests: HashSet::new(),
        nonneg: HashSet::new(),
        nested: HashSet::new(),
        use_blocks: HashMap::new(),
        loops: Default::default(),
        layout: Default::default(),
        loop_ctx: Vec::new(),
        switch_depth: 0,
    };
    let f = e.function()?;
    Ok(((inst.func, i), f))
}

impl Emitter<'_, '_> {
    fn function(&mut self) -> Result<Function> {
        let body = self.body;
        self.check_entities()?;
        self.count_uses();
        let order = rpo(body);
        self.nested = nested_blocks(body, &order);

        let sig = &self.sigs[&self.inst.name];
        let mut params = Vec::new();
        for (i, &(_, v)) in body.blocks[body.entry].params.iter().enumerate() {
            let n = format!("p{i}");
            self.names.insert(v, n.clone());
            params.push((n, sig.params[i]));
        }
        // Block parameters are locals.
        for &b in &order {
            if b == body.entry {
                continue;
            }
            for &(_, v) in &body.blocks[b].params {
                if self.uses.contains_key(&v) {
                    self.local_for(v)?;
                }
            }
        }

        self.loops = std::rc::Rc::new(Loops::new(body, &order));
        self.layout = std::rc::Rc::new(self.loops.layout(body.entry, &order, &self.nested));
        self.unit(body.entry, None)?;

        // Only keep labels that are jumped to.
        let gotos = std::mem::take(&mut self.gotos);
        let mut body_stmts = std::mem::take(&mut self.out);
        drop_labels(&mut body_stmts, &gotos);
        if matches!(body_stmts.last(), Some(Stmt::Return(None))) {
            body_stmts.pop();
        }
        let locals = self.share_locals(&order, &params, &mut body_stmts);
        Ok(Function {
            name: self.inst.name.clone(),
            params,
            ret: sig.ret,
            locals,
            body: body_stmts,
        })
    }

    /// Emit `blocks` (laid out after something, see [`Loops::layout`]) in
    /// order. `next` is where control goes after the last.
    fn chain(&mut self, blocks: &[Block], next: Option<Block>) -> Result<()> {
        for (i, &b) in blocks.iter().enumerate() {
            self.unit(b, blocks.get(i + 1).copied().or(next))?;
        }
        Ok(())
    }

    /// Emit block `b` where it's laid out: as a loop, if it's a loop's
    /// header, else after a label.
    fn unit(&mut self, b: Block, next: Option<Block>) -> Result<()> {
        if self.loops.loops.contains_key(&b) {
            return self.loop_item(b, next);
        }
        self.out.push(Stmt::Label(label(b)));
        self.block(b, next)
    }

    /// Emit block `b` (its instructions, then its terminator, which emits
    /// the blocks nested in it), then the blocks laid out after it. `next`
    /// is where control goes if it falls off the end.
    fn block(&mut self, b: Block, next: Option<Block>) -> Result<()> {
        let layout = self.layout.clone();
        let after = layout.after(Anchor::Block(b));
        self.insts(b)?;
        if self.loop_ctx.last().is_some_and(|c| c.latch == Some(b)) {
            debug_assert!(after.is_empty());
            return self.latch_test(b);
        }
        self.finish(b, after.first().copied().or(next))?;
        self.chain(after, next)
    }

    /// Emit the terminator of a `do ... while`'s latch `b`: the back edge's
    /// copies, then (in the loop context) the loop's condition and the exit
    /// edge's copies.
    fn latch_test(&mut self, b: Block) -> Result<()> {
        let Terminator::CondBr {
            cond,
            if_true,
            if_false,
        } = &self.body.blocks[b].terminator
        else {
            unreachable!("checked by `do_while_latch`")
        };
        let back = self.loop_ctx.last().map(|c| c.header) == Some(if_true.block);
        self.in_term = true;
        let c = self.operand(Site::Term(b), TERM_COND, *cond)?;
        let t = self.edge(b, 0, if_true)?;
        let f = self.edge(b, 1, if_false)?;
        self.record(&Stmt::If(c.clone(), t.clone(), f.clone()));
        // The back edge's copies don't affect the condition or what the
        // loop's exit reads (see `do_while_latch`).
        let (copies, exit_copies, c) = if back { (t, f, c) } else { (f, t, negate(c)) };
        self.out.extend(copies);
        self.check_pending(b)?;
        self.loop_ctx.last_mut().unwrap().test = Some((c, exit_copies));
        Ok(())
    }

    /// Emit the instructions of block `b`.
    fn insts(&mut self, b: Block) -> Result<()> {
        let body = self.body;
        self.block = b;
        self.in_term = false;
        for &v in &body.blocks[b].insts {
            if const_val(body, v).is_some() || const_expr(body, v).is_some() {
                continue;
            }
            self.inst(v)
                .with_context(|| format!("at {v} = {} (in {b})", infer::describe(body, v)))?;
        }
        Ok(())
    }

    /// Emit the terminator of block `b`, whose instructions were just
    /// emitted.
    fn finish(&mut self, b: Block, next: Option<Block>) -> Result<()> {
        self.in_term = true;
        self.terminator(b, next)
            .with_context(|| format!("at the end of {b}"))?;
        self.check_pending(b)
    }

    fn check_pending(&self, b: Block) -> Result<()> {
        ensure!(
            self.pending.is_empty(),
            "values left unused at the end of {b}"
        );
        Ok(())
    }

    /// Emit the loop with header `h` and blocks laid out as `items` (the
    /// first being `h`), which is followed by `next`. In order of
    /// preference:
    ///
    /// - `do { ... } while (c);` if its only back edge is a conditional
    ///   branch at the end of its last block, which leaves the loop
    ///   otherwise (see [`Self::do_while_latch`]);
    /// - `while (c) { ... }` if its header only tests a condition, leaving
    ///   the loop if it's false;
    /// - `while (1) { ... }`.
    ///
    /// A `break` goes to the loop's follow: the exit of the condition, or
    /// for `while (1)`, its only exit or the exit to `next`. If that's
    /// nested (the loop is its only predecessor), it's emitted after the
    /// loop.
    fn loop_item(&mut self, h: Block, next: Option<Block>) -> Result<()> {
        let body = self.body;
        let loops = self.loops.clone();
        let layout = self.layout.clone();
        let l = &loops.loops[&h];
        // The loop's body is its header's code and what follows it; then
        // come the blocks laid out after the loop.
        let after_h = layout.after(Anchor::Block(h));
        let post = layout.after(Anchor::Loop(h));
        let after_header = after_h.first().copied().unwrap_or(h);
        let loop_next = post.first().copied().or(next);

        let (follow, stmt, exit_copies) = if let Some((latch, exit)) = self.do_while_latch(h, l) {
            self.loop_ctx.push(LoopCtx {
                header: h,
                follow: Some(exit),
                do_while: true,
                switch_depth: self.switch_depth,
                latch: Some(latch),
                test: None,
            });
            let saved = std::mem::take(&mut self.out);
            let r = self.block(h, Some(h));
            let stmts = std::mem::replace(&mut self.out, saved);
            let ctx = self.loop_ctx.pop().unwrap();
            r?;
            let (c, exit_copies) = ctx.test.expect("the latch is emitted");
            (Some(exit), Stmt::DoWhile(stmts, c), exit_copies)
        } else {
            let saved = std::mem::take(&mut self.out);
            let r = self.insts(h);
            let pre = std::mem::replace(&mut self.out, saved);
            r?;
            let test = match &body.blocks[h].terminator {
                Terminator::CondBr {
                    cond,
                    if_true,
                    if_false,
                } if pre.is_empty() => {
                    let inside = |b: Block| l.blocks.contains(&b);
                    // The exit's copies follow the loop, so no `break` may
                    // go there.
                    let exit_ok =
                        |t: &waffle::BlockTarget| !self.has_copies(t) || self.only_exit(l, t.block);
                    match (inside(if_true.block), inside(if_false.block)) {
                        (true, false) if exit_ok(if_false) => {
                            Some((*cond, if_true, if_false, false))
                        }
                        (false, true) if exit_ok(if_true) => Some((*cond, if_true, if_false, true)),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some((cond, if_true, if_false, negated)) = test {
                // `while (c)`: `negated` if the loop continues when `cond`
                // is false.
                let (stay, exit) = if negated {
                    (if_false, if_true)
                } else {
                    (if_true, if_false)
                };
                self.in_term = true;
                let c = self.operand(Site::Term(h), TERM_COND, cond)?;
                let t = self.edge(h, 0, if_true)?;
                let f = self.edge(h, 1, if_false)?;
                self.record(&Stmt::If(c.clone(), t.clone(), f.clone()));
                let (copies, exit_copies, c) = if negated {
                    (f, t, negate(c))
                } else {
                    (t, f, c)
                };
                self.check_pending(h)?;
                self.loop_ctx.push(LoopCtx {
                    header: h,
                    follow: Some(exit.block),
                    do_while: false,
                    switch_depth: self.switch_depth,
                    latch: None,
                    test: None,
                });
                let saved = std::mem::replace(&mut self.out, copies);
                let r = (|| -> Result<()> {
                    let tail = self.tail(stay.block, Some(after_header))?;
                    self.out.extend(tail);
                    self.chain(after_h, Some(h))
                })();
                let stmts = std::mem::replace(&mut self.out, saved);
                self.loop_ctx.pop();
                r?;
                self.out.push(Stmt::Label(label(h)));
                (Some(exit.block), Stmt::While(c, stmts), exit_copies)
            } else {
                // `while (1)`.
                let mut exits: Vec<Block> = Vec::new();
                for &b in &l.blocks {
                    for t in targets(&body.blocks[b].terminator) {
                        if !l.blocks.contains(&t.block) && !exits.contains(&t.block) {
                            exits.push(t.block);
                        }
                    }
                }
                // The only exit, else the exit to what follows the loop,
                // else the only nested exit (which then follows the loop)
                // that doesn't follow an enclosing loop already.
                let nested_exits: Vec<Block> = exits
                    .iter()
                    .copied()
                    .filter(|x| {
                        self.nested.contains(x)
                            && !self.loop_ctx.iter().any(|c| c.follow == Some(*x))
                    })
                    .collect();
                let follow = match (exits.as_slice(), nested_exits.as_slice()) {
                    (&[x], _) => Some(x),
                    _ if loop_next.is_some_and(|n| exits.contains(&n)) => loop_next,
                    (_, &[x]) => Some(x),
                    _ => None,
                };
                self.loop_ctx.push(LoopCtx {
                    header: h,
                    follow,
                    do_while: false,
                    switch_depth: self.switch_depth,
                    latch: None,
                    test: None,
                });
                let saved = std::mem::replace(&mut self.out, vec![Stmt::Label(label(h))]);
                self.out.extend(pre);
                let r = (|| -> Result<()> {
                    self.finish(h, Some(after_header))?;
                    self.chain(after_h, Some(h))
                })();
                let stmts = std::mem::replace(&mut self.out, saved);
                self.loop_ctx.pop();
                r?;
                (follow, Stmt::While(Expr::Float(1.0), stmts), vec![])
            }
        };
        self.out.push(stmt);
        self.out.extend(exit_copies);
        match follow {
            Some(f) if self.nested.contains(&f) => {
                self.out.push(Stmt::Label(label(f)));
                self.block(f, loop_next)?;
            }
            Some(f) => {
                let tail = self.tail(f, loop_next)?;
                self.out.extend(tail);
            }
            None => {}
        }
        self.chain(post, next)
    }

    /// The block emitted last at the top level of the body of the loop with
    /// header `h` (following what's laid out after each block), unless
    /// that's a loop.
    fn last_in_body(&self, h: Block) -> Option<Block> {
        let mut b = h;
        loop {
            let after = if b != h && self.loops.loops.contains_key(&b) {
                match self.layout.after(Anchor::Loop(b)) {
                    [] => return None,
                    after => after,
                }
            } else {
                self.layout.after(Anchor::Block(b))
            };
            match after.last() {
                Some(&m) => b = m,
                None => return Some(b),
            }
        }
    }

    /// If the loop `l` with header `h` can be a `do ... while`: its latch,
    /// and where the loop's exit goes. The loop must have a single back
    /// edge, from a conditional branch that otherwise leaves the loop, at
    /// the end of the block emitted last in its body. If the
    /// exit has copies, they follow the loop, so it must be the loop's only
    /// edge to its target. The back edge's copies are made before the
    /// condition is tested, so they mustn't affect it or the exit's copies,
    /// and the header's parameters (which they assign) mustn't be used
    /// outside the loop.
    fn do_while_latch(&self, h: Block, l: &Loop) -> Option<(Block, Block)> {
        let body = self.body;
        let &[latch] = l.latches.as_slice() else {
            return None;
        };
        if self.last_in_body(h) != Some(latch) {
            return None;
        }
        let Terminator::CondBr {
            cond,
            if_true,
            if_false,
        } = &body.blocks[latch].terminator
        else {
            return None;
        };
        let (back, exit) = if if_true.block == h && !l.blocks.contains(&if_false.block) {
            (if_true, if_false)
        } else if if_false.block == h && !l.blocks.contains(&if_true.block) {
            (if_false, if_true)
        } else {
            return None;
        };
        if self.has_copies(exit) && !self.only_exit(l, exit.block) {
            return None;
        }
        let dests: HashSet<Value> = back
            .args
            .iter()
            .zip(&body.blocks[h].params)
            .filter(|&(&a, &(_, p))| self.names.contains_key(&p) && body.resolve_alias(a) != p)
            .map(|(_, &(_, p))| p)
            .collect();
        if dests.is_empty() {
            return Some((latch, exit.block));
        }
        let outside = dests.iter().any(|p| {
            self.use_blocks
                .get(p)
                .is_some_and(|bs| bs.iter().any(|b| !l.blocks.contains(b)))
        });
        // What the condition and the exit's copies may read: them, and (if
        // they're inlined into them) the values computed in the latch that
        // they depend on.
        let in_latch: HashSet<Value> = body.blocks[latch].insts.iter().copied().collect();
        let mut seen = HashSet::new();
        let mut stack = vec![*cond];
        stack.extend(&exit.args);
        let mut reads_dest = false;
        while let Some(v) = stack.pop() {
            let v = body.resolve_alias(v);
            if !seen.insert(v) {
                continue;
            }
            if dests.contains(&v) {
                reads_dest = true;
                break;
            }
            // Values used more than once are assigned to locals, not
            // inlined.
            if in_latch.contains(&v) && self.uses.get(&v).is_some_and(|&(n, _)| n == 1) {
                stack.extend(args(body, v));
            }
        }
        (!outside && !reads_dest).then_some((latch, exit.block))
    }

    /// Whether the loop `l` has only one edge to `b`.
    fn only_exit(&self, l: &Loop, b: Block) -> bool {
        l.blocks
            .iter()
            .flat_map(|&x| targets(&self.body.blocks[x].terminator))
            .filter(|t| t.block == b)
            .count()
            == 1
    }

    /// Whether branch `t` assigns any of its block's parameters.
    fn has_copies(&self, t: &waffle::BlockTarget) -> bool {
        let params = &self.body.blocks[t.block].params;
        t.args
            .iter()
            .zip(params)
            .any(|(&a, &(_, p))| self.names.contains_key(&p) && self.body.resolve_alias(a) != p)
    }

    /// The statements of nested block `b`, emitted inside the current
    /// block's terminator.
    fn nested_body(&mut self, b: Block, next: Option<Block>) -> Result<Vec<Stmt>> {
        let saved = std::mem::take(&mut self.out);
        let (block, in_term) = (self.block, self.in_term);
        let r = self.block(b, next);
        self.block = block;
        self.in_term = in_term;
        let stmts = std::mem::replace(&mut self.out, saved);
        r?;
        Ok(stmts)
    }

    /// Every entity value that isn't computed as `e + k` (a parameter, a
    /// call or load result, a block parameter...) is an entity, not a
    /// pointer to one of its fields.
    fn check_entities(&mut self) -> Result<()> {
        let body = self.body;
        for (_, def) in body.blocks.entries() {
            let values = def
                .params
                .iter()
                .map(|&(_, p)| p)
                .chain(def.insts.iter().copied());
            for v in values {
                let offset = matches!(
                    &body.values[v],
                    ValueDef::Operator(Operator::I32Add | Operator::I32Sub, _, _)
                ) && args(body, v).iter().any(|&a| const_val(body, a).is_some());
                if offset || !self.inf.funcs[&self.inst.func].has_value(v) {
                    continue;
                }
                if let (Kind::Entity, bias) = self.inf.value(self.inst, v)? {
                    ensure!(
                        bias == 0,
                        "a pointer to an entity's field is used as a value (only field \
                         accesses, `e.field`, are supported)"
                    );
                }
            }
        }
        Ok(())
    }

    fn count_uses(&mut self) {
        self.count_value_uses();
        // Shifts only used by a bit test.
        let body = self.body;
        for (_, def) in body.blocks.entries() {
            for &v in &def.insts {
                if let Some((s, ..)) = bit_test(body, v)
                    && self.uses.get(&s).is_some_and(|&(n, _)| n == 1)
                {
                    self.bit_tests.insert(s);
                }
            }
        }
        self.nonneg = nonneg_params(body);
    }

    fn count_value_uses(&mut self) {
        let body = self.body;
        let use_blocks = &mut self.use_blocks;
        let mut add = |uses: &mut HashMap<Value, (usize, Block)>, v: Value, b: Block| {
            let v = body.resolve_alias(v);
            use_blocks.entry(v).or_default().insert(b);
            if const_val(body, v).is_none() && const_expr(body, v).is_none() {
                let e = uses.entry(v).or_insert((0, b));
                e.0 += 1;
                e.1 = b;
            }
        };
        for (b, def) in body.blocks.entries() {
            for &v in &def.insts {
                if const_val(body, v).is_some() {
                    continue;
                }
                for a in args(body, v) {
                    add(&mut self.uses, a, b);
                }
            }
            match &def.terminator {
                Terminator::CondBr { cond: x, .. } | Terminator::Select { value: x, .. } => {
                    add(&mut self.uses, *x, b)
                }
                Terminator::Return { values } => {
                    for &x in values {
                        add(&mut self.uses, x, b);
                    }
                }
                _ => {}
            }
            for t in targets(&def.terminator) {
                for &x in &t.args {
                    add(&mut self.uses, x, b);
                }
            }
        }
    }

    fn emit(&mut self, s: Stmt) {
        self.record(&s);
        self.out.push(s);
    }

    fn emit_all(&mut self, stmts: Vec<Stmt>) {
        for s in stmts {
            self.emit(s);
        }
    }

    /// Record the locals `s` reads and assigns, in the current block.
    fn record(&mut self, s: &Stmt) {
        let mut uses = Vec::new();
        let mut defs = Vec::new();
        fn walk(s: &Stmt, uses: &mut Vec<String>, defs: &mut Vec<String>) {
            let read = |e: &Expr, uses: &mut Vec<String>| {
                e.visit_locals(&mut |n| uses.push(n.to_string()))
            };
            match s {
                Stmt::Assign(lhs, rhs) => {
                    read(rhs, uses);
                    match lhs {
                        Expr::Local(n) => defs.push(n.clone()),
                        // Component writes only go to temporaries, which
                        // aren't shared.
                        lhs => {
                            for c in lhs.children() {
                                read(c, uses);
                            }
                        }
                    }
                }
                Stmt::Expr(e) | Stmt::Return(Some(e)) => read(e, uses),
                Stmt::If(c, t, e) => {
                    read(c, uses);
                    for s in t.iter().chain(e) {
                        walk(s, uses, defs);
                    }
                }
                Stmt::Switch(x, arms, d) => {
                    read(x, uses);
                    for s in arms.iter().flat_map(|(_, b)| b).chain(d) {
                        walk(s, uses, defs);
                    }
                }
                Stmt::While(c, body) | Stmt::DoWhile(body, c) => {
                    read(c, uses);
                    for s in body {
                        walk(s, uses, defs);
                    }
                }
                Stmt::Return(None)
                | Stmt::Break
                | Stmt::Continue
                | Stmt::Goto(_)
                | Stmt::Label(_)
                | Stmt::Comment(_) => {}
            }
        }
        // Outside terminators there are no `if`s, so uses come before the
        // definition. Terminators record only their edge copies, not the
        // blocks nested in them (which record their own).
        walk(s, &mut uses, &mut defs);
        let rec = self.rec.entry(self.block).or_default();
        if self.in_term {
            rec.term_uses.extend(uses);
            rec.term_defs.extend(defs);
        } else {
            rec.events.extend(uses.into_iter().map(|n| (false, n)));
            rec.events.extend(defs.into_iter().map(|n| (true, n)));
        }
    }

    /// Give values with disjoint lifetimes the same local (see
    /// [`crate::alloc`]), renaming them in `stmts`. Returns the locals.
    fn share_locals(
        &mut self,
        order: &[Block],
        params: &[(String, Ty)],
        stmts: &mut Vec<Stmt>,
    ) -> Vec<(String, Ty)> {
        // Parameters, then the values' locals. Temporaries aren't shared.
        let mut vars = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for (n, ty) in params {
            index.insert(n.clone(), vars.len());
            vars.push(alloc::Var {
                ty: *ty,
                param: true,
            });
        }
        let (values, temps): (Vec<_>, Vec<_>) = std::mem::take(&mut self.locals)
            .into_iter()
            .partition(|(n, _)| n.starts_with('v'));
        for (n, ty) in &values {
            index.insert(n.clone(), vars.len());
            vars.push(alloc::Var {
                ty: *ty,
                param: false,
            });
        }
        let pos: HashMap<Block, usize> = order.iter().enumerate().map(|(i, &b)| (b, i)).collect();
        let ids = |ns: &[String]| -> Vec<usize> {
            ns.iter().filter_map(|n| index.get(n).copied()).collect()
        };
        let blocks: Vec<alloc::Block> = order
            .iter()
            .map(|b| {
                let rec = self.rec.remove(b).unwrap_or_default();
                alloc::Block {
                    events: rec
                        .events
                        .iter()
                        .filter_map(|(def, n)| {
                            let &i = index.get(n)?;
                            Some(if *def {
                                alloc::Event::Def(i)
                            } else {
                                alloc::Event::Use(i)
                            })
                        })
                        .collect(),
                    term_uses: ids(&rec.term_uses),
                    term_defs: ids(&rec.term_defs),
                    edges: rec
                        .edges
                        .iter()
                        .map(|edge| {
                            edge.iter()
                                .filter_map(|(d, plain, reads)| {
                                    Some(alloc::Copy {
                                        dest: *index.get(d)?,
                                        plain: plain.as_ref().and_then(|p| index.get(p).copied()),
                                        reads: ids(reads),
                                    })
                                })
                                .collect()
                        })
                        .collect(),
                    succs: targets(&self.body.blocks[*b].terminator)
                        .iter()
                        .filter_map(|t| pos.get(&t.block).copied())
                        .collect(),
                }
            })
            .collect();
        let colours = alloc::colour(&vars, &blocks);

        // Name the colours: parameters keep theirs.
        let mut names: HashMap<usize, String> = HashMap::new();
        for (i, (n, _)) in params.iter().enumerate() {
            names.insert(colours[i], n.clone());
        }
        let mut locals = Vec::new();
        let mut rename: HashMap<String, String> = HashMap::new();
        for (n, ty) in &values {
            let c = colours[index[n]];
            let new = names.entry(c).or_insert_with(|| {
                let name = format!("l{}", locals.len());
                locals.push((name.clone(), *ty));
                name
            });
            rename.insert(n.clone(), new.clone());
        }
        for s in stmts.iter_mut() {
            s.visit_locals_mut(&mut |n| {
                if let Some(new) = rename.get(n) {
                    *n = new.clone();
                }
            });
        }
        remove_self_copies(stmts);
        locals.extend(temps);
        locals
    }

    fn new_temp(&mut self, ty: QcType) -> String {
        let n = format!("t{}", self.next_temp);
        self.next_temp += 1;
        self.locals.push((n.clone(), Ty::Known(ty)));
        n
    }

    /// The local for value `v`, declaring it if needed.
    fn local_for(&mut self, v: Value) -> Result<String> {
        if let Some(n) = self.names.get(&v) {
            return Ok(n.clone());
        }
        let (k, _) = self.inf.value(self.inst, v)?;
        let n = format!("v{}", v.index());
        self.locals.push((n.clone(), Ty::Known(k.qc())));
        self.names.insert(v, n.clone());
        Ok(n)
    }

    fn kind(&mut self, v: Value) -> Result<Kind> {
        Ok(self.inf.value(self.inst, v)?.0)
    }

    /// The expression for operand `idx` of `site`, which is `v`.
    fn operand(&mut self, site: Site, idx: u32, v: Value) -> Result<Expr> {
        let body = self.body;
        let v = body.resolve_alias(v);
        if const_val(body, v).is_none()
            && self.inf.funcs[&self.inst.func].has_value(v)
            && let (Kind::Entity, bias) = self.inf.value(self.inst, v)?
        {
            ensure!(
                bias == 0,
                "a pointer to an entity's field is used as a value (only field accesses, \
                 `e.field`, are supported)"
            );
        }
        self.address(site, idx, v)
    }

    /// Operand `idx` of `site`, `v`, which may be a pointer to an entity's
    /// field (an address).
    fn address(&mut self, site: Site, idx: u32, v: Value) -> Result<Expr> {
        let body = self.body;
        let v = body.resolve_alias(v);
        if let Some(c) = const_val(body, v) {
            let (k, bias) = self.inf.operand_of(self.inst, body, site, idx, v)?;
            return self.constant(c, k, bias);
        }
        if let Some(e) = const_expr(body, v) {
            return e;
        }
        if let Some(e) = self.pending.remove(&v) {
            return Ok(e);
        }
        if let Some(n) = self.names.get(&v) {
            return Ok(Expr::Local(n.clone()));
        }
        bail!("{v} is used before it is defined")
    }

    /// The operands of instruction `v`.
    fn operands(&mut self, v: Value) -> Result<Vec<Expr>> {
        let a = args(self.body, v);
        let mut out = Vec::with_capacity(a.len());
        for (i, &x) in a.iter().enumerate() {
            out.push(self.operand(Site::Inst(v), i as u32, x)?);
        }
        Ok(out)
    }

    /// A constant `c`, used as a value of kind `k` with bias `bias`.
    fn constant(&mut self, c: i64, k: Kind, bias: i64) -> Result<Expr> {
        let addr = c.wrapping_add(bias);
        match k {
            Kind::StackOffset => bail!("internal error: an unreified shadow stack address"),
            Kind::Entity => {
                ensure!(
                    addr == 0,
                    "entity constant {addr} (only null, 0, is supported)"
                );
                Ok(Expr::Null)
            }
            Kind::Float => {
                if c.unsigned_abs() > MAX_EXACT_INT as u64 {
                    self.ctx.warnings.push(format!(
                        "integer constant {c} is not exactly representable as a QuakeC float"
                    ));
                }
                Ok(Expr::Float(c as f32))
            }
            Kind::FloatBits => {
                let f = f32::from_bits(c as u32);
                ensure!(f.is_finite(), "non-finite float constant {f}");
                Ok(Expr::Float(f))
            }
            Kind::Func => {
                let Some(&f) = self.ctx.table_funcs.get(&(addr as u64)) else {
                    ensure!(
                        addr == 0,
                        "function pointer {addr} is not in the function table"
                    );
                    return Ok(Expr::Null);
                };
                Ok(self.func_value(f))
            }
            Kind::Field(t) => {
                let name = self.field_name(addr, t)?;
                Ok(Expr::Global(name))
            }
            Kind::String => {
                if addr == 0 {
                    return Ok(Expr::Null);
                }
                let bytes = self.ctx.data.c_string(addr as u64).ok_or_else(|| {
                    anyhow!("string literal at address {addr} is not a NUL-terminated string in static data")
                })?;
                Ok(Expr::Str(bytes))
            }
            Kind::Vector => bail!("unsupported {k} constant {c}"),
        }
    }

    /// A function value (always typed `void()`, see [`Self::fn_cast`]).
    fn func_value(&self, f: Func) -> Expr {
        // Functions in the table are roots, and their first instance has the
        // base name.
        let name = self.ctx.funcs[&f].name.clone();
        self.fn_cast(&Sig::void(), Expr::FuncName(name))
    }

    /// Cast a function value to `sig`, unless it already has that type.
    fn fn_cast(&self, sig: &Sig, f: Expr) -> Expr {
        let f = match f {
            Expr::FnCast(_, inner) => *inner,
            f => f,
        };
        let own = match &f {
            Expr::FuncName(n) => self.sigs.get(n).cloned(),
            _ => Some(Sig::void()),
        };
        if own.as_ref() == Some(sig) {
            f
        } else {
            Expr::FnCast(sig.clone(), Box::new(f))
        }
    }

    /// The name of the field at `offset` (a system field offset, or the
    /// address of a user field's name).
    fn field_name(&mut self, offset: i64, ty: QcType) -> Result<String> {
        ensure!(offset >= 0, "negative entity field offset {offset}");
        if StaticData::is_data_addr(offset as u64) {
            return self.ctx.user_name(offset as u64, ty, true);
        }
        self.ctx.defs.field_at(offset as u32, ty).ok_or_else(|| {
            anyhow!(
                "entity field offset {offset} is not a known {} field",
                ty.name()
            )
        })
    }

    /// The QuakeC global at constant address `addr` (a system global slot,
    /// or the address of a user global's name).
    fn global_at(&mut self, addr: i64, expect: QcType) -> Result<String> {
        ensure!(addr >= 0, "negative memory address {addr}");
        let addr = addr as u64;
        if StaticData::is_data_addr(addr) {
            return self.ctx.user_name(addr, expect, false);
        }
        if expect == QcType::Vector {
            let slot = self.ctx.globals.by_addr(addr);
            return match slot.and_then(|s| s.vector.filter(|_| s.name.ends_with("_x"))) {
                Some(v) => Ok(v.to_string()),
                None => bail!("memory address {addr} is not a vector QuakeC global"),
            };
        }
        let slot = self.ctx.globals.by_addr(addr).ok_or_else(|| {
            anyhow!(
                "memory address {addr} (slot {}) is not a known QuakeC global",
                addr / 4
            )
        })?;
        ensure!(
            slot.ty == expect,
            "memory address {addr} is `{}` of type {}, but was accessed as {}",
            slot.name,
            slot.ty.name(),
            expect.name()
        );
        Ok(slot.name.clone())
    }

    /// The QuakeC global at the address computed by `e` (from operand `v`).
    /// LLVM merges `c ? a.get() : b.get()` into one load from a selected
    /// address, so this may be a `select`, or a value that is one of several
    /// constants (compared at run time).
    fn global_ref(&mut self, e: Expr, v: Value, t: QcType) -> Result<Expr> {
        match e {
            Expr::Float(a) => Ok(Expr::Global(self.global_at(a as i64, t)?)),
            Expr::Select(c, a, b) if matches!((&*a, &*b), (Expr::Float(_), Expr::Float(_))) => {
                let (a, b) = (self.global_ref(*a, v, t)?, self.global_ref(*b, v, t)?);
                Ok(Expr::Select(c, Box::new(a), Box::new(b)))
            }
            e => {
                let mut consts = Vec::new();
                ensure!(
                    const_set(self.body, v, 0, &mut HashSet::new(), &mut consts),
                    "address of QuakeC global is not a constant: `{e:?}`"
                );
                consts.sort();
                consts.dedup();
                let mut good = Vec::new();
                let mut arms = Vec::new();
                for c in consts {
                    // Addresses LLVM never uses on this path may be invalid.
                    if let Ok(g) = self.global_at(c, t) {
                        good.push(c as f32);
                        arms.push(Expr::Global(g));
                    }
                }
                ensure!(
                    !arms.is_empty(),
                    "no value of `{e:?}` is a QuakeC global address"
                );
                let e = self.simple(e, QcType::Float);
                Ok(select_chain(&e, &good, arms))
            }
        }
    }

    /// Record the value of `v`: inlined into its single use if that is later
    /// in this block, else assigned to its local.
    fn define(&mut self, v: Value, e: Expr) -> Result<()> {
        let (n, b) = self.uses.get(&v).copied().unwrap_or((0, self.block));
        if n == 0 {
            // Unused. Pure, so nothing to emit.
            return Ok(());
        }
        // A copy of a local (e.g. an offset string address) is that local.
        // Locals are only reassigned on edges into the block defining
        // them, which the copy's uses can't be reached through.
        if let Expr::Local(name) = &e {
            self.names.insert(v, name.clone());
            return Ok(());
        }
        if n == 1 && b == self.block {
            self.pending.insert(v, e);
            return Ok(());
        }
        self.assign(v, e)
    }

    /// Emit `local(v) = e`.
    fn assign(&mut self, v: Value, e: Expr) -> Result<()> {
        let name = self.local_for(v)?;
        self.assign_to(&name, e);
        Ok(())
    }

    fn assign_to(&mut self, name: &str, e: Expr) {
        match e {
            Expr::MakeVector(parts) => {
                let parts = *parts;
                let e = self.materialize(Expr::MakeVector(Box::new(parts)));
                self.emit(Stmt::Assign(Expr::Local(name.into()), e));
            }
            e => self.emit(Stmt::Assign(Expr::Local(name.into()), e)),
        }
    }

    /// Before a side effect: emit pending values that read state it could
    /// change.
    fn flush(&mut self) -> Result<()> {
        let mut vs: Vec<Value> = self
            .pending
            .iter()
            .filter(|(_, e)| e.reads_globals())
            .map(|(&v, _)| v)
            .collect();
        vs.sort();
        for v in vs {
            let e = self.pending.remove(&v).unwrap();
            self.assign(v, e)?;
        }
        Ok(())
    }

    /// Emit a call: `local(v) = call;` if it has a used result.
    fn call(&mut self, v: Value, call: Expr) -> Result<()> {
        self.flush()?;
        let used = self.uses.get(&v).is_some_and(|&(n, _)| n > 0);
        if used {
            self.assign(v, call)
        } else {
            self.emit(Stmt::Expr(call));
            Ok(())
        }
    }

    fn inst(&mut self, v: Value) -> Result<()> {
        let body = self.body;
        let ValueDef::Operator(op, _, _) = &body.values[v] else {
            return Ok(());
        };
        if let Some(names) = self.inf.field_names(self.inst.func, v) {
            let names = names.to_vec();
            return self.field_access(v, op, &names);
        }
        use Operator as O;
        match op {
            O::Nop => {}
            O::Unreachable => {
                self.emit(Stmt::Comment("unreachable".into()));
            }
            // `x + k`: an offset unless `x` is a number.
            O::I32Add | O::I32Sub
                if const_val(body, args(body, v)[0]).is_some()
                    != const_val(body, args(body, v)[1]).is_some() =>
            {
                let a = args(body, v);
                let (k, _) = self.inf.value(self.inst, v)?;
                let (x, c) = match (const_val(body, a[0]), const_val(body, a[1])) {
                    (None, Some(c)) => (0, c),
                    (Some(c), None) => (1, c),
                    _ => unreachable!(),
                };
                if k == Kind::Float {
                    let ops = self.operands(v)?;
                    let [a0, a1] = ops.try_into().unwrap();
                    let op = if *op == O::I32Add {
                        BinOp::Add
                    } else {
                        BinOp::Sub
                    };
                    self.define(v, fold(op, a0, a1))?;
                } else {
                    ensure!(
                        x == 0 || *op == O::I32Add,
                        "{k} value computed as `{c} - x`"
                    );
                    let e = self.address(Site::Inst(v), x as u32, a[x])?;
                    self.define(v, e)?;
                }
            }
            O::I32Eq | O::I32Ne | O::I64Eq | O::I64Ne => {
                let [a0, a1]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let op = if matches!(op, O::I32Eq | O::I64Eq) {
                    BinOp::Eq
                } else {
                    BinOp::Ne
                };
                self.define(v, fold(op, a0, a1))?;
            }
            O::I32Eqz | O::I64Eqz => {
                let [a0]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                self.define(v, not(a0))?;
            }
            O::Select | O::TypedSelect { .. } => {
                let [a, b, c]: [Expr; 3] = self.operands(v)?.try_into().unwrap();
                let (a, b) = (self.materialize(a), self.materialize(b));
                self.define(v, Expr::Select(Box::new(c), Box::new(a), Box::new(b)))?;
            }
            O::GlobalGet { global_index } => {
                let name = self
                    .ctx
                    .wasm_globals
                    .get(global_index)
                    .cloned()
                    .ok_or_else(|| anyhow!("unsupported global"))?;
                self.define(v, Expr::Global(name))?;
            }
            O::GlobalSet { global_index } => {
                let name = self
                    .ctx
                    .wasm_globals
                    .get(global_index)
                    .cloned()
                    .ok_or_else(|| anyhow!("unsupported global"))?;
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let x = self.materialize(x);
                self.flush()?;
                self.emit(Stmt::Assign(Expr::Global(name), x));
            }
            O::Call { function_index } => self.lower_call(v, *function_index)?,
            O::CallIndirect { sig_index, .. } => {
                let mut ops = self.operands(v)?;
                let f = ops.pop().unwrap();
                let ops = ops.into_iter().map(|e| self.materialize(e)).collect();
                let sig = self.sig_of_type(*sig_index, v)?;
                let callee = self.fn_cast(&sig, f);
                let callee = match callee {
                    e @ (Expr::FuncName(_)
                    | Expr::Local(_)
                    | Expr::Global(_)
                    | Expr::Field(..)
                    | Expr::FnCast(..)) => e,
                    e => Expr::Local(self.simple_name(e, QcType::Func)),
                };
                self.call(v, Expr::Call(Box::new(callee), ops))?;
            }
            O::F32Load { .. }
            | O::I32Load { .. }
            | O::I32Load8S { .. }
            | O::I32Load8U { .. }
            | O::I32Load16S { .. }
            | O::I32Load16U { .. } => self.load(v, op)?,
            O::F32Store { memory } => {
                let [addr, x]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let Expr::Float(a) = addr else {
                    bail!("stores must be to constant QuakeC global addresses, found `{addr:?}`")
                };
                let a = a as i64 + memory.offset as i64;
                ensure!(
                    !StaticData::is_data_addr(a as u64),
                    "writes to static data are not supported (address {a}); Zig `var` globals \
                     can't be used, declare QuakeC globals with `defs.userGlobal` instead"
                );
                let g = self.global_at(a, QcType::Float)?;
                self.flush()?;
                self.emit(Stmt::Assign(Expr::Global(g), x));
            }
            O::V128Load32Zero { memory } | O::V128Load64Zero { memory } => {
                let [addr]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let Expr::Float(a) = addr else {
                    bail!("vector loads must be from constant QuakeC global addresses")
                };
                let n = if matches!(op, O::V128Load32Zero { .. }) {
                    1
                } else {
                    2
                };
                let mut parts = [Expr::Float(0.0), Expr::Float(0.0), Expr::Float(0.0)];
                for (i, p) in parts.iter_mut().enumerate().take(n) {
                    let at = a as i64 + memory.offset as i64 + 4 * i as i64;
                    *p = Expr::Global(self.global_at(at, QcType::Float)?);
                }
                self.define(v, Expr::MakeVector(Box::new(parts)))?;
            }
            O::V128Load32Lane { memory, lane } => {
                let [addr, vec]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let Expr::Float(a) = addr else {
                    bail!("vector loads must be from constant QuakeC global addresses")
                };
                let x =
                    Expr::Global(self.global_at(a as i64 + memory.offset as i64, QcType::Float)?);
                let e = self.replace_lane(vec, *lane, x);
                self.define(v, e)?;
            }
            O::V128Store32Lane { memory, lane } => {
                let [addr, vec]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let Expr::Float(a) = addr else {
                    bail!("vector stores must be to constant QuakeC global addresses")
                };
                let g = self.global_at(a as i64 + memory.offset as i64, QcType::Float)?;
                let x = self.extract_lane(vec, *lane);
                self.flush()?;
                self.emit(Stmt::Assign(Expr::Global(g), x));
            }
            O::F32x4ReplaceLane { lane } => {
                let [vec, x]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let e = self.replace_lane(vec, *lane, x);
                self.define(v, e)?;
            }
            O::F32x4ExtractLane { lane } | O::I32x4ExtractLane { lane } => {
                let [vec]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let e = self.extract_lane(vec, *lane);
                self.define(v, e)?;
            }
            O::F32x4Eq
            | O::F32x4Ne
            | O::F32x4Lt
            | O::F32x4Gt
            | O::F32x4Le
            | O::F32x4Ge
            | O::F32x4PMin
            | O::F32x4PMax => {
                let [a, b]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let e = self.lanewise(op, a, b);
                self.define(v, e)?;
            }
            // Bitwise not on integer lanes (including masks, see
            // `lanewise`): `-1 - x`.
            O::V128Not => {
                let [m]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let parts = self
                    .vector_lanes(m)
                    .map(|x| fold(BinOp::Sub, Expr::Float(-1.0), x));
                self.define(v, Expr::MakeVector(Box::new(parts)))?;
            }
            // `m ? a : b` lane by lane, for a mask `m` (see `lanewise`).
            O::V128Bitselect => {
                let [a, b, m]: [Expr; 3] = self.operands(v)?.try_into().unwrap();
                let [a0, a1, a2] = self.vector_lanes(a);
                let [b0, b1, b2] = self.vector_lanes(b);
                let [m0, m1, m2] = self.vector_lanes(m);
                let sel = |m, a, b| Expr::Select(Box::new(m), Box::new(a), Box::new(b));
                let parts = [sel(m0, a0, b0), sel(m1, a1, b1), sel(m2, a2, b2)];
                self.define(v, Expr::MakeVector(Box::new(parts)))?;
            }
            O::I8x16Shuffle { lanes } => {
                let [a, b]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let e = self.shuffle(a, b, lanes)?;
                self.define(v, e)?;
            }
            O::F32x4Mul | O::F32x4Div => {
                let [a, b]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let e = self.vector_mul_div(*op == O::F32x4Div, a, b);
                self.define(v, e)?;
            }
            O::F32x4Add | O::F32x4Sub => {
                let [a, b]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let (a, b) = (self.materialize(a), self.materialize(b));
                let op = if *op == O::F32x4Add {
                    BinOp::Add
                } else {
                    BinOp::Sub
                };
                self.define(v, Expr::Bin(op, Box::new(a), Box::new(b)))?;
            }
            O::F32x4Splat => {
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let e = self.splat(x);
                self.define(v, e)?;
            }
            O::F32x4Neg => {
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let x = self.materialize(x);
                self.define(v, Expr::Un(UnOp::Neg, Box::new(x)))?;
            }
            // Float → int truncation. `OP_BITOR` converts both operands to
            // int (truncating toward zero) and back. `x | 0` would be folded
            // away by fteqcc, but `x | x` is kept.
            O::I32TruncF32S | O::I32TruncF32U | O::I32TruncSatF32S | O::I32TruncSatF32U => {
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let e = match x {
                    Expr::Float(f) => Expr::Float(f.trunc()),
                    x => {
                        let x = self.simple(x, QcType::Float);
                        Expr::Bin(BinOp::BitOr, Box::new(x.clone()), Box::new(x))
                    }
                };
                self.define(v, e)?;
            }
            O::F32Neg => {
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                self.define(v, Expr::Un(UnOp::Neg, Box::new(x)))?;
            }
            // Integers are already floats.
            O::F32ConvertI32S | O::F32ConvertI32U => {
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                self.define(v, x)?;
            }
            // Emitted as part of the bit test using it.
            O::I32Shl | O::I32ShrU if self.bit_tests.contains(&v) => {}
            O::I32And if bit_test(body, v).is_some_and(|(s, ..)| self.bit_tests.contains(&s)) => {
                let (s, x, mask, shl) = bit_test(body, v).unwrap();
                let x = self.operand(Site::Inst(s), 1, x)?;
                let x = self.simple(x, QcType::Float);
                // `(1 << x) & mask` is `2^x` if bit `x` of `mask` is set, and
                // `(mask >> x) & 1` is 1; both are 0 otherwise.
                let mut e = Expr::Float(0.0);
                for bit in (0..32).rev().filter(|b| mask & (1 << b) != 0) {
                    let val = if shl { (1u64 << bit) as f32 } else { 1.0 };
                    let is = Expr::Bin(
                        BinOp::Eq,
                        Box::new(x.clone()),
                        Box::new(Expr::Float(bit as f32)),
                    );
                    e = Expr::Select(Box::new(is), Box::new(Expr::Float(val)), Box::new(e));
                }
                self.define(v, e)?;
            }
            // Integer division truncates toward zero, as `OP_BITOR` does
            // (see the truncation above). Exact for magnitudes below 2^24.
            O::I32DivS | O::I32DivU | O::I32RemS | O::I32RemU => {
                let [a, b]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let rem = matches!(op, O::I32RemS | O::I32RemU);
                if let (Expr::Float(x), Expr::Float(y)) = (&a, &b) {
                    let (x, y) = (*x as i64, *y as i64);
                    ensure!(y != 0, "integer division by zero");
                    let r = if rem { x % y } else { x / y };
                    self.define(v, Expr::Float(r as f32))?;
                    return Ok(());
                }
                // Operands used twice are evaluated once (constants can be
                // repeated).
                let once = |this: &mut Self, e: Expr| match e {
                    e @ Expr::Float(_) => e,
                    e if rem => this.simple(e, QcType::Float),
                    e => e,
                };
                let a = once(self, a);
                let b = once(self, b);
                let q = Expr::Bin(BinOp::Div, Box::new(a.clone()), Box::new(b.clone()));
                let q = self.simple(q, QcType::Float);
                let q = Expr::Bin(BinOp::BitOr, Box::new(q.clone()), Box::new(q));
                let e = if rem {
                    Expr::Bin(
                        BinOp::Sub,
                        Box::new(a),
                        Box::new(Expr::Bin(BinOp::Mul, Box::new(b), Box::new(q))),
                    )
                } else {
                    q
                };
                self.define(v, e)?;
            }
            O::I32x4Eq
            | O::I32x4Ne
            | O::I32x4LtS
            | O::I32x4LtU
            | O::I32x4GtS
            | O::I32x4GtU
            | O::I32x4LeS
            | O::I32x4LeU
            | O::I32x4GeS
            | O::I32x4GeU => {
                let [a, b]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let e = self.lanewise(op, a, b);
                self.define(v, e)?;
            }
            O::I32x4Add | O::I32x4Sub => {
                let [a, b]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let (a, b) = (self.materialize(a), self.materialize(b));
                let op = if *op == O::I32x4Add {
                    BinOp::Add
                } else {
                    BinOp::Sub
                };
                self.define(v, fold_vector(op, a, b))?;
            }
            O::I32x4Mul => {
                let [a, b]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let e = self.lanes2(a, b, |x, y| fold(BinOp::Mul, x, y));
                self.define(v, e)?;
            }
            O::I32x4Neg => {
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let parts = self
                    .vector_lanes(x)
                    .map(|x| fold(BinOp::Sub, Expr::Float(0.0), x));
                self.define(v, Expr::MakeVector(Box::new(parts)))?;
            }
            O::I32x4Shl => {
                let [a, k]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let Expr::Float(k) = k else {
                    bail!("unsupported i32x4.shl by a non-constant amount")
                };
                let m = Expr::Float((1u64 << (k as u32 & 31)) as f32);
                let parts = self.vector_lanes(a).map(|x| fold(BinOp::Mul, x, m.clone()));
                self.define(v, Expr::MakeVector(Box::new(parts)))?;
            }
            O::I32x4Splat => {
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let e = self.splat(x);
                self.define(v, e)?;
            }
            O::I32x4ReplaceLane { lane } => {
                let [vec, x]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let e = self.replace_lane(vec, *lane, x);
                self.define(v, e)?;
            }
            // With a mask, a lane-wise `select`, which is exact for floats
            // too. Otherwise on integer lanes.
            O::V128And | O::V128Or | O::V128AndNot => {
                let a = args(body, v);
                let (ma, mb) = (is_mask(body, a[0]), is_mask(body, a[1]));
                let [x, y]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let sel =
                    |c: Expr, t: Expr, f: Expr| Expr::Select(Box::new(c), Box::new(t), Box::new(f));
                let all = Expr::Float(-1.0);
                let zero = Expr::Float(0.0);
                let e = match (op, ma, mb) {
                    (O::V128And, _, true) => self.lanes2(x, y, |x, m| sel(m, x, zero.clone())),
                    (O::V128And, true, false) => self.lanes2(x, y, |m, x| sel(m, x, zero.clone())),
                    (O::V128Or, _, true) => self.lanes2(x, y, |x, m| sel(m, all.clone(), x)),
                    (O::V128Or, true, false) => self.lanes2(x, y, |m, x| sel(m, all.clone(), x)),
                    // `x & !m`
                    (O::V128AndNot, _, true) => self.lanes2(x, y, |x, m| sel(m, zero.clone(), x)),
                    (O::V128And, ..) => self.lanes2(x, y, |x, y| fold(BinOp::BitAnd, x, y)),
                    (O::V128Or, ..) => self.lanes2(x, y, |x, y| fold(BinOp::BitOr, x, y)),
                    _ => self.lanes2(x, y, |x, y| {
                        fold(BinOp::BitAnd, x, fold(BinOp::Sub, Expr::Float(-1.0), y))
                    }),
                };
                self.define(v, e)?;
            }
            // Unsigned comparisons. Integers are floats, so a value that
            // wrapped below 0 (like `x - 1` in a `switch`'s range check) is
            // negative, but compares as at least 2^31.
            O::I32LtU | O::I32GtU | O::I32LeU | O::I32GeU => {
                let a = args(body, v);
                let (na, nb) = (
                    nonneg(body, &self.nonneg, a[0]),
                    nonneg(body, &self.nonneg, a[1]),
                );
                let [x, y]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                // `x op x - c` (LLVM's `umax`/`usub.sat` idiom): `x - c`
                // wraps exactly when `x <u c`, so this is `x >=u c` or
                // `x <u c`.
                let minus = |a: Value, b: Value| {
                    let c = -sub_const(body, b, a)?;
                    (c > 0 && c < 1 << 24).then_some(c as f32)
                };
                let e = if let Some(c) = minus(a[0], a[1]) {
                    let ge = matches!(op, O::I32GtU | O::I32GeU);
                    let op = if ge { O::I32GeU } else { O::I32LtU };
                    self.unsigned_cmp(&op, x, Expr::Float(c), na, true)
                } else if let Some(c) = minus(a[1], a[0]) {
                    // `x - c op x`: the same, flipped.
                    let lt = matches!(op, O::I32LtU | O::I32LeU);
                    let op = if lt { O::I32GeU } else { O::I32LtU };
                    self.unsigned_cmp(&op, y, Expr::Float(c), nb, true)
                } else {
                    self.unsigned_cmp(op, x, y, na, nb)
                };
                self.define(v, e)?;
            }
            op => {
                let [a, b]: [Expr; 2] = match self.operands(v)?.try_into() {
                    Ok(ops) => ops,
                    Err(_) => bail!("unsupported operator {op:?}"),
                };
                let e = binop(op, a, b)?;
                self.define(v, e)?;
            }
        }
        Ok(())
    }

    /// `x op y` for an unsigned comparison `op`, where `nx` and `ny` say
    /// whether each operand is known to be non-negative.
    fn unsigned_cmp(&mut self, op: &Operator, x: Expr, y: Expr, nx: bool, ny: bool) -> Expr {
        use Operator as O;
        let signed = match op {
            O::I32LtU => BinOp::Lt,
            O::I32GtU => BinOp::Gt,
            O::I32LeU => BinOp::Le,
            _ => BinOp::Ge,
        };
        let bin = |op, a: Expr, b: Expr| Expr::Bin(op, Box::new(a), Box::new(b));
        if let (Expr::Float(a), Expr::Float(b)) = (&x, &y) {
            let (a, b) = (*a as i64 as u32, *b as i64 as u32);
            let r = match signed {
                BinOp::Lt => a < b,
                BinOp::Gt => a > b,
                BinOp::Le => a <= b,
                _ => a >= b,
            };
            return Expr::Float(r as u32 as f32);
        }
        if nx && ny {
            return fold(signed, x, y);
        }
        // A negative operand compares as greater than any non-negative one.
        let greater = matches!(signed, BinOp::Gt | BinOp::Ge);
        let neg = |e: Expr| bin(BinOp::Lt, e, Expr::Float(0.0));
        let pos = |e: Expr| bin(BinOp::Ge, e, Expr::Float(0.0));
        if ny {
            // `x <u y` is `x >= 0 && x < y`; `x >u y` is `x < 0 || x > y`.
            let x = self.simple(x, QcType::Float);
            let cmp = bin(signed, x.clone(), y);
            return if greater {
                bin(BinOp::BitOr, neg(x), cmp)
            } else {
                bin(BinOp::BitAnd, pos(x), cmp)
            };
        }
        let y = self.simple(y, QcType::Float);
        if nx {
            // `x <u y` is `y < 0 || x < y`; `x >u y` is `y >= 0 && x > y`.
            let cmp = bin(signed, x, y.clone());
            return if greater {
                bin(BinOp::BitAnd, pos(y), cmp)
            } else {
                bin(BinOp::BitOr, neg(y), cmp)
            };
        }
        // The signed comparison, flipped if the signs differ.
        let x = self.simple(x, QcType::Float);
        let cmp = bin(signed, x.clone(), y.clone());
        bin(BinOp::Ne, cmp, bin(BinOp::Ne, neg(x), neg(y)))
    }

    /// A load or store `v` of the entity fields `names` (one per cell).
    fn field_access(&mut self, v: Value, op: &Operator, names: &[String]) -> Result<()> {
        let body = self.body;
        let a = args(body, v);
        let (store, _, cells) = crate::memory::cells(op)
            .ok()
            .flatten()
            .expect("a memory access");
        let ent = self.address(Site::Inst(v), 0, a[0])?;
        let ent = self.simple(ent, QcType::Entity);
        let field = |n: &String| Expr::Field(Box::new(ent.clone()), n.clone());
        let lanes: Vec<Option<u8>> = cells.iter().map(|c| c.lane).collect();
        if store {
            let x = self.operand(Site::Inst(v), 1, a[1])?;
            let x = if lanes.len() > 1 {
                self.simple(x, QcType::Vector)
            } else {
                self.materialize(x)
            };
            let mut stmts = Vec::new();
            for (lane, n) in lanes.iter().zip(names) {
                let rhs = match lane {
                    Some(l) => self.extract_lane(x.clone(), *l),
                    None => x.clone(),
                };
                stmts.push(Stmt::Assign(field(n), rhs));
            }
            self.flush()?;
            for s in stmts {
                self.emit(s);
            }
            return Ok(());
        }
        if lanes[0].is_none() {
            return self.define(v, field(&names[0]));
        }
        // Lanes into a vector: the one loaded into, or zeros.
        let mut vec = match op {
            Operator::V128Load32Lane { .. } | Operator::V128Load64Lane { .. } => {
                self.operand(Site::Inst(v), 1, a[1])?
            }
            _ => Expr::Vector([0.0; 3]),
        };
        for (lane, n) in lanes.iter().zip(names) {
            vec = self.replace_lane(vec, lane.unwrap(), field(n));
        }
        self.define(v, whole_field(vec))
    }

    fn sig_of_type(&mut self, sig: waffle::Signature, v: Value) -> Result<Sig> {
        let s = &self.ctx.module.signatures[sig];
        let default = |t: &Type| -> Result<QcType> {
            Ok(match t {
                Type::I32 | Type::F32 => QcType::Float,
                Type::V128 => QcType::Vector,
                other => bail!("unsupported type {other:?}"),
            })
        };
        // The arguments' kinds (the last operand is the table index).
        let body = self.body;
        let a = args(body, v);
        let mut params = Vec::new();
        for (i, &x) in a[..s.params.len()].iter().enumerate() {
            let (k, _) = self
                .inf
                .operand_of(self.inst, body, Site::Inst(v), i as u32, x)?;
            params.push(k.qc());
        }
        let ret = match (s.returns.as_slice(), self.uses.contains_key(&v)) {
            ([], _) => QcType::Void,
            ([_], true) => self.kind(v)?.qc(),
            ([t], false) => default(t)?,
            _ => bail!("multi-value returns are not supported"),
        };
        Ok(Sig {
            params: params.into_iter().map(Ty::Known).collect(),
            ret: Ty::Known(ret),
        })
    }

    fn load(&mut self, v: Value, op: &Operator) -> Result<()> {
        let (dk, offset) = DataKind::from_load(op).expect("a load");
        let [addr]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
        let used = self.uses.get(&v).is_some_and(|&(n, _)| n > 0);
        // A pointer stored in static data.
        let (k, bias) = if used {
            self.inf.value(self.inst, v)?
        } else {
            (Kind::Float, 0)
        };
        if k != Kind::Float {
            ensure!(
                matches!(k, Kind::String | Kind::Func) && dk == DataKind::I32 && bias == 0,
                "unsupported load of a {k} from static data"
            );
            let addr = fold(BinOp::Add, addr, Expr::Float(offset as f32));
            if let Expr::Float(a) = addr {
                let x = self
                    .ctx
                    .data
                    .read_u32(a as u64)
                    .ok_or_else(|| anyhow!("load from address {a}, outside of static data"))?;
                let e = self.constant(x as i64, k, 0)?;
                return self.define(v, e);
            }
            self.ctx.pointer_accessors.insert(k.qc());
            let name = crate::translate::pointer_accessor_name(k.qc());
            let call = Expr::Call(Box::new(Expr::FuncName(name)), vec![addr]);
            return self.assign(v, call);
        }
        if let Expr::Float(a) = addr {
            let a = a as i64 + offset as i64;
            if !StaticData::is_data_addr(a as u64) {
                ensure!(
                    dk == DataKind::F32,
                    "unsupported load {op:?}: QuakeC globals must be read as f32 \
                     (or via __quakec_load_* for entities, strings and functions)"
                );
                let g = self.global_at(a, QcType::Float)?;
                return self.define(v, Expr::Global(g));
            }
            let x = self
                .ctx
                .data
                .read(a as u64, dk)
                .ok_or_else(|| anyhow!("load from address {a}, outside of static data"))?;
            if x.fract() == 0.0 && x.abs() > MAX_EXACT_INT as f32 && dk != DataKind::F32 {
                self.ctx.warnings.push(format!(
                    "static data value {x} is not exactly representable as a QuakeC float"
                ));
            }
            return self.define(v, Expr::Float(x));
        }
        // A dynamic address: assume it points into static data, since QuakeC
        // globals can't be indexed dynamically.
        self.ctx.data_accessors.insert(dk);
        let addr = if offset == 0 {
            addr
        } else {
            fold(BinOp::Add, addr, Expr::Float(offset as f32))
        };
        let call = Expr::Call(
            Box::new(Expr::FuncName(dk.accessor_name().into())),
            vec![addr],
        );
        // Reads only static data: no need to flush.
        let used = self.uses.get(&v).is_some_and(|&(n, _)| n > 0);
        if used {
            self.assign(v, call)?;
        }
        Ok(())
    }

    fn lower_call(&mut self, v: Value, callee: Func) -> Result<()> {
        let info = &self.ctx.funcs[&callee];
        let (name, kind) = (info.name.clone(), info.kind);
        let FuncKind::Intrinsic(intr) = kind else {
            let ops = self.operands(v)?;
            let ops = ops.into_iter().map(|e| self.materialize(e)).collect();
            let name = match kind {
                FuncKind::Local => {
                    let j = self.inst.callees[&v];
                    self.instances[j].name.clone()
                }
                _ => name,
            };
            // Function arguments are `void()`; cast named functions.
            return self.call(v, Expr::Call(Box::new(Expr::FuncName(name)), ops));
        };

        let a = args(self.body, v);
        use Intrinsic as I;
        match intr {
            I::LoadGlobal(t) => {
                let [addr]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let e = self.global_ref(addr, a[0], t)?;
                self.define(v, e)?;
            }
            I::StoreGlobal(t) => {
                let [addr, x]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let Expr::Float(addr) = addr else {
                    bail!("address of QuakeC global is not a constant: `{addr:?}`")
                };
                let g = self.global_at(addr as i64, t)?;
                let x = self.materialize(x);
                self.flush()?;
                self.emit(Stmt::Assign(Expr::Global(g), x));
            }
            I::LoadField(_) => {
                let [ent, field]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let e = self.field(ent, field);
                self.define(v, e)?;
            }
            I::StoreField(_) => {
                let [ent, field, x]: [Expr; 3] = self.operands(v)?.try_into().unwrap();
                let lhs = self.field(ent, field);
                let x = self.materialize(x);
                self.flush()?;
                self.emit(Stmt::Assign(lhs, x));
            }
            I::Eq(_) | I::Ne(_) | I::DotVector => {
                let [x, y]: [Expr; 2] = self.operands(v)?.try_into().unwrap();
                let (x, y) = (self.materialize(x), self.materialize(y));
                let op = match intr {
                    I::Eq(_) => BinOp::Eq,
                    I::Ne(_) => BinOp::Ne,
                    _ => BinOp::Mul,
                };
                self.define(v, Expr::Bin(op, Box::new(x), Box::new(y)))?;
            }
            I::Not(_) => {
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let x = self.materialize(x);
                self.define(v, not(x))?;
            }
            // The same QuakeC value as the operand.
            I::FieldRefString | I::StringLiteral => {
                let [x]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                self.define(v, x)?;
            }
        }
        Ok(())
    }

    /// `ent.field`, for a field reference `field`.
    fn field(&mut self, ent: Expr, field: Expr) -> Expr {
        let ent = self.simple(ent, QcType::Entity);
        match field {
            Expr::Global(name) => Expr::Field(Box::new(ent), name),
            f => {
                // A field reference value: `ent.(f)`.
                let f = self.simple(f, QcType::FloatField);
                Expr::DynField(Box::new(ent), Box::new(f))
            }
        }
    }

    fn terminator(&mut self, b: Block, next: Option<Block>) -> Result<()> {
        let body = self.body;
        let site = Site::Term(b);
        match &body.blocks[b].terminator {
            Terminator::Br { target } => {
                let copies = self.edge(b, 0, target)?;
                self.emit_all(copies);
                let tail = self.tail(target.block, next)?;
                self.out.extend(tail);
            }
            Terminator::CondBr {
                cond,
                if_true,
                if_false,
            } => {
                let c = self.operand(site, TERM_COND, *cond)?;
                let tc = self.edge(b, 0, if_true)?;
                let fc = self.edge(b, 1, if_false)?;
                self.record(&Stmt::If(c.clone(), tc.clone(), fc.clone()));
                let tt = self.tail(if_true.block, next)?;
                let ft = self.tail(if_false.block, next)?;
                // Going round the loop at the end of its body needs no
                // statement, but an explicit `continue` lets the other
                // branch follow the `if`.
                let falls_to_header = |blk: Block, tail: &[Stmt]| {
                    tail.is_empty()
                        && Some(blk) == next
                        && self
                            .loop_ctx
                            .last()
                            .is_some_and(|c| c.header == blk && !c.do_while)
                };
                let (cont_t, cont_f) = (
                    falls_to_header(if_true.block, &tt),
                    falls_to_header(if_false.block, &ft),
                );
                let mut t = tc;
                t.extend(tt);
                let mut f = fc;
                f.extend(ft);
                if cont_t && !f.is_empty() && falls_through(&f) {
                    t.push(Stmt::Continue);
                } else if cont_f && !t.is_empty() && falls_through(&t) {
                    f.push(Stmt::Continue);
                }
                // A branch that doesn't fall through goes first (so the
                // other can follow the `if`), then a nested block, unless
                // only the other branch does anything.
                let swap = if t.is_empty() {
                    true
                } else if f.is_empty() {
                    false
                } else {
                    match (falls_through(&t), falls_through(&f)) {
                        (true, false) => true,
                        (false, true) => false,
                        _ => {
                            !self.nested.contains(&if_true.block)
                                && self.nested.contains(&if_false.block)
                        }
                    }
                };
                let (c, t, f) = if swap { (negate(c), f, t) } else { (c, t, f) };
                if t.is_empty() {
                    // Both branches go to `next`.
                } else if falls_through(&t) {
                    self.out.push(Stmt::If(c, t, f));
                } else {
                    // No `else` is needed.
                    self.out.push(Stmt::If(c, t, vec![]));
                    self.out.extend(f);
                }
            }
            Terminator::Select {
                value,
                targets: ts,
                default,
            } => {
                let idx = self.operand(site, TERM_COND, *value)?;
                let idx = self.simple(idx, QcType::Float);
                let mut copies = Vec::new();
                for (i, t) in ts.iter().chain([default]).enumerate() {
                    copies.push(self.edge(b, i, t)?);
                }
                let default_copies = copies.pop().expect("default edge");
                // Group the cases by edge (see `nested_blocks`); those
                // going where the default does are left to it.
                let mut groups: Vec<(Vec<f32>, usize)> = Vec::new();
                for (i, t) in ts.iter().enumerate() {
                    if same_edge(t, default) {
                        continue;
                    }
                    match groups.iter_mut().find(|(_, j)| same_edge(&ts[*j], t)) {
                        Some((cases, _)) => cases.push(i as f32),
                        None => groups.push((vec![i as f32], i)),
                    }
                }
                self.record(&Stmt::Switch(
                    idx.clone(),
                    groups
                        .iter()
                        .map(|(cases, i)| (cases.clone(), copies[*i].clone()))
                        .collect(),
                    default_copies.clone(),
                ));
                // A `break` in an arm leaves the `switch`, not a loop. (The
                // default may follow the `switch`, but is treated the same.)
                self.switch_depth += 1;
                let r = (|| {
                    let mut arms = Vec::new();
                    for (cases, i) in groups {
                        let mut body = std::mem::take(&mut copies[i]);
                        body.extend(self.tail(ts[i].block, next)?);
                        if falls_through(&body) {
                            body.push(Stmt::Break);
                        }
                        arms.push((cases, body));
                    }
                    let mut d = default_copies;
                    d.extend(self.tail(default.block, next)?);
                    Ok::<_, anyhow::Error>((arms, d))
                })();
                self.switch_depth -= 1;
                let (arms, d) = r?;
                if arms.is_empty() {
                    self.out.extend(d);
                } else if arms
                    .iter()
                    .any(|(_, body)| matches!(body.last(), Some(Stmt::Break)))
                {
                    self.out.push(Stmt::Switch(idx, arms, d));
                } else {
                    // No case leaves the `switch`, so the default can follow
                    // it.
                    self.out.push(Stmt::Switch(idx, arms, vec![]));
                    self.out.extend(d);
                }
            }
            Terminator::Return { values } => match values.as_slice() {
                [] => self.emit(Stmt::Return(None)),
                [x] => {
                    let e = self.operand(site, term_ret(0), *x)?;
                    let e = self.materialize(e);
                    self.emit(Stmt::Return(Some(e)));
                }
                _ => bail!("multi-value returns are not supported"),
            },
            Terminator::Unreachable | Terminator::None => {
                self.emit(Stmt::Comment("unreachable".into()));
                let v = match self.sigs[&self.inst.name].ret {
                    Ty::Known(QcType::Void) => None,
                    Ty::Known(QcType::Float) => Some(Expr::Float(0.0)),
                    Ty::Known(QcType::Vector) => Some(Expr::Vector([0.0; 3])),
                    _ => Some(Expr::Null),
                };
                self.emit(Stmt::Return(v));
            }
        }
        Ok(())
    }

    fn goto(&mut self, b: Block) -> Stmt {
        let l = label(b);
        self.gotos.insert(l.clone());
        Stmt::Goto(l)
    }

    /// What follows an edge's copies to go to `b`, when control otherwise
    /// continues at `next`: `b` itself if it's nested, else nothing if `b`
    /// is `next`, else a `goto` (or what `b` returns, if that's all it does).
    fn tail(&mut self, b: Block, next: Option<Block>) -> Result<Vec<Stmt>> {
        // A loop's follow is emitted after it, and reached by `break`.
        let follow = self.loop_ctx.iter().any(|c| c.follow == Some(b));
        if self.nested.contains(&b) && !follow {
            return self.nested_body(b, next);
        }
        if Some(b) == next {
            return Ok(vec![]);
        }
        // fteqcc has no labelled `continue` or `break`: they're only for
        // the innermost loop.
        if let Some(c) = self.loop_ctx.last() {
            if b == c.header && !c.do_while {
                return Ok(vec![Stmt::Continue]);
            }
            if Some(b) == c.follow && self.switch_depth == c.switch_depth {
                return Ok(vec![Stmt::Break]);
            }
        }
        if let Some(r) = self.trivial_return(b)? {
            return Ok(vec![r]);
        }
        Ok(vec![self.goto(b)])
    }

    /// The `return` of block `b`, if that's all it does: it has no
    /// parameters and returns nothing or a constant.
    fn trivial_return(&mut self, b: Block) -> Result<Option<Stmt>> {
        let body = self.body;
        let def = &body.blocks[b];
        let Terminator::Return { values } = &def.terminator else {
            return Ok(None);
        };
        let consts = def
            .insts
            .iter()
            .all(|&v| const_val(body, v).is_some() || const_expr(body, v).is_some());
        if !def.params.is_empty() || !consts {
            return Ok(None);
        }
        Ok(match values.as_slice() {
            [] => Some(Stmt::Return(None)),
            &[x] => {
                let x = body.resolve_alias(x);
                if const_val(body, x).is_none() && const_expr(body, x).is_none() {
                    return Ok(None);
                }
                match self.operand(Site::Term(b), term_ret(0), x)? {
                    e @ (Expr::Float(_)
                    | Expr::Null
                    | Expr::Vector(_)
                    | Expr::Str(_)
                    | Expr::Global(_)
                    | Expr::FuncName(_)) => Some(Stmt::Return(Some(e))),
                    _ => None,
                }
            }
            _ => None,
        })
    }

    /// The assignments of the arguments of branch `t` (the `i`th target of
    /// `from`'s terminator) to its block's parameters, as a parallel copy.
    fn edge(&mut self, from: Block, i: usize, t: &waffle::BlockTarget) -> Result<Vec<Stmt>> {
        let body = self.body;
        let params = &body.blocks[t.block].params;
        let mut copies: Vec<(String, Expr, QcType)> = Vec::new();
        for (a, (&arg, &(_, p))) in t.args.iter().zip(params).enumerate() {
            let e = self.operand(Site::Term(from), term_arg(i, a), arg)?;
            let Some(dest) = self.names.get(&p).cloned() else {
                continue; // Unused parameter.
            };
            if e == Expr::Local(dest.clone()) {
                continue;
            }
            let k = self.kind(p)?.qc();
            let saved = std::mem::take(&mut self.out);
            let e = self.materialize(e);
            let pre = std::mem::replace(&mut self.out, saved);
            ensure!(
                pre.is_empty() || k == QcType::Vector,
                "unexpected statements in an edge copy"
            );
            // Vectors are materialized into temporaries (`pre`) first.
            copies.push((dest, e, k));
            self.emit_all(pre);
        }
        // Record the parallel copy (see `alloc`).
        let group = copies
            .iter()
            .map(|(d, e, _)| {
                let plain = match e {
                    Expr::Local(x) => Some(x.clone()),
                    _ => None,
                };
                let mut reads = Vec::new();
                e.visit_locals(&mut |n| reads.push(n.to_string()));
                (d.clone(), plain, reads)
            })
            .collect();
        self.rec.entry(from).or_default().edges.push(group);
        // Sequentialize the parallel copy: a copy can go once no other
        // remaining copy reads its destination. On a cycle, one source is
        // saved in a temporary, and its destination assigned last.
        let mut stmts = Vec::new();
        let mut last = Vec::new();
        while !copies.is_empty() {
            let free = (0..copies.len()).find(|&i| {
                let d = &copies[i].0;
                copies
                    .iter()
                    .enumerate()
                    .all(|(j, (_, e, _))| i == j || !e.reads_local(d))
            });
            match free {
                Some(i) => {
                    let (d, e, _) = copies.remove(i);
                    stmts.push(Stmt::Assign(Expr::Local(d), e));
                }
                None => {
                    let (d, e, k) = copies.remove(0);
                    let tmp = self.new_temp(k);
                    stmts.push(Stmt::Assign(Expr::Local(tmp.clone()), e));
                    last.push(Stmt::Assign(Expr::Local(d), Expr::Local(tmp)));
                }
            }
        }
        stmts.extend(last);
        Ok(stmts)
    }

    /// Make `e` a plain local or global (spilling it if necessary), so that
    /// it can be used as the base of a component or field access.
    fn simple(&mut self, e: Expr, ty: QcType) -> Expr {
        match e {
            Expr::Local(_) | Expr::Global(_) => e,
            e => Expr::Local(self.simple_name(e, ty)),
        }
    }

    fn simple_name(&mut self, e: Expr, ty: QcType) -> String {
        if let Expr::Local(n) = &e {
            return n.clone();
        }
        let e = self.materialize(e);
        let tmp = self.new_temp(ty);
        self.emit(Stmt::Assign(Expr::Local(tmp.clone()), e));
        tmp
    }

    /// Turn a `MakeVector` into an expression QuakeC can represent: a
    /// literal, a whole vector global or field (if the components are its
    /// `_x`/`_y`/`_z`), or else a temporary assigned component-wise.
    fn materialize(&mut self, e: Expr) -> Expr {
        let Expr::MakeVector(parts) = e else { return e };
        if let [Expr::Float(x), Expr::Float(y), Expr::Float(z)] = *parts {
            return Expr::Vector([x, y, z]);
        }
        if let Some(whole) = whole_vector(self.ctx.defs, &parts) {
            return whole;
        }
        let tmp = self.new_temp(QcType::Vector);
        for (i, p) in parts.into_iter().enumerate() {
            self.emit(Stmt::Assign(
                Expr::Component(Box::new(Expr::Local(tmp.clone())), i as u8),
                p,
            ));
        }
        Expr::Local(tmp)
    }

    fn extract_lane(&mut self, v: Expr, idx: u8) -> Expr {
        match (v, idx) {
            // The fourth lane doesn't exist in QuakeC.
            (_, 3..) => Expr::Float(0.0),
            (Expr::Vector(c), i) => Expr::Float(c[i as usize]),
            (Expr::MakeVector(parts), i) => {
                let [x, y, z] = *parts;
                [x, y, z].into_iter().nth(i as usize).unwrap()
            }
            (Expr::Field(e, name), i) => {
                Expr::Field(e, format!("{name}_{}", ["x", "y", "z"][i as usize]))
            }
            (v, i) => Expr::Component(Box::new(self.simple(v, QcType::Vector)), i),
        }
    }

    /// The three components of a vector, evaluating it only once.
    fn vector_lanes(&mut self, v: Expr) -> [Expr; 3] {
        let v = match v {
            v @ (Expr::Vector(_) | Expr::MakeVector(_) | Expr::Local(_) | Expr::Global(_)) => v,
            v => {
                let v = self.materialize(v);
                self.simple(v, QcType::Vector)
            }
        };
        [0, 1, 2].map(|i| self.extract_lane(v.clone(), i))
    }

    /// A lane-wise comparison (`f32x4` or `i32x4`), `pmin` or `pmax`. LLVM
    /// vectorizes runs of component comparisons (like bounding box checks).
    /// Comparison results are masks: as in wasm, each lane is all ones (-1)
    /// or 0.
    fn lanewise(&mut self, op: &Operator, a: Expr, b: Expr) -> Expr {
        let (a, b) = (self.vector_lanes(a), self.vector_lanes(b));
        let lane = |x: Expr, y: Expr| {
            let sel =
                |c: Expr, t: Expr, f: Expr| Expr::Select(Box::new(c), Box::new(t), Box::new(f));
            // -1 or 0.
            let cmp = |op| fold(BinOp::Sub, Expr::Float(0.0), fold(op, x.clone(), y.clone()));
            match op {
                // `pmin(a, b)` is `b < a ? b : a`, `pmax(a, b)` is `a < b ? b : a`.
                Operator::F32x4PMin => sel(fold(BinOp::Lt, y.clone(), x.clone()), y, x),
                Operator::F32x4PMax => sel(fold(BinOp::Lt, x.clone(), y.clone()), y, x),
                Operator::F32x4Eq | Operator::I32x4Eq => cmp(BinOp::Eq),
                Operator::F32x4Ne | Operator::I32x4Ne => cmp(BinOp::Ne),
                Operator::F32x4Lt | Operator::I32x4LtS | Operator::I32x4LtU => cmp(BinOp::Lt),
                Operator::F32x4Gt | Operator::I32x4GtS | Operator::I32x4GtU => cmp(BinOp::Gt),
                Operator::F32x4Le | Operator::I32x4LeS | Operator::I32x4LeU => cmp(BinOp::Le),
                Operator::F32x4Ge | Operator::I32x4GeS | Operator::I32x4GeU => cmp(BinOp::Ge),
                _ => unreachable!("not a lane-wise operator"),
            }
        };
        let [a0, a1, a2] = a;
        let [b0, b1, b2] = b;
        Expr::MakeVector(Box::new([lane(a0, b0), lane(a1, b1), lane(a2, b2)]))
    }

    /// `f(a_i, b_i)` for each lane.
    fn lanes2(&mut self, a: Expr, b: Expr, f: impl Fn(Expr, Expr) -> Expr) -> Expr {
        let [a0, a1, a2] = self.vector_lanes(a);
        let [b0, b1, b2] = self.vector_lanes(b);
        Expr::MakeVector(Box::new([f(a0, b0), f(a1, b1), f(a2, b2)]))
    }

    /// `i8x16.shuffle` of two vectors. LLVM uses it to splat or rearrange
    /// lanes, so only shuffles that move whole `f32` lanes are supported.
    /// The result's fourth lane is ignored.
    fn shuffle(&mut self, a: Expr, b: Expr, indices: &[u8; 16]) -> Result<Expr> {
        let mut lanes = [0u8; 3];
        for (i, lane) in lanes.iter_mut().enumerate() {
            let g = &indices[i * 4..i * 4 + 4];
            ensure!(
                g[0].is_multiple_of(4) && g.iter().zip(g[0]..).all(|(&x, y)| x == y),
                "unsupported i8x16.shuffle {indices:?}: only whole f32 lanes can be moved"
            );
            *lane = g[0] / 4;
        }
        // Each source is evaluated once, and only if one of its lanes is used.
        let source = |this: &mut Self, v: Expr, used: bool| match v {
            _ if !used => None,
            v @ (Expr::Vector(_) | Expr::MakeVector(_) | Expr::Local(_) | Expr::Global(_)) => {
                Some(v)
            }
            v => {
                let v = this.materialize(v);
                Some(this.simple(v, QcType::Vector))
            }
        };
        let a = source(self, a, lanes.iter().any(|&l| l < 4));
        let b = source(self, b, lanes.iter().any(|&l| l >= 4));
        let parts = lanes.map(|l| {
            let (v, i) = if l < 4 { (&a, l) } else { (&b, l - 4) };
            self.extract_lane(v.clone().expect("source is used"), i)
        });
        Ok(Expr::MakeVector(Box::new(parts)))
    }

    fn replace_lane(&mut self, v: Expr, idx: u8, x: Expr) -> Expr {
        if idx >= 3 {
            return v;
        }
        let mut parts = match v {
            Expr::Vector([a, b, c]) => [Expr::Float(a), Expr::Float(b), Expr::Float(c)],
            Expr::MakeVector(parts) => *parts,
            v => {
                // Split an existing vector into its components.
                let v = self.simple(v, QcType::Vector);
                [0, 1, 2].map(|i| Expr::Component(Box::new(v.clone()), i))
            }
        };
        parts[idx as usize] = x;
        Expr::MakeVector(Box::new(parts))
    }

    /// `a * b` or `a / b` on vectors. QuakeC can only scale a vector by a
    /// float, so multiplying by a splat becomes `v * f`; anything else is done
    /// component by component.
    fn vector_mul_div(&mut self, div: bool, a: Expr, b: Expr) -> Expr {
        let splat_of = |e: &Expr| match e {
            Expr::Vector([x, y, z]) if x == y && y == z => Some(Expr::Float(*x)),
            Expr::MakeVector(p) if p[0] == p[1] && p[1] == p[2] => Some(p[0].clone()),
            _ => None,
        };
        if let Some(f) = splat_of(&b) {
            let a = self.materialize(a);
            let f = if div {
                fold(BinOp::Div, Expr::Float(1.0), f)
            } else {
                f
            };
            return Expr::Bin(BinOp::Mul, Box::new(a), Box::new(f));
        }
        if !div && let Some(f) = splat_of(&a) {
            let b = self.materialize(b);
            return Expr::Bin(BinOp::Mul, Box::new(b), Box::new(f));
        }
        let (a, b) = (self.vector_lanes(a), self.vector_lanes(b));
        let op = if div { BinOp::Div } else { BinOp::Mul };
        let [a0, a1, a2] = a;
        let [b0, b1, b2] = b;
        Expr::MakeVector(Box::new([
            fold(op, a0, b0),
            fold(op, a1, b1),
            fold(op, a2, b2),
        ]))
    }

    fn splat(&mut self, x: Expr) -> Expr {
        if let Expr::Float(f) = x {
            return Expr::Vector([f; 3]);
        }
        let x = self.simple(x, QcType::Float);
        Expr::MakeVector(Box::new([x.clone(), x.clone(), x]))
    }
}

/// The expression for a float or vector constant, which are used inline
/// wherever they're needed.
fn const_expr(body: &FunctionBody, v: Value) -> Option<Result<Expr>> {
    let ValueDef::Operator(op, _, _) = &body.values[body.resolve_alias(v)] else {
        return None;
    };
    Some(match op {
        Operator::F32Const { value } => {
            let f = f32::from_bits(*value);
            if f.is_finite() {
                Ok(Expr::Float(f))
            } else {
                Err(anyhow!("non-finite float constant {f}"))
            }
        }
        Operator::V128Const { value } => {
            let bits = |i: u32| (value >> (32 * i)) as u32;
            // Small integers are subnormal floats (or NaNs, if negative),
            // which no float constant would use: these are integer lanes.
            let int = (0..4).all(|i| {
                let (b, f) = (bits(i), f32::from_bits(bits(i)));
                b == 0 || f.is_subnormal() || (f.is_nan() && (b as i32).unsigned_abs() <= 1 << 24)
            });
            let lane = |i: u32| {
                if int {
                    bits(i) as i32 as f32
                } else {
                    f32::from_bits(bits(i))
                }
            };
            let c = [lane(0), lane(1), lane(2)];
            if c.iter().all(|f| f.is_finite()) {
                Ok(Expr::Vector(c))
            } else {
                Err(anyhow!("non-finite vector constant {c:?}"))
            }
        }
        _ => return None,
    })
}

/// A bit test `v`: `(1 << x) & mask` or `(mask >> x) & 1`, for a constant
/// `mask`. Returns the shift, `x`, `mask` and whether it's the first form.
fn bit_test(body: &FunctionBody, v: Value) -> Option<(Value, Value, u32, bool)> {
    let ValueDef::Operator(Operator::I32And, _, _) = &body.values[v] else {
        return None;
    };
    let a = args(body, v);
    let (s, k) = match (const_val(body, a[0]), const_val(body, a[1])) {
        (None, Some(k)) => (a[0], k),
        (Some(k), None) => (a[1], k),
        _ => return None,
    };
    let ValueDef::Operator(op @ (Operator::I32Shl | Operator::I32ShrU), _, _) = &body.values[s]
    else {
        return None;
    };
    let sa = args(body, s);
    if const_val(body, sa[1]).is_some() {
        return None;
    }
    let c = const_val(body, sa[0])? as u32;
    match op {
        Operator::I32Shl if c == 1 => Some((s, sa[1], k as u32, true)),
        Operator::I32ShrU if k == 1 => Some((s, sa[1], c, false)),
        _ => None,
    }
}

/// Whether the `i32` `v` is known to be non-negative (as a signed integer),
/// given the block parameters `params` that are. Like the rest of the
/// converter, this assumes adding and multiplying don't wrap.
fn nonneg(body: &FunctionBody, params: &HashSet<Value>, v: Value) -> bool {
    use Operator as O;
    let v = body.resolve_alias(v);
    if let Some(c) = const_val(body, v) {
        return c as i32 >= 0;
    }
    let op = match &body.values[v] {
        ValueDef::BlockParam(..) => return params.contains(&v),
        ValueDef::Operator(op, _, _) => op,
        _ => return false,
    };
    let all = || args(body, v).iter().all(|&a| nonneg(body, params, a));
    match op {
        O::I32TruncF32U
        | O::I32TruncSatF32U
        | O::I32Load8U { .. }
        | O::I32Load16U { .. }
        | O::I32Eqz
        | O::I32Eq
        | O::I32Ne
        | O::I32LtS
        | O::I32LtU
        | O::I32GtS
        | O::I32GtU
        | O::I32LeS
        | O::I32LeU
        | O::I32GeS
        | O::I32GeU
        | O::F32Eq
        | O::F32Ne
        | O::F32Lt
        | O::F32Gt
        | O::F32Le
        | O::F32Ge => true,
        O::I32And => args(body, v).iter().any(|&a| nonneg(body, params, a)),
        O::I32ShrU => const_val(body, args(body, v)[1]).is_some_and(|k| k % 32 != 0),
        O::I32Add | O::I32Mul | O::I32Or | O::I32DivU | O::I32RemU => all(),
        // The condition doesn't matter.
        O::Select => args(body, v)[..2].iter().all(|&a| nonneg(body, params, a)),
        _ => false,
    }
}

/// `d` if `y` is `x + d` for a constant `d`.
fn sub_const(body: &FunctionBody, y: Value, x: Value) -> Option<i64> {
    let (x, y) = (body.resolve_alias(x), body.resolve_alias(y));
    let ValueDef::Operator(op, _, _) = &body.values[y] else {
        return None;
    };
    let a = args(body, y);
    let same = |v: Value| body.resolve_alias(v) == x;
    let k = |v: Value| const_val(body, v).map(|k| k as i32 as i64);
    match op {
        Operator::I32Add if same(a[0]) => k(a[1]),
        Operator::I32Add if same(a[1]) => k(a[0]),
        Operator::I32Sub if same(a[0]) => k(a[1]).map(|k| -k),
        _ => None,
    }
}

/// The block parameters that are non-negative: those (other than the
/// function's parameters) whose every incoming value is. Found
/// optimistically, so a loop counter that starts at 0 and counts up is.
fn nonneg_params(body: &FunctionBody) -> HashSet<Value> {
    let mut edges: Vec<(Value, Value)> = Vec::new();
    for (_, def) in body.blocks.entries() {
        def.terminator.visit_targets(|t| {
            for (&a, &(_, p)) in t.args.iter().zip(&body.blocks[t.block].params) {
                edges.push((p, a));
            }
        });
    }
    let mut set: HashSet<Value> = body
        .blocks
        .entries()
        .filter(|&(b, _)| b != body.entry)
        .flat_map(|(_, def)| &def.params)
        .filter(|&&(ty, _)| ty == Type::I32)
        .map(|&(_, p)| p)
        .collect();
    loop {
        let bad: Vec<Value> = edges
            .iter()
            .filter(|&&(p, a)| set.contains(&p) && !nonneg(body, &set, a))
            .map(|&(p, _)| p)
            .collect();
        if bad.is_empty() {
            return set;
        }
        for p in bad {
            set.remove(&p);
        }
    }
}

/// Whether `v` is a mask: each lane all ones or 0.
fn is_mask(body: &FunctionBody, v: Value) -> bool {
    use Operator as O;
    let v = body.resolve_alias(v);
    let ValueDef::Operator(op, _, _) = &body.values[v] else {
        return false;
    };
    match op {
        O::F32x4Eq
        | O::F32x4Ne
        | O::F32x4Lt
        | O::F32x4Gt
        | O::F32x4Le
        | O::F32x4Ge
        | O::I32x4Eq
        | O::I32x4Ne
        | O::I32x4LtS
        | O::I32x4LtU
        | O::I32x4GtS
        | O::I32x4GtU
        | O::I32x4LeS
        | O::I32x4LeU
        | O::I32x4GeS
        | O::I32x4GeU => true,
        O::V128Not => is_mask(body, args(body, v)[0]),
        O::V128And | O::V128Or | O::V128AndNot => {
            let a = args(body, v);
            is_mask(body, a[0]) && is_mask(body, a[1])
        }
        _ => false,
    }
}

/// `a op b` on whole vectors, folding constants.
fn fold_vector(op: BinOp, a: Expr, b: Expr) -> Expr {
    match (&a, &b) {
        (Expr::Vector(x), Expr::Vector(y)) => {
            let f = |i: usize| match op {
                BinOp::Add => x[i] + y[i],
                _ => x[i] - y[i],
            };
            Expr::Vector([f(0), f(1), f(2)])
        }
        _ => Expr::Bin(op, Box::new(a), Box::new(b)),
    }
}

/// `e.f_x, e.f_y, e.f_z` as a vector is `e.f`.
fn whole_field(v: Expr) -> Expr {
    if let Expr::MakeVector(parts) = &v
        && let [Expr::Field(a, x), Expr::Field(b, y), Expr::Field(c, z)] = &**parts
        && a == b
        && b == c
        && let (Some(f), Some(g), Some(h)) = (
            x.strip_suffix("_x"),
            y.strip_suffix("_y"),
            z.strip_suffix("_z"),
        )
        && f == g
        && g == h
    {
        return Expr::Field(a.clone(), f.to_string());
    }
    v
}

/// Remove `x = x` assignments (left by sharing locals), and `if`s left with
/// nothing to do in their `then` branch.
fn remove_self_copies(stmts: &mut Vec<Stmt>) {
    stmts.retain(|s| !matches!(s, Stmt::Assign(Expr::Local(a), Expr::Local(b)) if a == b));
    for s in stmts.iter_mut() {
        match s {
            Stmt::If(c, t, e) => {
                remove_self_copies(t);
                remove_self_copies(e);
                if t.is_empty() && !e.is_empty() {
                    let c = std::mem::replace(c, Expr::Null);
                    *s = Stmt::If(negate(c), std::mem::take(e), vec![]);
                }
            }
            Stmt::Switch(_, arms, d) => {
                for (_, body) in arms {
                    remove_self_copies(body);
                }
                remove_self_copies(d);
            }
            Stmt::While(_, body) | Stmt::DoWhile(body, _) => remove_self_copies(body),
            _ => {}
        }
    }
    // Conditions are pure.
    stmts.retain(|s| !matches!(s, Stmt::If(_, t, e) if t.is_empty() && e.is_empty()));
}

fn label(b: Block) -> String {
    format!("b{}", b.index())
}

fn not(e: Expr) -> Expr {
    Expr::Un(UnOp::Not, Box::new(e))
}

/// The negation of a condition (only its truth matters).
fn negate(e: Expr) -> Expr {
    match e {
        Expr::Un(UnOp::Not, x) => *x,
        e => not(e),
    }
}

/// Whether control can reach the end of `stmts`, which were built for one
/// terminator (so they contain no labels, and a `break` only at the end of
/// a `switch` arm).
fn falls_through(stmts: &[Stmt]) -> bool {
    match stmts.last() {
        None => true,
        Some(Stmt::Return(_) | Stmt::Goto(_) | Stmt::Continue | Stmt::Break) => false,
        Some(Stmt::While(Expr::Float(c), body)) if *c != 0.0 => breaks_out(body),
        Some(Stmt::If(_, t, e)) => e.is_empty() || falls_through(t) || falls_through(e),
        Some(Stmt::Switch(_, arms, d)) => {
            d.is_empty()
                || falls_through(d)
                || arms
                    .iter()
                    .any(|(_, body)| matches!(body.last(), Some(Stmt::Break)))
        }
        Some(_) => true,
    }
}

/// Whether `stmts` contain a `break` out of the loop they're the body of.
fn breaks_out(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        Stmt::Break => true,
        Stmt::If(_, t, e) => breaks_out(t) || breaks_out(e),
        _ => false,
    })
}

/// Keep only the labels in `gotos`.
fn drop_labels(stmts: &mut Vec<Stmt>, gotos: &HashSet<String>) {
    stmts.retain(|s| !matches!(s, Stmt::Label(l) if !gotos.contains(l)));
    for s in stmts {
        match s {
            Stmt::If(_, t, e) => {
                drop_labels(t, gotos);
                drop_labels(e, gotos);
            }
            Stmt::Switch(_, arms, d) => {
                for (_, body) in arms {
                    drop_labels(body, gotos);
                }
                drop_labels(d, gotos);
            }
            Stmt::While(_, body) | Stmt::DoWhile(body, _) => drop_labels(body, gotos),
            _ => {}
        }
    }
}

/// Whether two branch targets are the same edge: they go to the same block
/// with the same arguments.
fn same_edge(a: &waffle::BlockTarget, b: &waffle::BlockTarget) -> bool {
    a.block == b.block && a.args == b.args
}

/// The blocks to emit inside their predecessor's terminator: those with a
/// single incoming edge (counting a `br_table`'s identical edges, which
/// become one `switch` arm, once), other than the entry and self-loops. The
/// predecessor dominates such a block, so it's not a loop header.
fn nested_blocks(body: &FunctionBody, order: &[Block]) -> HashSet<Block> {
    let mut preds: HashMap<Block, (usize, Block)> = HashMap::new();
    for &b in order {
        let ts = targets(&body.blocks[b].terminator);
        let select = matches!(body.blocks[b].terminator, Terminator::Select { .. });
        for (i, t) in ts.iter().enumerate() {
            if select && ts[..i].iter().any(|u| same_edge(u, t)) {
                continue;
            }
            preds.entry(t.block).or_insert((0, b)).0 += 1;
        }
    }
    preds
        .into_iter()
        .filter(|&(b, (n, p))| n == 1 && p != b && b != body.entry)
        .map(|(b, _)| b)
        .collect()
}

/// The blocks reachable from the entry, in reverse postorder.
fn rpo(body: &FunctionBody) -> Vec<Block> {
    let mut seen = HashSet::new();
    let mut post = Vec::new();
    // Iterative DFS: (block, next successor index).
    let mut stack = vec![(body.entry, 0usize)];
    seen.insert(body.entry);
    while let Some((b, i)) = stack.pop() {
        let succs: Vec<Block> = targets(&body.blocks[b].terminator)
            .iter()
            .map(|t| t.block)
            .collect();
        if let Some(&s) = succs.get(i) {
            stack.push((b, i + 1));
            if seen.insert(s) {
                stack.push((s, 0));
            }
        } else {
            post.push(b);
        }
    }
    post.reverse();
    post
}

/// The constants `v + offset` may be: through `select`s, block parameters
/// and constant offsets. `false` if it may be anything else.
fn const_set(
    body: &FunctionBody,
    v: Value,
    offset: i64,
    seen: &mut HashSet<Value>,
    out: &mut Vec<i64>,
) -> bool {
    let v = body.resolve_alias(v);
    if let Some(c) = const_val(body, v) {
        out.push(c + offset);
        return true;
    }
    if !seen.insert(v) {
        return true;
    }
    match &body.values[v] {
        ValueDef::Operator(Operator::Select | Operator::TypedSelect { .. }, _, _) => {
            let a = args(body, v);
            const_set(body, a[0], offset, seen, out) && const_set(body, a[1], offset, seen, out)
        }
        ValueDef::Operator(Operator::I32Add, _, _) => {
            let a = args(body, v);
            match (const_val(body, a[0]), const_val(body, a[1])) {
                (Some(k), None) => const_set(body, a[1], offset + k, seen, out),
                (None, Some(k)) => const_set(body, a[0], offset + k, seen, out),
                _ => false,
            }
        }
        ValueDef::BlockParam(b, _, _) if *b == body.entry => false,
        ValueDef::BlockParam(b, i, _) => {
            let mut ok = true;
            for (_, def) in body.blocks.entries() {
                for t in targets(&def.terminator) {
                    if t.block == *b {
                        ok &= const_set(body, t.args[*i as usize], offset, seen, out);
                    }
                }
            }
            ok
        }
        _ => false,
    }
}

/// `arms[i]` where `v == choices[i]` (`v` must be one of `choices`): a chain
/// of `select`s comparing `v` with each choice but the last.
fn select_chain(v: &Expr, choices: &[f32], mut arms: Vec<Expr>) -> Expr {
    let mut e = arms.pop().expect("at least one choice");
    let rest = &choices[..arms.len()];
    for (&c, arm) in rest.iter().zip(arms).rev() {
        let is_c = Expr::Bin(BinOp::Eq, Box::new(v.clone()), Box::new(Expr::Float(c)));
        e = Expr::Select(Box::new(is_c), Box::new(arm), Box::new(e));
    }
    e
}

fn binop(op: &Operator, a: Expr, b: Expr) -> Result<Expr> {
    use Operator as O;
    let qop = match op {
        // Integers are represented as floats; wrapping is not emulated.
        O::I32Add | O::F32Add => BinOp::Add,
        O::I32Sub | O::F32Sub => BinOp::Sub,
        O::I32Mul | O::F32Mul => BinOp::Mul,
        O::F32Div => BinOp::Div,
        O::F32Eq => BinOp::Eq,
        O::F32Ne => BinOp::Ne,
        O::I32LtS | O::I32LtU | O::F32Lt => BinOp::Lt,
        O::I32GtS | O::I32GtU | O::F32Gt => BinOp::Gt,
        O::I32LeS | O::I32LeU | O::F32Le => BinOp::Le,
        O::I32GeS | O::I32GeU | O::F32Ge => BinOp::Ge,
        O::I32And => BinOp::BitAnd,
        O::I32Or => BinOp::BitOr,
        // `x << k` for a constant `k` is `x * 2^k`.
        O::I32Shl => match b {
            Expr::Float(k) if k.fract() == 0.0 && (0.0..32.0).contains(&k) => {
                return Ok(fold(BinOp::Mul, a, Expr::Float((1u32 << k as u32) as f32)));
            }
            _ => bail!("unsupported shift by a non-constant amount"),
        },
        other => bail!("unsupported operator {other:?}"),
    };
    Ok(fold(qop, a, b))
}

/// Build `a op b`, folding it if both operands are constants (as QuakeC
/// would compute it at runtime).
///
/// Arithmetic with a constant and a `select` between constants is pushed
/// into the `select`, so that `base + (c ? 5 : 9)` (which LLVM produces when
/// it merges two calls taking different addresses) becomes
/// `c ? base + 5 : base + 9`, a `select` between constant addresses.
fn fold(op: BinOp, a: Expr, b: Expr) -> Expr {
    let arith = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul);
    let consts = |x: &Expr, y: &Expr| matches!((x, y), (Expr::Float(_), Expr::Float(_)));
    match (a, b) {
        (Expr::Select(c, x, y), k @ Expr::Float(_)) if arith && consts(&x, &y) => Expr::Select(
            c,
            Box::new(fold_consts(op, *x, k.clone())),
            Box::new(fold_consts(op, *y, k)),
        ),
        (k @ Expr::Float(_), Expr::Select(c, x, y)) if arith && consts(&x, &y) => Expr::Select(
            c,
            Box::new(fold_consts(op, k.clone(), *x)),
            Box::new(fold_consts(op, k, *y)),
        ),
        (a, b) => fold_consts(op, a, b),
    }
}

/// [`fold`] for two constant operands; anything else is left as is.
fn fold_consts(op: BinOp, a: Expr, b: Expr) -> Expr {
    let (Expr::Float(x), Expr::Float(y)) = (&a, &b) else {
        return Expr::Bin(op, Box::new(a), Box::new(b));
    };
    let (x, y) = (*x, *y);
    let bool = |c: bool| if c { 1.0 } else { 0.0 };
    Expr::Float(match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x * y,
        // Don't fold division (e.g. by zero); leave it to the VM.
        BinOp::Div => return Expr::Bin(op, Box::new(a), Box::new(b)),
        BinOp::Eq => bool(x == y),
        BinOp::Ne => bool(x != y),
        BinOp::Lt => bool(x < y),
        BinOp::Gt => bool(x > y),
        BinOp::Le => bool(x <= y),
        BinOp::Ge => bool(x >= y),
        BinOp::BitAnd => ((x as i32) & (y as i32)) as f32,
        BinOp::BitOr => ((x as i32) | (y as i32)) as f32,
    })
}

/// If `parts` are exactly the `_x`, `_y` and `_z` components of one vector
/// global or vector field, return that whole vector.
fn whole_vector(defs: &crate::sysdefs::SysDefs, parts: &[Expr; 3]) -> Option<Expr> {
    let base = |e: &Expr, suffix: &str| -> Option<(Option<Expr>, String)> {
        match e {
            Expr::Global(n) => Some((None, n.strip_suffix(suffix)?.to_string())),
            Expr::Field(ent, n) => {
                Some((Some((**ent).clone()), n.strip_suffix(suffix)?.to_string()))
            }
            _ => None,
        }
    };
    let (ent, name) = base(&parts[0], "_x")?;
    let same = |p: &Expr, suffix| {
        base(p, suffix).is_some_and(|(e, n)| {
            n == name
                && match (&e, &ent) {
                    (None, None) => true,
                    (Some(Expr::Local(a)), Some(Expr::Local(b)))
                    | (Some(Expr::Global(a)), Some(Expr::Global(b))) => a == b,
                    _ => false,
                }
        })
    };
    if !(same(&parts[1], "_y") && same(&parts[2], "_z")) {
        return None;
    }
    match ent {
        None if defs.is_vector_global(&name) => Some(Expr::Global(name)),
        Some(ent) if defs.is_vector_field(&name) => Some(Expr::Field(Box::new(ent), name)),
        _ => None,
    }
}
