//! Lowering of a single wasm function to a QuakeC function.
//!
//! The wasm operand stack is simulated symbolically: each entry is a QuakeC
//! expression. Entries are "spilled" into temporaries whenever something
//! could change the value they would compute (a call, store, local write, or
//! control flow). Calls are always assigned to a temporary immediately, since
//! QuakeC return values live in `OFS_RETURN` and are clobbered by later calls.

use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use walrus::ir::{
    self, BinaryOp, Instr, InstrSeqId, InstrSeqType, LoadKind, StoreKind, UnaryOp, Value,
};
use walrus::{FunctionId, LocalFunction, LocalId};

use crate::data::{DATA_BASE, DataKind, StaticData};
use crate::params::{Len, ParamRepr};
use crate::qc::{BinOp, Expr, Function, QcType, Sig, Stmt, Ty, UnOp};
use crate::translate::{Ctx, FuncKind, Intrinsic, TABLE_BASE};

/// Largest integer magnitude exactly representable as an f32.
const MAX_EXACT_INT: i32 = 1 << 24;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LabelKind {
    /// Branching to the function body returns.
    Return,
    /// Branching to a block/if jumps to its end.
    End,
    /// Branching to a loop jumps to its start.
    Loop,
}

struct Label {
    name: String,
    kind: LabelKind,
    /// Temporary receiving the block's result, for `End` labels.
    result: Option<(String, Ty)>,
    used: bool,
    /// The merged [`Lower::known`] of every branch to an `End` label so far.
    incoming: Option<Known>,
}

/// For each local (by QuakeC name), the constants it may hold on every path
/// reaching this point of the lowering, sorted. Locals not in the map may hold
/// anything.
type Known = HashMap<String, Vec<f32>>;

/// Sets larger than this are forgotten.
const MAX_KNOWN: usize = 16;

struct Lower<'a, 'm> {
    ctx: &'a mut Ctx<'m>,
    func: &'a LocalFunction,
    locals: HashMap<LocalId, (String, Ty)>,
    local_decls: Vec<(String, Ty)>,
    temps: Vec<(String, Ty)>,
    next_temp: usize,
    next_label: usize,
    stack: Vec<(Expr, Ty)>,
    /// Compile-time contents of `__funcref_call_table`.
    table_slots: HashMap<i32, (Expr, Ty)>,
    labels: Vec<Label>,
    label_of: HashMap<InstrSeqId, usize>,
    ret: Ty,
    out: Vec<Stmt>,
    /// The most recent statement assigned a call result to this temporary.
    last_call_temp: Option<String>,
    /// Locals that only ever hold `__memory_base` or `__table_base`, with
    /// that base's value (see [`base_locals`]).
    base_locals: HashMap<LocalId, f32>,
    /// Locals known to hold one of a few constants at this point of the
    /// lowering. Reads of a local with just one become that constant, so that
    /// addresses computed through locals stay constant; with several, uses
    /// that need a constant pick between them (see [`Self::choose`]).
    known: Known,
    /// Parameters retyped as functions or strings (see `params.rs`), by name.
    retyped: HashSet<String>,
    /// Locals that may alias a retyped parameter (see `params.rs`), and
    /// those that do, with the expression they hold.
    may_alias: HashSet<LocalId>,
    aliases: HashMap<LocalId, Expr>,
}

pub fn lower_function(ctx: &mut Ctx<'_>, id: FunctionId, func: &LocalFunction) -> Result<Function> {
    let info = &ctx.funcs[&id];
    let name = info.name.clone();
    let sig = info.sig.clone();

    let mut locals = HashMap::new();
    let mut params = Vec::new();
    for (i, (&arg, &ty)) in func.args.iter().zip(&sig.params).enumerate() {
        let n = format!("p{i}");
        locals.insert(arg, (n.clone(), ty));
        params.push((n, ty));
    }

    let bases = [
        (ctx.memory_base, DATA_BASE as f32),
        (ctx.table_base, TABLE_BASE as f32),
    ];
    let base_locals = base_locals(func, &bases, &locals);
    let retyped = match ctx.param_reprs.get(&id) {
        Some(reprs) => (params.iter().zip(reprs))
            .filter(|(_, r)| r.is_some())
            .map(|((n, _), _)| n.clone())
            .collect(),
        None => HashSet::new(),
    };
    let may_alias = ctx.param_aliases.get(&id).cloned().unwrap_or_default();
    let mut l = Lower {
        ctx,
        func,
        locals,
        local_decls: Vec::new(),
        temps: Vec::new(),
        next_temp: 0,
        next_label: 0,
        stack: Vec::new(),
        table_slots: HashMap::new(),
        labels: Vec::new(),
        label_of: HashMap::new(),
        ret: sig.ret,
        out: Vec::new(),
        last_call_temp: None,
        base_locals,
        known: HashMap::new(),
        retyped,
        may_alias,
        aliases: HashMap::new(),
    };

    let entry = func.entry_block();
    l.labels.push(Label {
        name: String::new(),
        kind: LabelKind::Return,
        result: None,
        used: false,
        incoming: None,
    });
    l.label_of.insert(entry, 0);
    let live = l.lower_seq(entry)?;
    if live && l.ret != Ty::Known(QcType::Void) {
        let (v, t) = l.pop()?;
        l.unify(l.ret, t, "return value");
        l.emit(Stmt::Return(Some(v)));
    }

    // Wasm locals start at zero, but QuakeC locals live in the globals area
    // and (in vanilla engines) keep their value from the previous call.
    let mut body: Vec<Stmt> = l
        .local_decls
        .iter()
        .map(|(n, t)| {
            let zero = match t {
                Ty::Known(QcType::Float) => Expr::Float(0.0),
                Ty::Known(QcType::Vector) => Expr::Vector([0.0; 3]),
                _ => Expr::Null,
            };
            Stmt::Assign(Expr::Local(n.clone()), zero)
        })
        .collect();
    body.append(&mut l.out);
    l.out = body;

    let mut all_locals = l.local_decls;
    all_locals.extend(l.temps);
    Ok(Function {
        name,
        params,
        ret: sig.ret,
        locals: all_locals,
        body: l.out,
    })
}

impl Lower<'_, '_> {
    /// The value computed by `arm` from whichever constant the local `n`
    /// holds: a function pointer, string literal address or global address
    /// that LLVM computed on different paths and merged. `n` must be known
    /// to hold one of several constants here; the result is a chain of
    /// `select`s comparing it with each.
    fn choose(
        &mut self,
        n: &str,
        what: &str,
        mut arm: impl FnMut(&mut Self, f32) -> Result<Expr>,
    ) -> Result<Expr> {
        let Some(cs) = self.known.get(n).cloned() else {
            bail!("{what} in local `{n}` is not a constant on every path here");
        };
        // Constants that aren't valid here (e.g. a zero that LLVM never uses
        // on this path) are dropped; at least one must be valid.
        let (mut good, mut arms, mut first_err) = (Vec::new(), Vec::new(), None);
        for c in cs {
            match arm(self, c) {
                Ok(a) => {
                    good.push(c);
                    arms.push(a);
                }
                Err(err) => {
                    first_err.get_or_insert(err);
                }
            }
        }
        if arms.is_empty() {
            let err = first_err.expect("known sets are never empty");
            return Err(err.context(format!("no value of local `{n}` is a valid {what}")));
        }
        Ok(select_chain(&Expr::Local(n.to_string()), &good, arms))
    }

    /// The constants `v` may evaluate to, if it is one of a few constants.
    fn consts_of(&self, v: &Expr) -> Option<Vec<f32>> {
        match v {
            Expr::Float(c) => Some(vec![*c]),
            Expr::Select(_, a, b) => {
                let (Expr::Float(a), Expr::Float(b)) = (&**a, &**b) else {
                    return None;
                };
                Some(const_set([*a, *b]))
            }
            Expr::Local(m) => self.known.get(m).cloned(),
            _ => None,
        }
    }

    /// Record that the local `name` now holds `v`.
    fn note_assign(&mut self, name: &str, v: &Expr) {
        match self.consts_of(v) {
            Some(cs) => self.known.insert(name.to_string(), cs),
            None => self.known.remove(name),
        };
    }

    /// The constant the local `n` holds, if it is known to be just one.
    fn single_const(&self, n: &str) -> Option<f32> {
        match self.known.get(n).map(Vec::as_slice) {
            Some(&[c]) => Some(c),
            _ => None,
        }
    }

    /// Merge the state at the end of a block or `if`: the fallthrough state
    /// (if control can fall off the end) and those of branches to its label.
    fn merge_at_end(&mut self, fallthrough: Option<Known>, idx: usize) {
        let incoming = self.labels[idx].incoming.take();
        self.known = match (fallthrough, incoming) {
            (Some(a), Some(b)) => meet(&a, &b),
            (Some(k), None) | (None, Some(k)) => k,
            // Unreachable from here on.
            (None, None) => Known::new(),
        };
    }

    fn emit(&mut self, s: Stmt) {
        self.last_call_temp = None;
        match &s {
            Stmt::Assign(Expr::Local(n), v) => self.note_assign(n, v),
            Stmt::Assign(Expr::Component(l, _), _) => {
                if let Expr::Local(n) = &**l {
                    self.known.remove(n);
                }
            }
            _ => {}
        }
        self.out.push(s);
    }

    fn unify(&mut self, a: Ty, b: Ty, what: &str) {
        self.ctx.tv.unify(a, b, what);
    }

    /// Pop a value, turning any `MakeVector` into a real expression.
    fn pop(&mut self) -> Result<(Expr, Ty)> {
        let (e, t) = self.pop_raw()?;
        Ok((self.materialize(e), t))
    }

    /// Pop a value without materializing it (for lane operations).
    fn pop_raw(&mut self) -> Result<(Expr, Ty)> {
        self.stack
            .pop()
            .ok_or_else(|| anyhow!("wasm operand stack underflow"))
    }

    fn pop_n(&mut self, n: usize) -> Result<Vec<(Expr, Ty)>> {
        ensure!(self.stack.len() >= n, "wasm operand stack underflow");
        let vals = self.stack.split_off(self.stack.len() - n);
        Ok(vals
            .into_iter()
            .map(|(e, t)| (self.materialize(e), t))
            .collect())
    }

    /// Materialize the value on top of the stack in place.
    fn materialize_top(&mut self) {
        if let Some((e @ Expr::MakeVector(_), _)) = self.stack.last_mut() {
            let e = std::mem::replace(e, Expr::Null);
            let e = self.materialize(e);
            self.stack.last_mut().unwrap().0 = e;
        }
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
        let tmp = self.new_temp(Ty::Known(QcType::Vector));
        for (i, p) in parts.into_iter().enumerate() {
            self.emit(Stmt::Assign(
                Expr::Component(Box::new(Expr::Local(tmp.clone())), i as u8),
                p,
            ));
        }
        Expr::Local(tmp)
    }

    fn new_temp(&mut self, ty: Ty) -> String {
        let n = format!("t{}", self.next_temp);
        self.next_temp += 1;
        self.temps.push((n.clone(), ty));
        n
    }

    fn new_label(&mut self, prefix: &str) -> String {
        let n = format!("{prefix}{}", self.next_label);
        self.next_label += 1;
        n
    }

    /// Push a read of the local `n`: its constant, if it is known to hold
    /// just one.
    fn push_local(&mut self, n: String, t: Ty) {
        let e = match self.single_const(&n) {
            Some(c) => Expr::Float(c),
            None => Expr::Local(n),
        };
        self.stack.push((e, t));
    }

    /// Push the result of a block or `if`, if it has one.
    fn push_result(&mut self, result: Option<(String, Ty)>) {
        if let Some((tmp, ty)) = result {
            self.push_local(tmp, ty);
        }
    }

    /// Forget the constants of locals assigned anywhere in `seqs`.
    fn forget_assigned_in(&mut self, seqs: &[InstrSeqId]) {
        if self.known.is_empty() {
            return;
        }
        let mut todo = seqs.to_vec();
        while let Some(seq) = todo.pop() {
            for (instr, _) in &self.func.block(seq).instrs {
                match instr {
                    Instr::LocalSet(ir::LocalSet { local })
                    | Instr::LocalTee(ir::LocalTee { local }) => {
                        if let Some((n, _)) = self.locals.get(local) {
                            self.known.remove(n);
                        }
                    }
                    Instr::Block(ir::Block { seq }) | Instr::Loop(ir::Loop { seq }) => {
                        todo.push(*seq)
                    }
                    Instr::IfElse(ir::IfElse {
                        consequent,
                        alternative,
                    }) => todo.extend([*consequent, *alternative]),
                    _ => {}
                }
            }
        }
    }

    fn local(&mut self, id: LocalId) -> Result<(String, Ty)> {
        if let Some(l) = self.locals.get(&id) {
            return Ok(l.clone());
        }
        let wasm_ty = self.ctx.module.locals.get(id).ty();
        let ty = self.ctx.val_ty(wasm_ty)?;
        let n = format!("l{}", self.local_decls.len());
        self.locals.insert(id, (n.clone(), ty));
        self.local_decls.push((n.clone(), ty));
        Ok((n, ty))
    }

    /// Spill every stack entry (and, if `tables`, call-table slot) matching
    /// `pred` into a temporary.
    fn spill_with(&mut self, tables: bool, pred: impl Fn(&Expr) -> bool) {
        for i in 0..self.stack.len() {
            if pred(&self.stack[i].0) {
                let (e, t) = self.stack[i].clone();
                let e = self.materialize(e);
                if !pred(&e) {
                    self.stack[i].0 = e;
                    continue;
                }
                let tmp = self.new_temp(t);
                self.emit(Stmt::Assign(Expr::Local(tmp.clone()), e));
                self.stack[i].0 = Expr::Local(tmp);
            }
        }
        if !tables {
            return;
        }
        let mut keys: Vec<_> = self.table_slots.keys().copied().collect();
        keys.sort();
        for k in keys {
            if pred(&self.table_slots[&k].0) {
                let (e, t) = self.table_slots[&k].clone();
                let tmp = self.new_temp(t);
                self.emit(Stmt::Assign(Expr::Local(tmp.clone()), e));
                self.table_slots.get_mut(&k).unwrap().0 = Expr::Local(tmp);
            }
        }
    }

    fn spill(&mut self, pred: impl Fn(&Expr) -> bool) {
        self.spill_with(true, pred);
    }

    /// Before a call or store. Call-table slots are deliberately not spilled:
    /// LLVM sets a slot immediately before the `call_indirect` that uses it,
    /// so treating the function global as read at the call is good enough.
    fn spill_globals(&mut self) {
        self.spill_with(false, |e| e.reads_globals());
    }

    fn spill_all(&mut self) {
        self.spill(|e| !e.is_const());
    }

    fn const_i32(e: &Expr, what: &str) -> Result<i32> {
        match e {
            Expr::Float(f) if f.fract() == 0.0 => Ok(*f as i32),
            other => bail!("{what} must be a compile-time constant, found `{other:?}`"),
        }
    }

    /// Read the QuakeC global at `addr`, which must be constant, a `select`
    /// between constants, or a local known to hold one of several constants
    /// (see [`Self::choose`]). LLVM merges `if (c) a.get() else b.get()` into
    /// one load of a selected address.
    fn global_load(&mut self, addr: &Expr, qt: QcType) -> Result<Expr> {
        if let Expr::Local(n) = addr {
            return self.choose(n, "memory address", |l, c| {
                l.global_load(&Expr::Float(c), qt)
            });
        }
        if let Expr::Select(c, a, b) = addr {
            return Ok(Expr::Select(
                c.clone(),
                Box::new(self.global_load(a, qt)?),
                Box::new(self.global_load(b, qt)?),
            ));
        }
        Ok(Expr::Global(self.global_at(addr, 0, qt)?))
    }

    /// Resolve a constant address into a QuakeC global: a system global slot,
    /// or (in static data) the name of a user global. A vector global is
    /// addressed by its first component.
    fn global_at(&mut self, addr: &Expr, offset: u64, expect: QcType) -> Result<String> {
        let base = Self::const_i32(addr, "memory address")?;
        let addr = base as i64 + offset as i64;
        ensure!(addr >= 0, "negative memory address {addr}");
        if StaticData::is_data_addr(addr as u64) {
            return self.ctx.user_name(addr as u64, expect, false);
        }
        if expect == QcType::Vector {
            let slot = self.ctx.globals.by_addr(addr as u64);
            return match slot.and_then(|s| s.vector.filter(|_| s.name.ends_with("_x"))) {
                Some(v) => Ok(v.to_string()),
                None => bail!("memory address {addr} is not a vector QuakeC global"),
            };
        }
        let slot = self.ctx.globals.by_addr(addr as u64).ok_or_else(|| {
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

    fn set_local(&mut self, id: LocalId, v: Expr, t: Ty) -> Result<()> {
        if self.may_alias.contains(&id) && self.retyped_param(&v).is_some() {
            self.aliases.insert(id, v);
            return Ok(());
        }
        let (name, lt) = self.local(id)?;
        self.unify(lt, t, &format!("assignment to local `{name}`"));
        let n = name.clone();
        self.spill(move |e| e.reads_local(&n));

        self.assign_local(name, v);
        Ok(())
    }

    /// Emit `name = v;`, folding `tN = call(..); name = tN;` into
    /// `name = call(..);`.
    fn assign_local(&mut self, name: String, v: Expr) {
        if let (Expr::Local(src), Some(tmp)) = (&v, &self.last_call_temp)
            && src == tmp
            && let Some(Stmt::Assign(lhs, _)) = self.out.last_mut()
        {
            self.known.remove(&name);
            *lhs = Expr::Local(name);
            let tmp = tmp.clone();
            self.temps.retain(|(n, _)| *n != tmp);
            self.last_call_temp = None;
            return;
        }
        self.emit(Stmt::Assign(Expr::Local(name), v));
    }

    /// Emit a call and push its result (if any).
    fn call(&mut self, callee: Expr, sig: &Sig, what: &str) -> Result<()> {
        let args = self.pop_n(sig.params.len())?;
        let mut arg_exprs = Vec::new();
        for (i, ((e, t), &pt)) in args.into_iter().zip(&sig.params).enumerate() {
            self.unify(pt, t, &format!("argument {i} of `{what}`"));
            arg_exprs.push(e);
        }
        self.spill_globals();
        let call = Expr::Call(Box::new(callee), arg_exprs);
        if sig.ret == Ty::Known(QcType::Void) {
            self.emit(Stmt::Expr(call));
        } else {
            let tmp = self.new_temp(sig.ret);
            self.emit(Stmt::Assign(Expr::Local(tmp.clone()), call));
            self.last_call_temp = Some(tmp.clone());
            self.stack.push((Expr::Local(tmp), sig.ret));
        }
        Ok(())
    }

    fn block_result(&mut self, seq: InstrSeqId) -> Result<Option<(String, Ty)>> {
        match self.func.block(seq).ty {
            InstrSeqType::Simple(None) => Ok(None),
            InstrSeqType::Simple(Some(t)) => {
                let ty = self.ctx.val_ty(t)?;
                Ok(Some((self.new_temp(ty), ty)))
            }
            InstrSeqType::MultiValue(_) => bail!("multi-value blocks are not supported"),
        }
    }

    /// Branch to `target`, consuming its result value from the stack.
    fn branch(&mut self, target: InstrSeqId) -> Result<Vec<Stmt>> {
        let idx = *self
            .label_of
            .get(&target)
            .ok_or_else(|| anyhow!("branch to unknown block"))?;
        self.labels[idx].used = true;
        self.materialize_top();
        let mut stmts = Vec::new();
        match self.labels[idx].kind {
            LabelKind::Return => {
                if self.ret == Ty::Known(QcType::Void) {
                    stmts.push(Stmt::Return(None));
                } else {
                    let (v, t) = self
                        .stack
                        .last()
                        .cloned()
                        .ok_or_else(|| anyhow!("stack underflow"))?;
                    self.unify(self.ret, t, "return value");
                    stmts.push(Stmt::Return(Some(v)));
                }
            }
            LabelKind::End => {
                if let Some((tmp, ty)) = self.labels[idx].result.clone() {
                    let (v, t) = self
                        .stack
                        .last()
                        .cloned()
                        .ok_or_else(|| anyhow!("stack underflow"))?;
                    self.unify(ty, t, "block result");
                    let mut known = self.known.clone();
                    match self.consts_of(&v) {
                        Some(cs) => known.insert(tmp.clone(), cs),
                        None => known.remove(&tmp),
                    };
                    self.reach(idx, known);
                    stmts.push(Stmt::Assign(Expr::Local(tmp), v));
                } else {
                    self.reach(idx, self.known.clone());
                }
                stmts.push(Stmt::Goto(self.labels[idx].name.clone()));
            }
            LabelKind::Loop => stmts.push(Stmt::Goto(self.labels[idx].name.clone())),
        }
        Ok(stmts)
    }

    /// Record that a branch reaches the `End` label `idx` with state `known`.
    fn reach(&mut self, idx: usize, known: Known) {
        let label = &mut self.labels[idx];
        label.incoming = Some(match label.incoming.take() {
            Some(k) => meet(&k, &known),
            None => known,
        });
    }

    /// Lower a sequence (block body) in place. Returns whether control can
    /// fall off the end.
    fn lower_seq(&mut self, seq: InstrSeqId) -> Result<bool> {
        let func = self.func;
        for (instr, _) in &func.block(seq).instrs {
            if !self
                .lower_instr(instr)
                .with_context(|| format!("at instruction {instr:?}"))?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Lower a nested sequence into a fresh statement list.
    fn lower_nested(&mut self, seq: InstrSeqId) -> Result<(Vec<Stmt>, bool)> {
        let saved = std::mem::take(&mut self.out);
        self.last_call_temp = None;
        let live = self.lower_seq(seq);
        // `last_call_temp` stays valid: callers append to the end of `body`.
        let body = std::mem::replace(&mut self.out, saved);
        Ok((body, live?))
    }

    /// At the fallthrough end of a block: move its result into the result temp.
    fn finish_block(
        &mut self,
        height: usize,
        live: bool,
        result: &Option<(String, Ty)>,
    ) -> Result<()> {
        if live && let Some((tmp, ty)) = result {
            let (v, t) = self.pop()?;
            self.unify(*ty, t, "block result");
            self.assign_local(tmp.clone(), v);
        }
        self.stack.truncate(height);
        Ok(())
    }

    /// Lower one instruction. Returns `false` if control cannot continue
    /// past it (unconditional branch, return, unreachable).
    fn lower_instr(&mut self, instr: &Instr) -> Result<bool> {
        match instr {
            Instr::Block(ir::Block { seq }) => {
                self.spill_all();
                let result = self.block_result(*seq)?;
                let name = self.new_label("blk");
                let idx = self.labels.len();
                self.labels.push(Label {
                    name: name.clone(),
                    kind: LabelKind::End,
                    result: result.clone(),
                    used: false,
                    incoming: None,
                });
                self.label_of.insert(*seq, idx);
                let height = self.stack.len();
                let (body, live) = self.lower_nested(*seq)?;
                self.out.extend(body);
                self.finish_block(height, live, &result)?;
                let fallthrough = live.then(|| std::mem::take(&mut self.known));
                self.merge_at_end(fallthrough, idx);
                if self.labels[idx].used {
                    self.emit(Stmt::Label(name));
                }
                self.push_result(result);
                Ok(live || self.labels[idx].used)
            }
            Instr::Loop(ir::Loop { seq }) => {
                self.spill_all();
                let name = self.new_label("loop");
                let idx = self.labels.len();
                self.labels.push(Label {
                    name: name.clone(),
                    kind: LabelKind::Loop,
                    result: None,
                    used: false,
                    incoming: None,
                });
                self.label_of.insert(*seq, idx);
                let height = self.stack.len();
                // The back edge may bring other values. The body is lowered
                // without assuming anything about them, so the states where
                // it exits hold on every iteration.
                self.forget_assigned_in(&[*seq]);
                let (body, live) = self.lower_nested(*seq)?;
                if self.labels[idx].used {
                    self.emit(Stmt::Label(name));
                }
                self.out.extend(body);
                // A loop's result (if any) is left on the stack by its body.
                let extra =
                    if live && !matches!(self.func.block(*seq).ty, InstrSeqType::Simple(None)) {
                        1
                    } else {
                        0
                    };
                self.stack
                    .truncate(height + extra.min(self.stack.len() - height));
                Ok(live)
            }
            Instr::IfElse(ir::IfElse {
                consequent,
                alternative,
            }) => {
                let (cond, _) = self.pop()?;
                self.spill_all();
                let result = self.block_result(*consequent)?;
                let name = self.new_label("endif");
                let idx = self.labels.len();
                self.labels.push(Label {
                    name: name.clone(),
                    kind: LabelKind::End,
                    result: result.clone(),
                    used: false,
                    incoming: None,
                });
                self.label_of.insert(*consequent, idx);
                self.label_of.insert(*alternative, idx);
                let height = self.stack.len();

                let entry = self.known.clone();
                let (mut then_body, then_live) = self.lower_nested(*consequent)?;
                std::mem::swap(&mut self.out, &mut then_body);
                self.finish_block(height, then_live, &result)?;
                std::mem::swap(&mut self.out, &mut then_body);
                let then_known = std::mem::replace(&mut self.known, entry);

                let (mut else_body, else_live) = self.lower_nested(*alternative)?;
                std::mem::swap(&mut self.out, &mut else_body);
                self.finish_block(height, else_live, &result)?;
                std::mem::swap(&mut self.out, &mut else_body);
                let else_known = std::mem::take(&mut self.known);

                let fallthrough = match (then_live, else_live) {
                    (true, true) => Some(meet(&then_known, &else_known)),
                    (true, false) => Some(then_known),
                    (false, true) => Some(else_known),
                    (false, false) => None,
                };
                self.merge_at_end(fallthrough, idx);
                self.emit(Stmt::If(cond, then_body, else_body));
                if self.labels[idx].used {
                    self.emit(Stmt::Label(name));
                }
                self.push_result(result);
                Ok(then_live || else_live || self.labels[idx].used)
            }
            Instr::Br(ir::Br { block }) => {
                let stmts = self.branch(*block)?;
                for s in stmts {
                    self.emit(s);
                }
                Ok(false)
            }
            Instr::BrIf(ir::BrIf { block }) => {
                let (cond, _) = self.pop()?;
                self.materialize_top();
                // The branch value (if any) is used twice: spill it.
                if let Some((e, _)) = self.stack.last()
                    && !e.is_const()
                    && !matches!(e, Expr::Local(_))
                {
                    let i = self.stack.len() - 1;
                    let (e, t) = self.stack[i].clone();
                    let tmp = self.new_temp(t);
                    self.emit(Stmt::Assign(Expr::Local(tmp.clone()), e));
                    self.stack[i].0 = Expr::Local(tmp);
                }
                let stmts = self.branch(*block)?;
                self.emit(Stmt::If(cond, stmts, vec![]));
                Ok(true)
            }
            Instr::BrTable(ir::BrTable { blocks, default }) => {
                let (idx, _) = self.pop()?;
                let idx = if idx.is_const() || matches!(idx, Expr::Local(_)) {
                    idx
                } else {
                    let tmp = self.new_temp(Ty::Known(QcType::Float));
                    self.emit(Stmt::Assign(Expr::Local(tmp.clone()), idx));
                    Expr::Local(tmp)
                };
                for (i, b) in blocks.iter().enumerate() {
                    let stmts = self.branch(*b)?;
                    let cond = Expr::Bin(
                        BinOp::Eq,
                        Box::new(idx.clone()),
                        Box::new(Expr::Float(i as f32)),
                    );
                    self.emit(Stmt::If(cond, stmts, vec![]));
                }
                let stmts = self.branch(*default)?;
                for s in stmts {
                    self.emit(s);
                }
                Ok(false)
            }
            Instr::Return(_) => {
                self.lower_return()?;
                Ok(false)
            }
            // Tail calls: an ordinary call followed by a return.
            Instr::ReturnCall(ir::ReturnCall { func }) => {
                self.lower_call(*func)?;
                self.lower_return()?;
                Ok(false)
            }
            Instr::ReturnCallIndirect(ir::ReturnCallIndirect { ty, table }) => {
                self.lower_call_indirect(*ty, *table)?;
                self.lower_return()?;
                Ok(false)
            }
            Instr::Unreachable(_) => {
                self.emit(Stmt::Comment("unreachable".into()));
                let v = match self.ret {
                    Ty::Known(QcType::Void) => None,
                    Ty::Known(QcType::Float) => Some(Expr::Float(0.0)),
                    Ty::Known(QcType::Vector) => Some(Expr::Vector([0.0; 3])),
                    _ => Some(Expr::Null),
                };
                self.emit(Stmt::Return(v));
                Ok(false)
            }
            Instr::Drop(_) => {
                self.pop()?;
                Ok(true)
            }
            Instr::Select(_) => {
                let (c, _) = self.pop()?;
                let (b, tb) = self.pop()?;
                let (a, ta) = self.pop()?;
                self.unify(ta, tb, "select operands");
                self.stack
                    .push((Expr::Select(Box::new(c), Box::new(a), Box::new(b)), ta));
                Ok(true)
            }
            Instr::LocalGet(ir::LocalGet { local }) if self.base_locals.contains_key(local) => {
                self.stack.push((
                    Expr::Float(self.base_locals[local]),
                    Ty::Known(QcType::Float),
                ));
                Ok(true)
            }
            Instr::LocalSet(ir::LocalSet { local }) if self.base_locals.contains_key(local) => {
                self.pop()?;
                Ok(true)
            }
            Instr::LocalTee(ir::LocalTee { local }) if self.base_locals.contains_key(local) => {
                Ok(true)
            }
            Instr::LocalGet(ir::LocalGet { local }) if self.aliases.contains_key(local) => {
                let e = self.aliases[local].clone();
                self.stack.push((e, Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::LocalGet(ir::LocalGet { local }) => {
                let (n, t) = self.local(*local)?;
                self.push_local(n, t);
                Ok(true)
            }
            Instr::LocalSet(ir::LocalSet { local }) => {
                let (v, t) = self.pop()?;
                self.set_local(*local, v, t)?;
                Ok(true)
            }
            Instr::LocalTee(ir::LocalTee { local }) => {
                let (v, t) = self.pop()?;
                self.set_local(*local, v, t)?;
                if let Some(e) = self.aliases.get(local) {
                    self.stack.push((e.clone(), Ty::Known(QcType::Float)));
                    return Ok(true);
                }
                let (n, t) = self.local(*local)?;
                self.push_local(n, t);
                Ok(true)
            }
            Instr::GlobalGet(ir::GlobalGet { global })
            | Instr::GlobalSet(ir::GlobalSet { global })
                if Some(*global) == self.ctx.stack_pointer =>
            {
                bail!(
                    "this function uses the shadow stack (`__stack_pointer`), which QuakeC can't \
                     support. Usually a struct was passed by reference or a local's address was \
                     taken; pass `Entity`, `String` and `Vector` values by value instead"
                )
            }
            Instr::GlobalGet(ir::GlobalGet { global }) if Some(*global) == self.ctx.memory_base => {
                self.stack
                    .push((Expr::Float(DATA_BASE as f32), Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::GlobalGet(ir::GlobalGet { global }) if Some(*global) == self.ctx.table_base => {
                self.stack
                    .push((Expr::Float(TABLE_BASE as f32), Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::GlobalGet(ir::GlobalGet { global }) => {
                let (n, t) = self
                    .ctx
                    .wasm_globals
                    .get(global)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown global"))?;
                self.stack.push((Expr::Global(n), t));
                Ok(true)
            }
            Instr::GlobalSet(ir::GlobalSet { global }) => {
                let (v, t) = self.pop()?;
                let (n, gt) = self
                    .ctx
                    .wasm_globals
                    .get(global)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown global"))?;
                self.unify(gt, t, "global assignment");
                self.spill_globals();
                self.emit(Stmt::Assign(Expr::Global(n), v));
                Ok(true)
            }
            Instr::Const(ir::Const { value }) => {
                let f = match *value {
                    Value::I32(v) => {
                        if v.unsigned_abs() > MAX_EXACT_INT as u32 {
                            self.ctx.warnings.push(format!(
                                "integer constant {v} is not exactly representable as a QuakeC float"
                            ));
                        }
                        v as f32
                    }
                    Value::F32(f) => {
                        ensure!(f.is_finite(), "non-finite float constant {f}");
                        f
                    }
                    Value::V128(bits) => {
                        let lane = |i: u32| f32::from_bits((bits >> (32 * i)) as u32);
                        let v = [lane(0), lane(1), lane(2)];
                        ensure!(
                            v.iter().all(|f| f.is_finite()),
                            "non-finite vector constant {v:?}"
                        );
                        self.stack
                            .push((Expr::Vector(v), Ty::Known(QcType::Vector)));
                        return Ok(true);
                    }
                    other => bail!("unsupported constant {other:?}"),
                };
                self.stack.push((Expr::Float(f), Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::Binop(ir::Binop {
                op: BinaryOp::F32x4ReplaceLane { idx },
            }) => {
                let (x, _) = self.pop()?;
                let (v, _) = self.pop_raw()?;
                let e = self.replace_lane(v, *idx, x);
                self.stack.push((e, Ty::Known(QcType::Vector)));
                Ok(true)
            }
            Instr::Binop(ir::Binop {
                op:
                    op @ (BinaryOp::F32x4Eq
                    | BinaryOp::F32x4Ne
                    | BinaryOp::F32x4Lt
                    | BinaryOp::F32x4Gt
                    | BinaryOp::F32x4Le
                    | BinaryOp::F32x4Ge
                    | BinaryOp::F32x4PMin
                    | BinaryOp::F32x4PMax),
            }) => {
                let (b, _) = self.pop_raw()?;
                let (a, _) = self.pop_raw()?;
                let e = self.lanewise(*op, a, b);
                self.stack.push((e, Ty::Known(QcType::Vector)));
                Ok(true)
            }
            // On a mask (see `lanewise`).
            Instr::Unop(ir::Unop {
                op: UnaryOp::V128Not,
            }) => {
                let (m, _) = self.pop_raw()?;
                let parts = self
                    .vector_lanes(m)
                    .map(|x| Expr::Un(UnOp::Not, Box::new(x)));
                self.stack
                    .push((Expr::MakeVector(Box::new(parts)), Ty::Known(QcType::Vector)));
                Ok(true)
            }
            // `m ? a : b` lane by lane, for a mask `m` (see `lanewise`).
            Instr::V128Bitselect(_) => {
                let (m, _) = self.pop_raw()?;
                let (b, _) = self.pop_raw()?;
                let (a, _) = self.pop_raw()?;
                let (m, b, a) = (
                    self.vector_lanes(m),
                    self.vector_lanes(b),
                    self.vector_lanes(a),
                );
                let [m0, m1, m2] = m;
                let [b0, b1, b2] = b;
                let [a0, a1, a2] = a;
                let sel = |m, a, b| Expr::Select(Box::new(m), Box::new(a), Box::new(b));
                let parts = [sel(m0, a0, b0), sel(m1, a1, b1), sel(m2, a2, b2)];
                self.stack
                    .push((Expr::MakeVector(Box::new(parts)), Ty::Known(QcType::Vector)));
                Ok(true)
            }
            // A lane of a mask (see `lanewise`).
            Instr::Unop(ir::Unop {
                op: UnaryOp::I32x4ExtractLane { idx },
            }) => {
                let (v, _) = self.pop_raw()?;
                let e = self.extract_lane(v, *idx);
                self.stack.push((e, Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::I8x16Shuffle(ir::I8x16Shuffle { indices }) => {
                let (b, _) = self.pop_raw()?;
                let (a, _) = self.pop_raw()?;
                let e = self.shuffle(a, b, indices)?;
                self.stack.push((e, Ty::Known(QcType::Vector)));
                Ok(true)
            }
            Instr::Unop(ir::Unop {
                op: UnaryOp::F32x4ExtractLane { idx },
            }) => {
                let (v, _) = self.pop_raw()?;
                let e = self.extract_lane(v, *idx);
                self.stack.push((e, Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::LoadSimd(ir::LoadSimd { kind, arg, .. }) => {
                use ir::LoadSimdKind as K;
                match kind {
                    K::V128Load32Zero | K::V128Load64Zero => {
                        let (addr, _) = self.pop()?;
                        let n = if matches!(kind, K::V128Load32Zero) {
                            1
                        } else {
                            2
                        };
                        let mut parts = [Expr::Float(0.0), Expr::Float(0.0), Expr::Float(0.0)];
                        for (i, p) in parts.iter_mut().enumerate().take(n) {
                            *p = Expr::Global(self.global_at(
                                &addr,
                                arg.offset + 4 * i as u64,
                                QcType::Float,
                            )?);
                        }
                        self.stack
                            .push((Expr::MakeVector(Box::new(parts)), Ty::Known(QcType::Vector)));
                    }
                    K::V128Load32Lane(lane) => {
                        let (v, _) = self.pop_raw()?;
                        let (addr, _) = self.pop()?;
                        let x = Expr::Global(self.global_at(&addr, arg.offset, QcType::Float)?);
                        let e = self.replace_lane(v, *lane, x);
                        self.stack.push((e, Ty::Known(QcType::Vector)));
                    }
                    K::V128Store32Lane(lane) => {
                        let (v, _) = self.pop_raw()?;
                        let (addr, _) = self.pop()?;
                        let g = self.global_at(&addr, arg.offset, QcType::Float)?;
                        let x = self.extract_lane(v, *lane);
                        self.spill_globals();
                        self.emit(Stmt::Assign(Expr::Global(g), x));
                    }
                    other => bail!("unsupported SIMD memory access {other:?}"),
                }
                Ok(true)
            }
            Instr::Binop(ir::Binop {
                op: op @ (BinaryOp::F32x4Mul | BinaryOp::F32x4Div),
            }) => {
                let (b, _) = self.pop_raw()?;
                let (a, _) = self.pop_raw()?;
                let e = self.vector_mul_div(matches!(op, BinaryOp::F32x4Div), a, b);
                self.stack.push((e, Ty::Known(QcType::Vector)));
                Ok(true)
            }
            Instr::Binop(ir::Binop { op }) => {
                let (b, _) = self.pop()?;
                let (a, _) = self.pop()?;
                let (e, t) = match *op {
                    BinaryOp::F32x4Add => (
                        Expr::Bin(BinOp::Add, Box::new(a), Box::new(b)),
                        QcType::Vector,
                    ),
                    BinaryOp::F32x4Sub => (
                        Expr::Bin(BinOp::Sub, Box::new(a), Box::new(b)),
                        QcType::Vector,
                    ),
                    op => (lower_binop(op, a, b)?, QcType::Float),
                };
                self.stack.push((e, Ty::Known(t)));
                Ok(true)
            }
            Instr::Unop(ir::Unop { op }) => {
                let (a, _) = self.pop()?;
                let (e, t) = match *op {
                    // Float → int truncation. `OP_BITOR` converts both operands
                    // to int (truncating toward zero) and back. `x | 0` would
                    // be folded away by fteqcc, but `x | x` is kept.
                    UnaryOp::I32TruncSF32
                    | UnaryOp::I32TruncUF32
                    | UnaryOp::I32TruncSSatF32
                    | UnaryOp::I32TruncUSatF32 => {
                        let e = match a {
                            Expr::Float(f) => Expr::Float(f.trunc()),
                            a => {
                                let a = self.simple(a, Ty::Known(QcType::Float));
                                Expr::Bin(BinOp::BitOr, Box::new(a.clone()), Box::new(a))
                            }
                        };
                        (e, QcType::Float)
                    }
                    UnaryOp::F32x4Splat => (self.splat(a), QcType::Vector),
                    UnaryOp::F32x4Neg => (Expr::Un(UnOp::Neg, Box::new(a)), QcType::Vector),
                    op => (lower_unop(op, a)?, QcType::Float),
                };
                self.stack.push((e, Ty::Known(t)));
                Ok(true)
            }
            Instr::Call(ir::Call { func }) => {
                self.lower_call(*func)?;
                Ok(true)
            }
            Instr::CallIndirect(ir::CallIndirect { ty, table }) => {
                self.lower_call_indirect(*ty, *table)?;
                Ok(true)
            }
            Instr::TableSet(ir::TableSet { table }) => {
                ensure!(
                    Some(*table) == self.ctx.call_table,
                    "table.set on unsupported table"
                );
                let (v, t) = self.pop()?;
                let (idx, _) = self.pop()?;
                let idx = Self::const_i32(&idx, "table.set index")?;
                if matches!(v, Expr::Null) {
                    self.table_slots.remove(&idx);
                } else {
                    self.table_slots.insert(idx, (v, t));
                }
                Ok(true)
            }
            Instr::RefNull(ir::RefNull { ty }) => {
                let t = self.ctx.val_ty(walrus::ValType::Ref(*ty))?;
                self.stack.push((Expr::Null, t));
                Ok(true)
            }
            Instr::RefIsNull(_) => {
                let (v, _) = self.pop()?;
                self.stack
                    .push((Expr::Un(UnOp::Not, Box::new(v)), Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::RefFunc(ir::RefFunc { func }) => {
                let name = self
                    .ctx
                    .funcs
                    .get(func)
                    .map(|f| f.name.clone())
                    .ok_or_else(|| anyhow!("unknown function"))?;
                let f = self.func_value(name);
                self.stack.push((f, Ty::Known(QcType::Func)));
                Ok(true)
            }
            Instr::Load(ir::Load { kind, arg, .. }) => {
                let (addr, _) = self.pop()?;
                if self.lower_data_load(&addr, *kind, arg.offset)? {
                    return Ok(true);
                }
                match kind {
                    LoadKind::F32 => {
                        let g = self.global_at(&addr, arg.offset, QcType::Float)?;
                        self.stack.push((Expr::Global(g), Ty::Known(QcType::Float)));
                    }
                    other => bail!(
                        "unsupported load {other:?}: QuakeC globals must be read as f32 \
                         (or via __quakec_load_* for entities, strings and functions)"
                    ),
                }
                Ok(true)
            }
            Instr::Store(ir::Store { kind, arg, .. }) => {
                let (v, _) = self.pop()?;
                let (addr, _) = self.pop()?;
                if let Expr::Float(a) = addr
                    && StaticData::is_data_addr(a as u64 + arg.offset)
                {
                    bail!(
                        "writes to static data are not supported (address {}); Zig `var` globals \
                         can't be used, declare QuakeC globals with `defs.userGlobal` instead",
                        a as u64 + arg.offset
                    );
                }
                match kind {
                    StoreKind::F32 => {
                        let g = self.global_at(&addr, arg.offset, QcType::Float)?;
                        self.spill_globals();
                        self.emit(Stmt::Assign(Expr::Global(g), v));
                    }
                    other => bail!(
                        "unsupported store {other:?}: QuakeC globals must be written as f32 \
                         (or via __quakec_store_* for entities, strings and functions)"
                    ),
                }
                Ok(true)
            }
            _ => bail!("unsupported instruction"),
        }
    }

    /// Handle a load from static data. Returns `false` if this is a load
    /// from a QuakeC global instead (a constant address below `DATA_BASE`).
    fn lower_data_load(&mut self, addr: &Expr, kind: LoadKind, offset: u64) -> Result<bool> {
        if let Expr::Float(a) = addr {
            let a = *a as u64 + offset;
            if !StaticData::is_data_addr(a) {
                return Ok(false);
            }
            let dk = DataKind::from_load(kind)
                .ok_or_else(|| anyhow!("unsupported load {kind:?} from static data"))?;
            let v = self
                .ctx
                .data
                .read(a, dk)
                .ok_or_else(|| anyhow!("load from address {a}, outside of static data"))?;
            if v.fract() == 0.0 && v.abs() > MAX_EXACT_INT as f32 && dk != DataKind::F32 {
                self.ctx.warnings.push(format!(
                    "static data value {v} is not exactly representable as a QuakeC float"
                ));
            }
            self.stack.push((Expr::Float(v), Ty::Known(QcType::Float)));
            return Ok(true);
        }
        // A dynamic address: assume it points into static data, since QuakeC
        // globals can't be indexed dynamically.
        let dk = DataKind::from_load(kind)
            .ok_or_else(|| anyhow!("unsupported load {kind:?} from a non-constant address"))?;
        self.ctx.data_accessors.insert(dk);
        let addr = if offset == 0 {
            addr.clone()
        } else {
            fold(BinOp::Add, addr.clone(), Expr::Float(offset as f32))
        };
        self.stack.push((addr, Ty::Known(QcType::Float)));
        let sig = Sig {
            params: vec![Ty::Known(QcType::Float)],
            ret: Ty::Known(QcType::Float),
        };
        self.call(
            Expr::FuncName(dk.accessor_name().into()),
            &sig,
            dk.accessor_name(),
        )?;
        Ok(true)
    }

    fn lower_return(&mut self) -> Result<()> {
        if self.ret == Ty::Known(QcType::Void) {
            self.emit(Stmt::Return(None));
        } else {
            let (v, t) = self.pop()?;
            self.unify(self.ret, t, "return value");
            self.emit(Stmt::Return(Some(v)));
        }
        Ok(())
    }

    /// Cast a function value to `sig`, unless it already has that type: a
    /// named function with that signature, or any other function value
    /// (always `void()`) when `sig` is `void()`.
    fn fn_cast(&self, sig: &Sig, f: Expr) -> Expr {
        let f = match f {
            Expr::FnCast(_, inner) => *inner,
            f => f,
        };
        let own = match &f {
            Expr::FuncName(n) => self
                .ctx
                .funcs
                .values()
                .find(|i| i.name == *n)
                .map(|i| i.sig.clone()),
            _ => Some(Sig::void()),
        };
        if own.as_ref() == Some(sig) {
            f
        } else {
            Expr::FnCast(sig.clone(), Box::new(f))
        }
    }

    /// A function value, which always has type `void()` (see [`Self::fn_cast`]).
    fn func_value(&self, name: String) -> Expr {
        self.fn_cast(&Sig::void(), Expr::FuncName(name))
    }

    /// The QuakeC function a Zig function pointer (an index into
    /// `__indirect_function_table`) refers to. Must be a constant, a
    /// `select` between constants, or a local known to hold one of several
    /// constants (see [`Self::choose`]).
    fn func_ref(&mut self, ptr: &Expr) -> Result<Expr> {
        if let Some(p) = self.retyped_param(ptr) {
            return Ok(Expr::Local(p));
        }
        match ptr {
            Expr::Local(n) => {
                self.choose(n, "function pointer", |l, c| l.func_ref(&Expr::Float(c)))
            }
            Expr::Float(i) => {
                let f = self.ctx.table_funcs.get(&(*i as u64)).with_context(|| {
                    format!("function pointer {i} is not in `__indirect_function_table`")
                })?;
                let name = self
                    .ctx
                    .funcs
                    .get(f)
                    .map(|f| f.name.clone())
                    .context("unknown function")?;
                Ok(self.func_value(name))
            }
            Expr::Select(c, a, b) => Ok(Expr::Select(
                c.clone(),
                Box::new(self.func_ref(a)?),
                Box::new(self.func_ref(b)?),
            )),
            other => bail!(
                "function pointer passed to `__quakec_func_ref` is not a constant: `{other:?}`"
            ),
        }
    }

    fn lower_call_indirect(&mut self, ty: walrus::TypeId, table: walrus::TableId) -> Result<()> {
        ensure!(
            Some(table) == self.ctx.call_table,
            "call_indirect on unsupported table"
        );
        let (idx, _) = self.pop()?;
        let idx = Self::const_i32(&idx, "call_indirect table index")?;
        let (f, _) = self.table_slots.get(&idx).cloned().ok_or_else(|| {
            anyhow!("call_indirect through call table slot {idx}, which was not set")
        })?;
        let sig = self.ctx.sig_of_type(ty)?;
        let callee = self.fn_cast(&sig, f);
        self.call(callee, &sig, "indirect call")
    }

    /// A retyped parameter (see `params.rs`) that `e` reads, plus the
    /// constant offset the analysis allowed for.
    fn retyped_param(&self, e: &Expr) -> Option<String> {
        let p = match e {
            Expr::Local(p) => p,
            Expr::Bin(BinOp::Add, a, b) => match (&**a, &**b) {
                (Expr::Float(_), Expr::Local(p)) | (Expr::Local(p), Expr::Float(_)) => p,
                _ => return None,
            },
            _ => return None,
        };
        self.retyped.contains(p).then(|| p.clone())
    }

    /// Before a call to `callee`: convert the arguments for its retyped
    /// parameters into the function or string they refer to.
    fn convert_args(&mut self, reprs: &[Option<ParamRepr>], callee: &str) -> Result<()> {
        ensure!(
            self.stack.len() >= reprs.len(),
            "wasm operand stack underflow"
        );
        let base = self.stack.len() - reprs.len();
        for (i, r) in reprs.iter().enumerate() {
            let Some(r) = *r else { continue };
            let arg = self.stack[base + i].0.clone();
            let ctx = || format!("in argument {i} of `{callee}`");
            let (e, qt) = match r {
                ParamRepr::Func { offset } => {
                    let e = self
                        .offset_arg(&arg, offset as f32, "function pointer", &|l, a| {
                            l.func_ref(&a)
                        })
                        .with_context(ctx)?;
                    (e, QcType::Func)
                }
                ParamRepr::Str { offset, len } => {
                    let len = match len {
                        Len::Const(l) => Expr::Float(l as f32),
                        Len::Param(j) => self.stack[base + j].0.clone(),
                    };
                    let e = self
                        .offset_arg(&arg, offset as f32, "string literal address", &|l, a| {
                            l.string_literal(&a, &len)
                        })
                        .with_context(ctx)?;
                    (e, QcType::String)
                }
                ParamRepr::Field { offset, ty } => {
                    let e = self
                        .offset_arg(&arg, offset as f32, "entity field offset", &|l, a| {
                            Ok(Expr::Global(l.field_name(&a, ty)?))
                        })
                        .with_context(ctx)?;
                    (e, ty.field_of().expect("fields have value types"))
                }
            };
            self.stack[base + i] = (e, Ty::Known(qt));
        }
        Ok(())
    }

    /// `arm(e + offset)`, where `e` is a constant, a `select` between
    /// constants or a local known to hold one of several. A retyped parameter
    /// is passed on as is.
    fn offset_arg(
        &mut self,
        e: &Expr,
        offset: f32,
        what: &str,
        arm: &dyn Fn(&mut Self, Expr) -> Result<Expr>,
    ) -> Result<Expr> {
        if let Some(p) = self.retyped_param(e) {
            return Ok(Expr::Local(p));
        }
        match e {
            Expr::Float(c) => arm(self, Expr::Float(c + offset)),
            Expr::Select(c, a, b) => Ok(Expr::Select(
                c.clone(),
                Box::new(self.offset_arg(a, offset, what, arm)?),
                Box::new(self.offset_arg(b, offset, what, arm)?),
            )),
            Expr::Local(n) => self.choose(n, what, |l, c| arm(l, Expr::Float(c + offset))),
            e => arm(self, fold(BinOp::Add, Expr::Float(offset), e.clone())),
        }
    }

    /// Make `e` a plain local or global (spilling it if necessary), so that it
    /// can be used as the base of a component or field access.
    fn simple(&mut self, e: Expr, ty: Ty) -> Expr {
        match e {
            Expr::Local(_) | Expr::Global(_) => e,
            e => {
                let tmp = self.new_temp(ty);
                self.emit(Stmt::Assign(Expr::Local(tmp.clone()), e));
                Expr::Local(tmp)
            }
        }
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
            (v, i) => Expr::Component(Box::new(self.simple(v, Ty::Known(QcType::Vector))), i),
        }
    }

    /// The three components of a vector, evaluating it only once.
    fn vector_lanes(&mut self, v: Expr) -> [Expr; 3] {
        let v = match v {
            v @ (Expr::Vector(_) | Expr::MakeVector(_) | Expr::Local(_) | Expr::Global(_)) => v,
            v => {
                let v = self.materialize(v);
                self.simple(v, Ty::Known(QcType::Vector))
            }
        };
        [0, 1, 2].map(|i| self.extract_lane(v.clone(), i))
    }

    /// A lane-wise `f32x4` comparison, `pmin` or `pmax`. LLVM vectorizes runs
    /// of component comparisons (like bounding box checks). Comparison
    /// results are masks, represented as vectors with 0 or 1 in each lane
    /// (wasm uses all-ones lanes, but the masks are only ever used through
    /// `v128.not`, `v128.bitselect` and `i32x4.extract_lane`, and the code
    /// LLVM generates only tests their low bit).
    fn lanewise(&mut self, op: BinaryOp, a: Expr, b: Expr) -> Expr {
        let (a, b) = (self.vector_lanes(a), self.vector_lanes(b));
        let lane = |x: Expr, y: Expr| {
            let sel =
                |c: Expr, t: Expr, f: Expr| Expr::Select(Box::new(c), Box::new(t), Box::new(f));
            let cmp = |op| fold(op, x.clone(), y.clone());
            match op {
                // `pmin(a, b)` is `b < a ? b : a`, `pmax(a, b)` is `a < b ? b : a`.
                BinaryOp::F32x4PMin => sel(fold(BinOp::Lt, y.clone(), x.clone()), y, x),
                BinaryOp::F32x4PMax => sel(cmp(BinOp::Lt), y, x),
                BinaryOp::F32x4Eq => cmp(BinOp::Eq),
                BinaryOp::F32x4Ne => cmp(BinOp::Ne),
                BinaryOp::F32x4Lt => cmp(BinOp::Lt),
                BinaryOp::F32x4Gt => cmp(BinOp::Gt),
                BinaryOp::F32x4Le => cmp(BinOp::Le),
                BinaryOp::F32x4Ge => cmp(BinOp::Ge),
                _ => unreachable!("not a lane-wise operator"),
            }
        };
        let [a0, a1, a2] = a;
        let [b0, b1, b2] = b;
        Expr::MakeVector(Box::new([lane(a0, b0), lane(a1, b1), lane(a2, b2)]))
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
                Some(this.simple(v, Ty::Known(QcType::Vector)))
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
                let v = self.simple(v, Ty::Known(QcType::Vector));
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
        let x = self.simple(x, Ty::Known(QcType::Float));
        Expr::MakeVector(Box::new([x.clone(), x.clone(), x]))
    }

    fn field_name(&mut self, offset: &Expr, ty: QcType) -> Result<String> {
        if let Some(p) = self.retyped_param(offset) {
            // A field reference parameter: `e.(p)`.
            return Ok(format!("({p})"));
        }
        let offset = Self::const_i32(offset, "entity field offset")?;
        ensure!(offset >= 0, "negative entity field offset {offset}");
        if StaticData::is_data_addr(offset as u64) {
            // A user field, named by a string in static data.
            return self.ctx.user_name(offset as u64, ty, true);
        }
        self.ctx.defs.field_at(offset as u32, ty).ok_or_else(|| {
            anyhow!(
                "entity field offset {offset} is not a known {} field",
                ty.name()
            )
        })
    }

    /// The string literal of `len` bytes at `ptr`. Both must be constants,
    /// or `select`s on the same condition between constants (LLVM sinks
    /// `if (c) lit("a") else lit("bc")` into one call). `ptr` may also be a
    /// local known to hold one of several constants, if `len` is constant
    /// (see [`Self::choose`]).
    fn string_literal(&mut self, ptr: &Expr, len: &Expr) -> Result<Expr> {
        if let Some(p) = self.retyped_param(ptr) {
            return Ok(Expr::Local(p));
        }
        if let (Expr::Local(n), Expr::Float(_)) = (ptr, len) {
            return self.choose(n, "string literal address", |l, c| {
                l.string_literal(&Expr::Float(c), len)
            });
        }
        match (ptr, len) {
            (Expr::Select(c, a, b), Expr::Select(lc, la, lb)) if c == lc => Ok(Expr::Select(
                c.clone(),
                Box::new(self.string_literal(a, la)?),
                Box::new(self.string_literal(b, lb)?),
            )),
            (Expr::Select(c, a, b), Expr::Float(_)) => Ok(Expr::Select(
                c.clone(),
                Box::new(self.string_literal(a, len)?),
                Box::new(self.string_literal(b, len)?),
            )),
            (_, Expr::Float(_)) => {
                let len = Self::const_i32(len, "string literal length")?;
                ensure!(len >= 0, "negative string literal length {len}");
                if len == 0 {
                    // Zig may give `""` any address.
                    return Ok(Expr::Str(Vec::new()));
                }
                let Expr::Float(a) = ptr else {
                    bail!("string literal address is not a constant")
                };
                let bytes = self.ctx.data.string(*a as u64, len as u64).ok_or_else(|| {
                    anyhow!("string literal at address {a} is outside of static data")
                })?;
                ensure!(
                    !bytes.contains(&0),
                    "string literal at address {a} contains a NUL byte"
                );
                Ok(Expr::Str(bytes))
            }
            _ => bail!("string literal length must be a constant, found `{len:?}`"),
        }
    }

    fn lower_call(&mut self, func: FunctionId) -> Result<()> {
        let info = self
            .ctx
            .funcs
            .get(&func)
            .ok_or_else(|| anyhow!("call to unknown function"))?;
        let (name, sig, kind) = (info.name.clone(), info.sig.clone(), info.kind);
        let FuncKind::Intrinsic(intr) = kind else {
            if let Some(reprs) = self.ctx.param_reprs.get(&func).cloned() {
                self.convert_args(&reprs, &name)?;
            }
            return self.call(Expr::FuncName(name.clone()), &sig, &name);
        };

        use Intrinsic::*;
        match intr {
            LoadFloat | LoadVector | LoadEntity | LoadString | LoadFn => {
                let qt = match intr {
                    LoadFloat => QcType::Float,
                    LoadVector => QcType::Vector,
                    LoadEntity => QcType::Entity,
                    LoadString => QcType::String,
                    _ => QcType::Func,
                };
                let (addr, _) = self.pop()?;
                let e = self.global_load(&addr, qt)?;
                self.stack.push((e, Ty::Known(qt)));
            }
            StoreFloat | StoreVector | StoreEntity | StoreString | StoreFn => {
                let qt = match intr {
                    StoreFloat => QcType::Float,
                    StoreVector => QcType::Vector,
                    StoreEntity => QcType::Entity,
                    StoreString => QcType::String,
                    _ => QcType::Func,
                };
                let (v, t) = self.pop()?;
                let (addr, _) = self.pop()?;
                let g = self.global_at(&addr, 0, qt)?;
                self.unify(Ty::Known(qt), t, &format!("store to `{g}`"));
                // Function globals are declared `void()`; cast named functions.
                let v = match v {
                    Expr::FuncName(_) => self.fn_cast(&Sig::void(), v),
                    v => v,
                };
                self.spill_globals();
                self.emit(Stmt::Assign(Expr::Global(g), v));
            }
            EqString | NeString | EqEntity | NeEntity => {
                let qt = if matches!(intr, EqString | NeString) {
                    QcType::String
                } else {
                    QcType::Entity
                };
                let op = if matches!(intr, EqString | EqEntity) {
                    BinOp::Eq
                } else {
                    BinOp::Ne
                };
                let (b, tb) = self.pop()?;
                let (a, ta) = self.pop()?;
                self.unify(Ty::Known(qt), ta, &name);
                self.unify(Ty::Known(qt), tb, &name);
                self.stack.push((
                    Expr::Bin(op, Box::new(a), Box::new(b)),
                    Ty::Known(QcType::Float),
                ));
            }
            LoadField(qt) => {
                let (offset, _) = self.pop()?;
                let (ent, te) = self.pop()?;
                let name = self.field_name(&offset, qt)?;
                self.unify(Ty::Known(QcType::Entity), te, &name);
                let ent = self.simple(ent, Ty::Known(QcType::Entity));
                self.stack
                    .push((Expr::Field(Box::new(ent), name), Ty::Known(qt)));
            }
            StoreField(qt) => {
                let (v, t) = self.pop()?;
                let (offset, _) = self.pop()?;
                let (ent, te) = self.pop()?;
                let name = self.field_name(&offset, qt)?;
                self.unify(Ty::Known(QcType::Entity), te, &name);
                self.unify(Ty::Known(qt), t, &format!("store to field `{name}`"));
                let ent = self.simple(ent, Ty::Known(QcType::Entity));
                let v = match v {
                    // Function fields are `void()`; cast named functions.
                    Expr::FuncName(_) => self.fn_cast(&Sig::void(), v),
                    v => v,
                };
                self.spill_globals();
                self.emit(Stmt::Assign(Expr::Field(Box::new(ent), name), v));
            }
            StringLiteral => {
                let (len, _) = self.pop()?;
                let (ptr, _) = self.pop()?;
                let s = self.string_literal(&ptr, &len)?;
                self.stack.push((s, Ty::Known(QcType::String)));
            }
            EqFn | NeFn | EqVector | NeVector | DotVector => {
                let op = match intr {
                    EqFn | EqVector => BinOp::Eq,
                    NeFn | NeVector => BinOp::Ne,
                    _ => BinOp::Mul,
                };
                let (b, _) = self.pop()?;
                let (a, _) = self.pop()?;
                self.stack.push((
                    Expr::Bin(op, Box::new(a), Box::new(b)),
                    Ty::Known(QcType::Float),
                ));
            }
            FuncRef => {
                let (ptr, _) = self.pop()?;
                let f = self.func_ref(&ptr)?;
                self.stack.push((f, Ty::Known(QcType::Func)));
            }
            FieldRefString => {
                let (offset, _) = self.pop()?;
                let name = self.field_name(&offset, QcType::String)?;
                self.stack
                    .push((Expr::Global(name), Ty::Known(QcType::StringField)));
            }
            NotFn | NotVector => {
                let (a, _) = self.pop()?;
                self.stack
                    .push((Expr::Un(UnOp::Not, Box::new(a)), Ty::Known(QcType::Float)));
            }
            NotString | NotEntity => {
                let qt = if intr == NotString {
                    QcType::String
                } else {
                    QcType::Entity
                };
                let (a, ta) = self.pop()?;
                self.unify(Ty::Known(qt), ta, &name);
                self.stack
                    .push((Expr::Un(UnOp::Not, Box::new(a)), Ty::Known(QcType::Float)));
            }
        }
        Ok(())
    }
}

/// `cs`, sorted and deduplicated.
fn const_set(cs: impl IntoIterator<Item = f32>) -> Vec<f32> {
    let mut cs: Vec<f32> = cs.into_iter().collect();
    cs.sort_by(f32::total_cmp);
    cs.dedup();
    cs
}

/// The state after a merge of paths with states `a` and `b`: a local may
/// hold any constant it may hold on either path.
fn meet(a: &Known, b: &Known) -> Known {
    a.iter()
        .filter_map(|(n, x)| {
            let cs = const_set(x.iter().chain(b.get(n)?).copied());
            (cs.len() <= MAX_KNOWN).then(|| (n.clone(), cs))
        })
        .collect()
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

fn lower_binop(op: BinaryOp, a: Expr, b: Expr) -> Result<Expr> {
    use BinaryOp::*;
    let qop = match op {
        // Integers are represented as floats; wrapping is not emulated.
        I32Add | F32Add => BinOp::Add,
        I32Sub | F32Sub => BinOp::Sub,
        I32Mul | F32Mul => BinOp::Mul,
        F32Div => BinOp::Div,
        I32Eq | F32Eq => BinOp::Eq,
        I32Ne | F32Ne => BinOp::Ne,
        I32LtS | I32LtU | F32Lt => BinOp::Lt,
        I32GtS | I32GtU | F32Gt => BinOp::Gt,
        I32LeS | I32LeU | F32Le => BinOp::Le,
        I32GeS | I32GeU | F32Ge => BinOp::Ge,
        I32And => BinOp::BitAnd,
        I32Or => BinOp::BitOr,
        // `x << k` for a constant `k` is `x * 2^k`.
        I32Shl => match b {
            Expr::Float(k) if k.fract() == 0.0 && (0.0..32.0).contains(&k) => {
                return Ok(fold(BinOp::Mul, a, Expr::Float((1u32 << k as u32) as f32)));
            }
            _ => bail!("unsupported shift by a non-constant amount"),
        },
        other => bail!("unsupported binary operator {other:?}"),
    };
    Ok(fold(qop, a, b))
}

/// Build `a op b`, folding it if both operands are constants (as QuakeC
/// would compute it at runtime). Folding matters for static data: addresses
/// like `__memory_base + 16` must become constants.
///
/// Arithmetic with a constant and a `select` between constants is pushed
/// into the `select`, so that `base + (c ? 5 : 9)` (which LLVM produces when
/// it merges two calls taking different string literals) becomes
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

fn lower_unop(op: UnaryOp, a: Expr) -> Result<Expr> {
    use UnaryOp::*;
    Ok(match op {
        I32Eqz => Expr::Un(UnOp::Not, Box::new(a)),
        F32Neg => Expr::Un(UnOp::Neg, Box::new(a)),
        // Integers are already floats.
        F32ConvertSI32 | F32ConvertUI32 => a,
        other => bail!("unsupported unary operator {other:?}"),
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

/// Locals whose every write is `global.get` of one of `bases` (always the
/// same one) immediately followed by `local.set`/`local.tee`, mapped to that
/// base's value. LLVM often keeps `__memory_base` or `__table_base` in a local
/// when it computes several addresses; treating reads of it as a constant
/// keeps those addresses constant.
fn base_locals(
    func: &LocalFunction,
    bases: &[(Option<walrus::GlobalId>, f32)],
    params: &HashMap<LocalId, (String, Ty)>,
) -> HashMap<LocalId, f32> {
    let mut good: HashMap<LocalId, f32> = HashMap::new();
    let mut bad = HashSet::new();
    let mut seqs = vec![func.entry_block()];
    while let Some(seq) = seqs.pop() {
        let mut prev: Option<f32> = None;
        for (instr, _) in &func.block(seq).instrs {
            match instr {
                Instr::LocalSet(ir::LocalSet { local })
                | Instr::LocalTee(ir::LocalTee { local }) => match prev {
                    Some(v) if good.get(local).is_none_or(|&g| g == v) => {
                        good.insert(*local, v);
                    }
                    _ => {
                        bad.insert(*local);
                    }
                },
                Instr::Block(ir::Block { seq }) | Instr::Loop(ir::Loop { seq }) => seqs.push(*seq),
                Instr::IfElse(ir::IfElse {
                    consequent,
                    alternative,
                }) => seqs.extend([*consequent, *alternative]),
                _ => {}
            }
            prev = match instr {
                Instr::GlobalGet(ir::GlobalGet { global }) => bases
                    .iter()
                    .find(|(b, _)| *b == Some(*global))
                    .map(|&(_, v)| v),
                _ => None,
            };
        }
    }
    good.retain(|l, _| !bad.contains(l) && !params.contains_key(l));
    good
}
