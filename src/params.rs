//! Parameters that carry a function or a string literal.
//!
//! A wasm function pointer or string literal address must be a constant for
//! QuakeC, but one can reach its use through a parameter: a function that
//! isn't inlined, or one merged from several that differ only in those
//! constants (Binaryen's `merge-similar-functions`):
//!
//! ```wat
//! (func $set_think (param $f i32)
//!   ... (call $func_ref (i32.add (global.get $__table_base) (local.get $f))))
//! (func $a (call $set_think (i32.const 52)))
//! ```
//!
//! The same goes for entity field offsets. QuakeC functions, strings and
//! field references are values, so such a parameter is retyped as one of
//! those. The callee uses it directly, and each
//! caller converts its argument (see `lower.rs`). This module finds the
//! parameters that can be retyped: those whose every use is as a function
//! pointer, string literal address or field offset (plus a constant offset),
//! or as an argument for another such parameter.

use std::collections::{HashMap, HashSet};

use walrus::ir::{self, BinaryOp, Instr, InstrSeqId, InstrSeqType, LoadSimdKind, Value};
use walrus::{FunctionId, FunctionKind, LocalFunction, LocalId, ValType};

use crate::data::DATA_BASE;
use crate::qc::QcType;
use crate::translate::{Ctx, FuncKind, Intrinsic, TABLE_BASE};

/// What a retyped parameter holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamRepr {
    /// The function whose table index is `offset` plus the argument.
    Func { offset: i64 },
    /// The string literal at address `offset` plus the argument.
    Str { offset: i64, len: Len },
    /// The entity field of type `ty` at offset `offset` plus the argument.
    Field { offset: i64, ty: QcType },
}

/// The length of a string literal parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Len {
    Const(i64),
    /// Another parameter of the same function, only used as this length. It
    /// keeps its type; callers pass it as usual and the callee ignores it.
    Param(usize),
}

/// An abstract value on the wasm operand stack.
#[derive(Clone, Copy, PartialEq)]
enum Val {
    /// Parameter `.0` plus `.1`.
    Param(usize, i64),
    Const(i64),
    Other,
}

#[derive(Clone, Copy, PartialEq)]
enum Use {
    Func(i64),
    Str(i64, Len),
    Field(i64, QcType),
    /// Passed, plus an offset, as argument `.1` of a call to `.0`.
    Pass(FunctionId, usize, i64),
    /// The length of the string literal at parameter `.0`.
    LenOf(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    /// Not retyped: used in some other way.
    Bad,
    /// Not retyped: not used as a function or string.
    Plain,
    Pending,
    Done(ParamRepr),
}

/// The result of [`find`].
#[derive(Default)]
pub struct Retyped {
    /// The parameters to retype, by function and parameter index.
    pub reprs: HashMap<FunctionId, Vec<Option<ParamRepr>>>,
    /// Locals that are copies of a parameter plus a constant: assigned once,
    /// at the top level of the function body (so the assignment dominates
    /// every read scanned after it). If the parameter is retyped, the lowering
    /// treats them as aliases of that expression.
    pub aliases: HashMap<FunctionId, HashSet<LocalId>>,
}

/// Find the parameters to retype.
pub fn find(ctx: &Ctx<'_>, funcs: &[FunctionId]) -> Retyped {
    let mut aliases = HashMap::new();
    let exported: Vec<FunctionId> = ctx
        .module
        .exports
        .iter()
        .filter_map(|e| match e.item {
            walrus::ExportItem::Function(f) => Some(f),
            _ => None,
        })
        .collect();

    let mut uses: HashMap<FunctionId, Vec<Vec<Use>>> = HashMap::new();
    let mut state: HashMap<(FunctionId, usize), State> = HashMap::new();
    for &f in funcs {
        let FunctionKind::Local(lf) = &ctx.module.funcs.get(f).kind else {
            continue;
        };
        // Exported functions are called by the engine, and functions in the
        // table by `call_indirect`: their arguments are unknown.
        let eligible = !exported.contains(&f) && !ctx.table_funcs.values().any(|&t| t == f);
        let mut scan = Scan {
            ctx,
            params: lf.args.iter().enumerate().map(|(i, &l)| (l, i)).collect(),
            uses: vec![Vec::new(); lf.args.len()],
            bad: vec![!eligible; lf.args.len()],
            sets: count_sets(lf),
            depth: 0,
            aliases: HashMap::new(),
        };
        if scan.seq(lf, lf.entry_block()).is_none() {
            scan.bad.fill(true);
        }
        for (i, &l) in lf.args.iter().enumerate() {
            let st = if scan.bad[i] || ctx.module.locals.get(l).ty() != ValType::I32 {
                State::Bad
            } else if scan.uses[i].iter().all(|u| matches!(u, Use::LenOf(_))) {
                State::Plain
            } else {
                State::Pending
            };
            state.insert((f, i), st);
        }
        uses.insert(f, scan.uses);
        aliases.insert(f, scan.aliases.into_keys().collect());
    }

    // Resolve parameters passed to others once those are resolved.
    loop {
        let mut changed = false;
        let mut pending: Vec<_> = state
            .iter()
            .filter(|(_, s)| **s == State::Pending)
            .map(|(&k, _)| k)
            .collect();
        pending.sort_by_key(|&(f, i)| (f.index(), i));
        for (f, i) in pending {
            let new = resolve(&uses[&f], i, |g, k| state[&(g, k)]);
            if let Some(new) = new {
                state.insert((f, i), new);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut reprs: HashMap<FunctionId, Vec<Option<ParamRepr>>> = HashMap::new();
    for (&(f, i), &st) in &state {
        if let State::Done(r) = st {
            let n = uses[&f].len();
            reprs.entry(f).or_insert_with(|| vec![None; n])[i] = Some(r);
        }
    }
    Retyped { reprs, aliases }
}

/// How many times each local is assigned.
fn count_sets(lf: &LocalFunction) -> HashMap<LocalId, usize> {
    let mut sets = HashMap::new();
    let mut todo = vec![lf.entry_block()];
    while let Some(seq) = todo.pop() {
        for (instr, _) in &lf.block(seq).instrs {
            match instr {
                Instr::LocalSet(ir::LocalSet { local })
                | Instr::LocalTee(ir::LocalTee { local }) => *sets.entry(*local).or_insert(0) += 1,
                Instr::Block(ir::Block { seq }) | Instr::Loop(ir::Loop { seq }) => todo.push(*seq),
                Instr::IfElse(ir::IfElse {
                    consequent,
                    alternative,
                }) => todo.extend([*consequent, *alternative]),
                _ => {}
            }
        }
    }
    sets
}

/// The new state of the pending parameter `i`, whose function's parameters
/// have uses `uses`, or `None` if it must wait for another parameter.
fn resolve(
    uses: &[Vec<Use>],
    i: usize,
    state: impl Fn(FunctionId, usize) -> State,
) -> Option<State> {
    let mut repr = None;
    for u in &uses[i] {
        let r = match *u {
            Use::Func(offset) => ParamRepr::Func { offset },
            Use::Field(offset, ty) => ParamRepr::Field { offset, ty },
            Use::Str(offset, len) => {
                if let Len::Param(j) = len
                    && !uses[j].iter().all(|u| *u == Use::LenOf(i))
                {
                    return Some(State::Bad);
                }
                ParamRepr::Str { offset, len }
            }
            Use::Pass(g, k, o) => match state(g, k) {
                State::Done(ParamRepr::Func { offset }) => ParamRepr::Func { offset: offset + o },
                State::Done(ParamRepr::Field { offset, ty }) => ParamRepr::Field {
                    offset: offset + o,
                    ty,
                },
                State::Done(ParamRepr::Str {
                    offset,
                    len: len @ Len::Const(_),
                }) => ParamRepr::Str {
                    offset: offset + o,
                    len,
                },
                State::Pending => return None,
                _ => return Some(State::Bad),
            },
            Use::LenOf(_) => return Some(State::Bad),
        };
        if repr.is_some_and(|x| x != r) {
            return Some(State::Bad);
        }
        repr = Some(r);
    }
    Some(repr.map_or(State::Plain, State::Done))
}

/// Records how a function's parameters are used, by simulating the operand
/// stack. Anything it doesn't understand marks the values involved (or, for
/// unknown instructions, every parameter) as used in some other way.
struct Scan<'a, 'm> {
    ctx: &'a Ctx<'m>,
    params: HashMap<LocalId, usize>,
    uses: Vec<Vec<Use>>,
    bad: Vec<bool>,
    sets: HashMap<LocalId, usize>,
    /// Nesting depth of the sequence being scanned (1 for the body).
    depth: usize,
    /// Copies of parameters (see [`Retyped::aliases`]) and constants
    /// assigned so far.
    aliases: HashMap<LocalId, Val>,
}

impl Scan<'_, '_> {
    /// `v` is used in some other way.
    fn escape(&mut self, v: Val) {
        if let Val::Param(i, _) = v {
            self.bad[i] = true;
        }
    }

    fn consume(&mut self, st: &mut Vec<Val>, n: usize) {
        for _ in 0..n {
            let v = st.pop().unwrap_or(Val::Other);
            self.escape(v);
        }
    }

    fn push_n(st: &mut Vec<Val>, n: usize) {
        st.extend(std::iter::repeat_n(Val::Other, n));
    }

    fn results(lf: &LocalFunction, seq: InstrSeqId) -> Option<usize> {
        match lf.block(seq).ty {
            InstrSeqType::Simple(t) => Some(t.is_some() as usize),
            InstrSeqType::MultiValue(_) => None,
        }
    }

    /// The parameter and result counts of `ty`.
    fn arity(&self, ty: walrus::TypeId) -> (usize, usize) {
        let t = self.ctx.module.types.get(ty);
        (t.params().len(), t.results().len())
    }

    /// Scan a sequence. `None` if it contains an unsupported instruction.
    fn seq(&mut self, lf: &LocalFunction, seq: InstrSeqId) -> Option<()> {
        self.depth += 1;
        let r = self.seq_body(lf, seq);
        self.depth -= 1;
        r
    }

    fn seq_body(&mut self, lf: &LocalFunction, seq: InstrSeqId) -> Option<()> {
        let mut st = Vec::new();
        for (instr, _) in &lf.block(seq).instrs {
            if !self.instr(lf, instr, &mut st)? {
                return Some(());
            }
        }
        // Values left at the end are the block's results.
        let n = st.len();
        self.consume(&mut st, n);
        Some(())
    }

    /// Scan one instruction. `Some(false)` if control can't continue past it.
    fn instr(&mut self, lf: &LocalFunction, instr: &Instr, st: &mut Vec<Val>) -> Option<bool> {
        let pop = |st: &mut Vec<Val>| st.pop().unwrap_or(Val::Other);
        match instr {
            Instr::Const(ir::Const { value }) => st.push(match *value {
                Value::I32(c) => Val::Const(c as i64),
                _ => Val::Other,
            }),
            Instr::LocalGet(ir::LocalGet { local }) => st.push(match self.params.get(local) {
                Some(&i) => Val::Param(i, 0),
                None => self.aliases.get(local).copied().unwrap_or(Val::Other),
            }),
            Instr::LocalSet(ir::LocalSet { local }) | Instr::LocalTee(ir::LocalTee { local }) => {
                if let Some(&i) = self.params.get(local) {
                    self.bad[i] = true;
                }
                let v = pop(st);
                // Copies of constants are followed too, as the lowering
                // folds them (e.g. a local holding `__memory_base`).
                if v != Val::Other && self.depth == 1 && self.sets[local] == 1 {
                    self.aliases.insert(*local, v);
                } else {
                    self.escape(v);
                }
                if matches!(instr, Instr::LocalTee(_)) {
                    st.push(v);
                }
            }
            Instr::GlobalGet(ir::GlobalGet { global }) => {
                st.push(if Some(*global) == self.ctx.table_base {
                    Val::Const(TABLE_BASE as i64)
                } else if Some(*global) == self.ctx.memory_base {
                    Val::Const(DATA_BASE as i64)
                } else {
                    Val::Other
                })
            }
            Instr::GlobalSet(_) => self.consume(st, 1),
            Instr::Binop(ir::Binop {
                op: BinaryOp::I32Add,
            }) => {
                let (b, a) = (pop(st), pop(st));
                st.push(match (a, b) {
                    (Val::Param(i, o), Val::Const(c)) | (Val::Const(c), Val::Param(i, o)) => {
                        Val::Param(i, o + c)
                    }
                    (Val::Const(x), Val::Const(y)) => Val::Const(x.wrapping_add(y)),
                    (a, b) => {
                        self.escape(a);
                        self.escape(b);
                        Val::Other
                    }
                });
            }
            Instr::Binop(_) | Instr::I8x16Shuffle(_) | Instr::Store(_) | Instr::TableSet(_) => {
                self.consume(st, 2);
                if !matches!(instr, Instr::Store(_) | Instr::TableSet(_)) {
                    st.push(Val::Other);
                }
            }
            Instr::Unop(_)
            | Instr::Load(_)
            | Instr::RefIsNull(_)
            | Instr::TableGet(_)
            | Instr::MemoryGrow(_) => {
                self.consume(st, 1);
                st.push(Val::Other);
            }
            Instr::LoadSimd(ir::LoadSimd { kind, .. }) => match kind {
                LoadSimdKind::V128Load8Lane(_)
                | LoadSimdKind::V128Load16Lane(_)
                | LoadSimdKind::V128Load32Lane(_)
                | LoadSimdKind::V128Load64Lane(_)
                | LoadSimdKind::V128Store8Lane(_)
                | LoadSimdKind::V128Store16Lane(_)
                | LoadSimdKind::V128Store32Lane(_)
                | LoadSimdKind::V128Store64Lane(_) => return None,
                _ => {
                    self.consume(st, 1);
                    st.push(Val::Other);
                }
            },
            Instr::Select(_) | Instr::V128Bitselect(_) => {
                self.consume(st, 3);
                st.push(Val::Other);
            }
            Instr::RefFunc(_) | Instr::RefNull(_) | Instr::MemorySize(_) => st.push(Val::Other),
            Instr::Drop(_) => {
                pop(st);
            }
            Instr::Call(ir::Call { func }) | Instr::ReturnCall(ir::ReturnCall { func }) => {
                self.call(*func, st);
                if matches!(instr, Instr::ReturnCall(_)) {
                    return Some(false);
                }
            }
            Instr::CallIndirect(ir::CallIndirect { ty, .. })
            | Instr::ReturnCallIndirect(ir::ReturnCallIndirect { ty, .. }) => {
                let (params, results) = self.arity(*ty);
                self.consume(st, params + 1);
                Self::push_n(st, results);
                if matches!(instr, Instr::ReturnCallIndirect(_)) {
                    return Some(false);
                }
            }
            Instr::Block(ir::Block { seq }) | Instr::Loop(ir::Loop { seq }) => {
                self.seq(lf, *seq)?;
                Self::push_n(st, Self::results(lf, *seq)?);
            }
            Instr::IfElse(ir::IfElse {
                consequent,
                alternative,
            }) => {
                self.consume(st, 1);
                self.seq(lf, *consequent)?;
                self.seq(lf, *alternative)?;
                Self::push_n(st, Self::results(lf, *consequent)?);
            }
            Instr::BrIf(_) => {
                self.consume(st, 1);
                // A branch value (if any) stays on the stack, and also goes
                // to the target.
                if let Some(&v) = st.last() {
                    self.escape(v);
                }
            }
            Instr::Br(_) | Instr::BrTable(_) | Instr::Return(_) => {
                let n = st.len();
                self.consume(st, n);
                return Some(false);
            }
            Instr::Unreachable(_) => return Some(false),
            _ => return None,
        }
        Some(true)
    }

    fn call(&mut self, func: FunctionId, st: &mut Vec<Val>) {
        let (params, results) = self.arity(self.ctx.module.funcs.get(func).ty());
        let kind = self.ctx.funcs.get(&func).map(|f| f.kind);
        match kind {
            Some(FuncKind::Intrinsic(Intrinsic::FuncRef)) => match st.pop() {
                Some(Val::Param(i, o)) => self.uses[i].push(Use::Func(o)),
                v => self.escape(v.unwrap_or(Val::Other)),
            },
            Some(FuncKind::Intrinsic(Intrinsic::StringLiteral)) => {
                let len = st.pop().unwrap_or(Val::Other);
                let ptr = st.pop().unwrap_or(Val::Other);
                match (ptr, len) {
                    (Val::Param(i, o), Val::Const(l)) => {
                        self.uses[i].push(Use::Str(o, Len::Const(l)))
                    }
                    (Val::Param(i, o), Val::Param(j, 0)) if i != j => {
                        self.uses[i].push(Use::Str(o, Len::Param(j)));
                        self.uses[j].push(Use::LenOf(i));
                    }
                    (ptr, len) => {
                        self.escape(ptr);
                        self.escape(len);
                    }
                }
            }
            Some(FuncKind::Intrinsic(
                intr @ (Intrinsic::LoadField(ty) | Intrinsic::StoreField(ty)),
            )) => {
                if matches!(intr, Intrinsic::StoreField(_)) {
                    self.consume(st, 1);
                }
                match st.pop() {
                    Some(Val::Param(i, o)) => self.uses[i].push(Use::Field(o, ty)),
                    v => self.escape(v.unwrap_or(Val::Other)),
                }
                self.consume(st, 1);
            }
            Some(FuncKind::Local) => {
                let at = st.len().saturating_sub(params);
                let args = st.split_off(at);
                for (k, v) in args.into_iter().enumerate() {
                    if let Val::Param(i, o) = v {
                        self.uses[i].push(Use::Pass(func, k, o));
                    }
                }
            }
            _ => self.consume(st, params),
        }
        Self::push_n(st, results);
    }
}
