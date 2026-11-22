//! Emission of one function instance from SSA form to QuakeC.
//!
//! Blocks are laid out in reverse postorder, each starting with a label (kept
//! only if something jumps to it). Block parameters are QuakeC locals,
//! assigned on each incoming edge. Values with a single use in the same
//! block are inlined into that use, unless a side effect in between could
//! change what they read; other values are assigned to a local. Calls are
//! always assigned, since QuakeC return values live in `OFS_RETURN` and are
//! clobbered by later calls.

use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use waffle::entity::EntityRef;
use waffle::{Block, Func, FunctionBody, Operator, Terminator, Type, Value, ValueDef};

use crate::data::{DataKind, StaticData};
use crate::infer::{
    self, Infer, Instance, Kind, Site, TERM_COND, args, const_val, targets, term_arg, term_ret,
};
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
    };
    let f = e.function()?;
    Ok(((inst.func, i), f))
}

impl Emitter<'_, '_> {
    fn function(&mut self) -> Result<Function> {
        let body = self.body;
        self.count_uses();
        let order = rpo(body);

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

        for (i, &b) in order.iter().enumerate() {
            self.block = b;
            self.out.push(Stmt::Label(label(b)));
            for &v in &body.blocks[b].insts {
                if const_val(body, v).is_some() || const_expr(body, v).is_some() {
                    continue;
                }
                self.inst(v).with_context(|| {
                    format!("at {v} = {} (in {b})", infer::describe(body, v))
                })?;
            }
            let next = order.get(i + 1).copied();
            self.terminator(b, next)
                .with_context(|| format!("at the end of {b}"))?;
            ensure!(self.pending.is_empty(), "values left unused at the end of {b}");
        }

        // Only keep labels that are jumped to.
        let gotos = std::mem::take(&mut self.gotos);
        let out = std::mem::take(&mut self.out);
        let mut body_stmts: Vec<Stmt> = out
            .into_iter()
            .filter(|s| !matches!(s, Stmt::Label(l) if !gotos.contains(l)))
            .collect();
        if matches!(body_stmts.last(), Some(Stmt::Return(None))) {
            body_stmts.pop();
        }
        Ok(Function {
            name: self.inst.name.clone(),
            params,
            ret: sig.ret,
            locals: std::mem::take(&mut self.locals),
            body: body_stmts,
        })
    }

    fn count_uses(&mut self) {
        let body = self.body;
        let add = |uses: &mut HashMap<Value, (usize, Block)>, v: Value, b: Block| {
            let v = body.resolve_alias(v);
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
        self.out.push(s);
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
        if let Some(c) = const_val(body, v) {
            let (k, bias) = self.inf.operand_of(self.inst, body, site, idx, v)?;
            let wasm = infer::wasm_ty(body, v).unwrap_or(Type::I32);
            return self.constant(c, k, bias, wasm);
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
    fn constant(&mut self, c: i64, k: Kind, bias: i64, wasm: Type) -> Result<Expr> {
        if wasm == Type::I64 {
            ensure!(c == 0, "unsupported {k} constant {c}");
            return Ok(Expr::Null);
        }
        let addr = c.wrapping_add(bias);
        match k {
            Kind::Float => {
                if c.unsigned_abs() > MAX_EXACT_INT as u64 {
                    self.ctx.warnings.push(format!(
                        "integer constant {c} is not exactly representable as a QuakeC float"
                    ));
                }
                Ok(Expr::Float(c as f32))
            }
            Kind::Func => {
                let Some(&f) = self.ctx.table_funcs.get(&(addr as u64)) else {
                    ensure!(addr == 0, "function pointer {addr} is not in the function table");
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
            Kind::Entity | Kind::Vector => bail!("unsupported {k} constant {c}"),
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
            Expr::Select(c, a, b)
                if matches!((&*a, &*b), (Expr::Float(_), Expr::Float(_))) =>
            {
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
                ensure!(!arms.is_empty(), "no value of `{e:?}` is a QuakeC global address");
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
                    let op = if *op == O::I32Add { BinOp::Add } else { BinOp::Sub };
                    self.define(v, fold(op, a0, a1))?;
                } else {
                    ensure!(
                        x == 0 || *op == O::I32Add,
                        "{k} value computed as `{c} - x`"
                    );
                    let e = self.operand(Site::Inst(v), x as u32, a[x])?;
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
                let n = if matches!(op, O::V128Load32Zero { .. }) { 1 } else { 2 };
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
                let x = Expr::Global(self.global_at(a as i64 + memory.offset as i64, QcType::Float)?);
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
            // On a mask (see `lanewise`).
            O::V128Not => {
                let [m]: [Expr; 1] = self.operands(v)?.try_into().unwrap();
                let parts = self.vector_lanes(m).map(not);
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
                let op = if *op == O::F32x4Add { BinOp::Add } else { BinOp::Sub };
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

    fn sig_of_type(&mut self, sig: waffle::Signature, v: Value) -> Result<Sig> {
        let s = &self.ctx.module.signatures[sig];
        let default = |t: &Type| -> Result<QcType> {
            Ok(match t {
                Type::I32 | Type::F32 => QcType::Float,
                Type::I64 => QcType::Entity,
                Type::V128 => QcType::Vector,
                other => bail!("unsupported type {other:?}"),
            })
        };
        let params = s.params.iter().map(default).collect::<Result<Vec<_>>>()?;
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
        let call = Expr::Call(Box::new(Expr::FuncName(dk.accessor_name().into())), vec![addr]);
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
                let f = self.simple_name(f, QcType::FloatField);
                Expr::Field(Box::new(ent), format!("({f})"))
            }
        }
    }

    fn terminator(&mut self, b: Block, next: Option<Block>) -> Result<()> {
        let body = self.body;
        let site = Site::Term(b);
        match &body.blocks[b].terminator {
            Terminator::Br { target } => {
                let copies = self.edge(b, 0, target)?;
                self.out.extend(copies);
                self.jump(target.block, next);
            }
            Terminator::CondBr {
                cond,
                if_true,
                if_false,
            } => {
                let c = self.operand(site, TERM_COND, *cond)?;
                let t = self.edge(b, 0, if_true)?;
                let f = self.edge(b, 1, if_false)?;
                if t.is_empty() && Some(if_true.block) == next {
                    let mut f = f;
                    f.push(self.goto(if_false.block));
                    self.emit(Stmt::If(not(c), f, vec![]));
                } else {
                    let mut t = t;
                    t.push(self.goto(if_true.block));
                    self.emit(Stmt::If(c, t, vec![]));
                    self.out.extend(f);
                    self.jump(if_false.block, next);
                }
            }
            Terminator::Select {
                value,
                targets: ts,
                default,
            } => {
                let idx = self.operand(site, TERM_COND, *value)?;
                let idx = self.simple(idx, QcType::Float);
                for (i, t) in ts.iter().enumerate() {
                    let mut stmts = self.edge(b, i, t)?;
                    stmts.push(self.goto(t.block));
                    let cond = Expr::Bin(
                        BinOp::Eq,
                        Box::new(idx.clone()),
                        Box::new(Expr::Float(i as f32)),
                    );
                    self.emit(Stmt::If(cond, stmts, vec![]));
                }
                let copies = self.edge(b, ts.len(), default)?;
                self.out.extend(copies);
                self.jump(default.block, next);
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

    /// Jump to `b`, unless it comes next.
    fn jump(&mut self, b: Block, next: Option<Block>) {
        if Some(b) != next {
            let g = self.goto(b);
            self.emit(g);
        }
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
            ensure!(pre.is_empty() || k == QcType::Vector, "unexpected statements in an edge copy");
            // Vectors are materialized into temporaries (`pre`) first.
            copies.push((dest, e, k));
            self.out.extend(pre);
        }
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

    /// A lane-wise `f32x4` comparison, `pmin` or `pmax`. LLVM vectorizes runs
    /// of component comparisons (like bounding box checks). Comparison
    /// results are masks, represented as vectors with 0 or 1 in each lane
    /// (wasm uses all-ones lanes, but the masks are only ever used through
    /// `v128.not`, `v128.bitselect` and `i32x4.extract_lane`, and the code
    /// LLVM generates only tests their low bit).
    fn lanewise(&mut self, op: &Operator, a: Expr, b: Expr) -> Expr {
        let (a, b) = (self.vector_lanes(a), self.vector_lanes(b));
        let lane = |x: Expr, y: Expr| {
            let sel =
                |c: Expr, t: Expr, f: Expr| Expr::Select(Box::new(c), Box::new(t), Box::new(f));
            let cmp = |op| fold(op, x.clone(), y.clone());
            match op {
                // `pmin(a, b)` is `b < a ? b : a`, `pmax(a, b)` is `a < b ? b : a`.
                Operator::F32x4PMin => sel(fold(BinOp::Lt, y.clone(), x.clone()), y, x),
                Operator::F32x4PMax => sel(cmp(BinOp::Lt), y, x),
                Operator::F32x4Eq => cmp(BinOp::Eq),
                Operator::F32x4Ne => cmp(BinOp::Ne),
                Operator::F32x4Lt => cmp(BinOp::Lt),
                Operator::F32x4Gt => cmp(BinOp::Gt),
                Operator::F32x4Le => cmp(BinOp::Le),
                Operator::F32x4Ge => cmp(BinOp::Ge),
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
            let lane = |i: u32| f32::from_bits((value >> (32 * i)) as u32);
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

fn label(b: Block) -> String {
    format!("b{}", b.index())
}

fn not(e: Expr) -> Expr {
    Expr::Un(UnOp::Not, Box::new(e))
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
