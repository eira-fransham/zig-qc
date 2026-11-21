//! Lowering of a single wasm function to a QuakeC function.
//!
//! The wasm operand stack is simulated symbolically: each entry is a QuakeC
//! expression. Entries are "spilled" into temporaries whenever something
//! could change the value they would compute (a call, store, local write, or
//! control flow). Calls are always assigned to a temporary immediately, since
//! QuakeC return values live in `OFS_RETURN` and are clobbered by later calls.

use std::collections::HashMap;

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use walrus::ir::{self, BinaryOp, Instr, InstrSeqId, InstrSeqType, LoadKind, StoreKind, UnaryOp, Value};
use walrus::{FunctionId, LocalFunction, LocalId};

use crate::qc::{BinOp, Expr, Function, QcType, Sig, Stmt, Ty, UnOp};
use crate::translate::{Ctx, FuncKind, Intrinsic};

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
}

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
    };

    let entry = func.entry_block();
    l.labels.push(Label { name: String::new(), kind: LabelKind::Return, result: None, used: false });
    l.label_of.insert(entry, 0);
    let live = l.lower_seq(entry)?;
    if live && l.ret != Ty::Known(QcType::Void) {
        let (v, t) = l.pop()?;
        l.unify(l.ret, t, "return value");
        l.emit(Stmt::Return(Some(v)));
    }

    let mut all_locals = l.local_decls;
    all_locals.extend(l.temps);
    Ok(Function { name, params, ret: sig.ret, locals: all_locals, body: l.out })
}

impl Lower<'_, '_> {
    fn emit(&mut self, s: Stmt) {
        self.last_call_temp = None;
        self.out.push(s);
    }

    fn unify(&mut self, a: Ty, b: Ty, what: &str) {
        self.ctx.tv.unify(a, b, what);
    }

    fn pop(&mut self) -> Result<(Expr, Ty)> {
        self.stack.pop().ok_or_else(|| anyhow!("wasm operand stack underflow"))
    }

    fn pop_n(&mut self, n: usize) -> Result<Vec<(Expr, Ty)>> {
        ensure!(self.stack.len() >= n, "wasm operand stack underflow");
        Ok(self.stack.split_off(self.stack.len() - n))
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

    /// Resolve a constant address into a QuakeC global.
    fn global_at(&self, addr: &Expr, offset: u64, expect: QcType) -> Result<String> {
        let base = Self::const_i32(addr, "memory address")?;
        let addr = base as i64 + offset as i64;
        ensure!(addr >= 0, "negative memory address {addr}");
        let slot = self
            .ctx
            .globals
            .by_addr(addr as u64)
            .ok_or_else(|| anyhow!("memory address {addr} (slot {}) is not a known QuakeC global", addr / 4))?;
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
        let mut stmts = Vec::new();
        match self.labels[idx].kind {
            LabelKind::Return => {
                if self.ret == Ty::Known(QcType::Void) {
                    stmts.push(Stmt::Return(None));
                } else {
                    let (v, t) = self.stack.last().cloned().ok_or_else(|| anyhow!("stack underflow"))?;
                    self.unify(self.ret, t, "return value");
                    stmts.push(Stmt::Return(Some(v)));
                }
            }
            LabelKind::End => {
                if let Some((tmp, ty)) = self.labels[idx].result.clone() {
                    let (v, t) = self.stack.last().cloned().ok_or_else(|| anyhow!("stack underflow"))?;
                    self.unify(ty, t, "block result");
                    stmts.push(Stmt::Assign(Expr::Local(tmp), v));
                }
                stmts.push(Stmt::Goto(self.labels[idx].name.clone()));
            }
            LabelKind::Loop => stmts.push(Stmt::Goto(self.labels[idx].name.clone())),
        }
        Ok(stmts)
    }

    /// Lower a sequence (block body) in place. Returns whether control can
    /// fall off the end.
    fn lower_seq(&mut self, seq: InstrSeqId) -> Result<bool> {
        let func = self.func;
        for (instr, _) in &func.block(seq).instrs {
            if !self.lower_instr(instr).with_context(|| format!("at instruction {instr:?}"))? {
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
    fn finish_block(&mut self, height: usize, live: bool, result: &Option<(String, Ty)>) -> Result<()> {
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
                self.labels.push(Label { name: name.clone(), kind: LabelKind::End, result: result.clone(), used: false });
                self.label_of.insert(*seq, idx);
                let height = self.stack.len();
                let (body, live) = self.lower_nested(*seq)?;
                self.out.extend(body);
                self.finish_block(height, live, &result)?;
                if self.labels[idx].used {
                    self.emit(Stmt::Label(name));
                }
                if let Some((tmp, ty)) = result {
                    self.stack.push((Expr::Local(tmp), ty));
                }
                Ok(live || self.labels[idx].used)
            }
            Instr::Loop(ir::Loop { seq }) => {
                self.spill_all();
                let name = self.new_label("loop");
                let idx = self.labels.len();
                self.labels.push(Label { name: name.clone(), kind: LabelKind::Loop, result: None, used: false });
                self.label_of.insert(*seq, idx);
                let height = self.stack.len();
                let (body, live) = self.lower_nested(*seq)?;
                if self.labels[idx].used {
                    self.emit(Stmt::Label(name));
                }
                self.out.extend(body);
                // A loop's result (if any) is left on the stack by its body.
                let extra = if live && !matches!(self.func.block(*seq).ty, InstrSeqType::Simple(None)) { 1 } else { 0 };
                self.stack.truncate(height + extra.min(self.stack.len() - height));
                Ok(live)
            }
            Instr::IfElse(ir::IfElse { consequent, alternative }) => {
                let (cond, _) = self.pop()?;
                self.spill_all();
                let result = self.block_result(*consequent)?;
                let name = self.new_label("endif");
                let idx = self.labels.len();
                self.labels.push(Label { name: name.clone(), kind: LabelKind::End, result: result.clone(), used: false });
                self.label_of.insert(*consequent, idx);
                self.label_of.insert(*alternative, idx);
                let height = self.stack.len();

                let (mut then_body, then_live) = self.lower_nested(*consequent)?;
                std::mem::swap(&mut self.out, &mut then_body);
                self.finish_block(height, then_live, &result)?;
                std::mem::swap(&mut self.out, &mut then_body);

                let (mut else_body, else_live) = self.lower_nested(*alternative)?;
                std::mem::swap(&mut self.out, &mut else_body);
                self.finish_block(height, else_live, &result)?;
                std::mem::swap(&mut self.out, &mut else_body);

                self.emit(Stmt::If(cond, then_body, else_body));
                if self.labels[idx].used {
                    self.emit(Stmt::Label(name));
                }
                if let Some((tmp, ty)) = result {
                    self.stack.push((Expr::Local(tmp), ty));
                }
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
                    let cond = Expr::Bin(BinOp::Eq, Box::new(idx.clone()), Box::new(Expr::Float(i as f32)));
                    self.emit(Stmt::If(cond, stmts, vec![]));
                }
                let stmts = self.branch(*default)?;
                for s in stmts {
                    self.emit(s);
                }
                Ok(false)
            }
            Instr::Return(_) => {
                if self.ret == Ty::Known(QcType::Void) {
                    self.emit(Stmt::Return(None));
                } else {
                    let (v, t) = self.pop()?;
                    self.unify(self.ret, t, "return value");
                    self.emit(Stmt::Return(Some(v)));
                }
                Ok(false)
            }
            Instr::Unreachable(_) => {
                self.emit(Stmt::Comment("unreachable".into()));
                let v = match self.ret {
                    Ty::Known(QcType::Void) => None,
                    Ty::Known(QcType::Float) => Some(Expr::Float(0.0)),
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
                self.stack.push((Expr::Select(Box::new(c), Box::new(a), Box::new(b)), ta));
                Ok(true)
            }
            Instr::LocalGet(ir::LocalGet { local }) => {
                let (n, t) = self.local(*local)?;
                self.stack.push((Expr::Local(n), t));
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
                let (n, t) = self.local(*local)?;
                self.stack.push((Expr::Local(n), t));
                Ok(true)
            }
            Instr::GlobalGet(ir::GlobalGet { global }) => {
                let (n, t) = self.ctx.wasm_globals.get(global).cloned().ok_or_else(|| anyhow!("unknown global"))?;
                self.stack.push((Expr::Global(n), t));
                Ok(true)
            }
            Instr::GlobalSet(ir::GlobalSet { global }) => {
                let (v, t) = self.pop()?;
                let (n, gt) = self.ctx.wasm_globals.get(global).cloned().ok_or_else(|| anyhow!("unknown global"))?;
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
                    other => bail!("unsupported constant {other:?}"),
                };
                self.stack.push((Expr::Float(f), Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::Binop(ir::Binop { op }) => {
                let (b, _) = self.pop()?;
                let (a, _) = self.pop()?;
                let e = lower_binop(*op, a, b)?;
                self.stack.push((e, Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::Unop(ir::Unop { op }) => {
                let (a, _) = self.pop()?;
                let e = lower_unop(*op, a)?;
                self.stack.push((e, Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::Call(ir::Call { func }) => {
                self.lower_call(*func)?;
                Ok(true)
            }
            Instr::CallIndirect(ir::CallIndirect { ty, table }) => {
                ensure!(Some(*table) == self.ctx.call_table, "call_indirect on unsupported table");
                let (idx, _) = self.pop()?;
                let idx = Self::const_i32(&idx, "call_indirect table index")?;
                let (f, _) = self
                    .table_slots
                    .get(&idx)
                    .cloned()
                    .ok_or_else(|| anyhow!("call_indirect through call table slot {idx}, which was not set"))?;
                let sig = self.ctx.sig_of_type(*ty)?;
                let callee = Expr::FnCast(sig.clone(), Box::new(f));
                self.call(callee, &sig, "indirect call")?;
                Ok(true)
            }
            Instr::TableSet(ir::TableSet { table }) => {
                ensure!(Some(*table) == self.ctx.call_table, "table.set on unsupported table");
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
                self.stack.push((Expr::Un(UnOp::Not, Box::new(v)), Ty::Known(QcType::Float)));
                Ok(true)
            }
            Instr::RefFunc(ir::RefFunc { func }) => {
                let name = self.ctx.funcs.get(func).map(|f| f.name.clone()).ok_or_else(|| anyhow!("unknown function"))?;
                self.stack.push((Expr::FuncName(name), Ty::Known(QcType::Func)));
                Ok(true)
            }
            Instr::Load(ir::Load { kind, arg, .. }) => {
                let (addr, _) = self.pop()?;
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

    fn lower_call(&mut self, func: FunctionId) -> Result<()> {
        let info = self.ctx.funcs.get(&func).ok_or_else(|| anyhow!("call to unknown function"))?;
        let (name, sig, kind) = (info.name.clone(), info.sig.clone(), info.kind);
        let FuncKind::Intrinsic(intr) = kind else {
            return self.call(Expr::FuncName(name.clone()), &sig, &name);
        };

        use Intrinsic::*;
        match intr {
            LoadEntity | LoadString | LoadFn => {
                let qt = match intr {
                    LoadEntity => QcType::Entity,
                    LoadString => QcType::String,
                    _ => QcType::Func,
                };
                let (addr, _) = self.pop()?;
                let g = self.global_at(&addr, 0, qt)?;
                self.stack.push((Expr::Global(g), Ty::Known(qt)));
            }
            StoreEntity | StoreString | StoreFn => {
                let qt = match intr {
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
                    Expr::FuncName(_) => Expr::FnCast(Sig { params: vec![], ret: Ty::Known(QcType::Void) }, Box::new(v)),
                    v => v,
                };
                self.spill_globals();
                self.emit(Stmt::Assign(Expr::Global(g), v));
            }
            EqString | NeString | EqEntity | NeEntity => {
                let qt = if matches!(intr, EqString | NeString) { QcType::String } else { QcType::Entity };
                let op = if matches!(intr, EqString | EqEntity) { BinOp::Eq } else { BinOp::Ne };
                let (b, tb) = self.pop()?;
                let (a, ta) = self.pop()?;
                self.unify(Ty::Known(qt), ta, &name);
                self.unify(Ty::Known(qt), tb, &name);
                self.stack.push((Expr::Bin(op, Box::new(a), Box::new(b)), Ty::Known(QcType::Float)));
            }
            NotString | NotEntity => {
                let qt = if intr == NotString { QcType::String } else { QcType::Entity };
                let (a, ta) = self.pop()?;
                self.unify(Ty::Known(qt), ta, &name);
                self.stack.push((Expr::Un(UnOp::Not, Box::new(a)), Ty::Known(QcType::Float)));
            }
        }
        Ok(())
    }
}

fn lower_binop(op: BinaryOp, a: Expr, b: Expr) -> Result<Expr> {
    use BinaryOp::*;
    let bin = |o, a, b| Expr::Bin(o, Box::new(a), Box::new(b));
    Ok(match op {
        // Integers are represented as floats; wrapping is not emulated.
        I32Add | F32Add => bin(BinOp::Add, a, b),
        I32Sub | F32Sub => bin(BinOp::Sub, a, b),
        I32Mul | F32Mul => bin(BinOp::Mul, a, b),
        F32Div => bin(BinOp::Div, a, b),
        I32Eq | F32Eq => bin(BinOp::Eq, a, b),
        I32Ne | F32Ne => bin(BinOp::Ne, a, b),
        I32LtS | I32LtU | F32Lt => bin(BinOp::Lt, a, b),
        I32GtS | I32GtU | F32Gt => bin(BinOp::Gt, a, b),
        I32LeS | I32LeU | F32Le => bin(BinOp::Le, a, b),
        I32GeS | I32GeU | F32Ge => bin(BinOp::Ge, a, b),
        I32And => bin(BinOp::BitAnd, a, b),
        I32Or => bin(BinOp::BitOr, a, b),
        other => bail!("unsupported binary operator {other:?}"),
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
