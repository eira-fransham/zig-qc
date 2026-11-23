//! Rewrite a module into the form `waffle` (and the rest of the converter)
//! expects, before it is parsed into SSA.
//!
//! - `__memory_base` and `__table_base` are host-controlled, so they are
//!   replaced by the fixed values [`DATA_BASE`] and [`TABLE_BASE`] everywhere:
//!   in function bodies, data and element segment offsets (which `waffle`
//!   requires to be constants) and global initializers.
//! - `return_call` becomes `call` then `return`: QuakeC has no tail calls,
//!   and `waffle` doesn't support them.
//! - A function exported under several names (LLVM merges identical
//!   functions) gets a wrapper for each name after the first, so that each
//!   becomes a QuakeC function (with its own types).

use anyhow::{Context as _, Result};
use walrus::ir::{self, Instr, InstrSeqId, Value};
use walrus::{ConstExpr, ConstOp, FunctionKind, GlobalId, GlobalKind, ImportKind, Module};

use crate::data::DATA_BASE;
use crate::translate::{ENV_MODULE, TABLE_BASE};

/// A module rewritten by [`prepare`].
pub struct Prepared {
    pub wasm: Vec<u8>,
    /// The original index of each function in `wasm`. walrus reorders
    /// local functions (largest first); output keeps the original order and
    /// names.
    pub original_index: Vec<u32>,
}

/// Prepare the module `wasm`, returning the rewritten bytes.
pub fn prepare(wasm: &[u8]) -> Result<Prepared> {
    let mut module = Module::from_buffer(wasm).context("failed to parse wasm module")?;

    let mut bases: Vec<(GlobalId, i32)> = Vec::new();
    for import in module.imports.iter() {
        if let ImportKind::Global(g) = import.kind
            && import.module == ENV_MODULE
        {
            match import.name.as_str() {
                "__memory_base" => bases.push((g, DATA_BASE as i32)),
                "__table_base" => bases.push((g, TABLE_BASE as i32)),
                _ => {}
            }
        }
    }
    let base_of = |g: GlobalId| bases.iter().find(|(b, _)| *b == g).map(|&(_, v)| v);

    for func in module.funcs.iter_mut() {
        let FunctionKind::Local(lf) = &mut func.kind else {
            continue;
        };
        let mut todo = vec![lf.entry_block()];
        while let Some(seq) = todo.pop() {
            todo.extend(rewrite_seq(lf, seq, &base_of));
        }
    }

    let ids: Vec<_> = module.data.iter().map(|d| d.id()).collect();
    for id in ids {
        if let walrus::DataKind::Active { offset, .. } = &mut module.data.get_mut(id).kind
            && let Some(v) = eval(offset, &base_of)
        {
            *offset = ConstExpr::Value(Value::I32(v));
        }
    }
    for el in module.elements.iter_mut() {
        if let walrus::ElementKind::Active { offset, .. } = &mut el.kind
            && let Some(v) = eval(offset, &base_of)
        {
            *offset = ConstExpr::Value(Value::I32(v));
        }
    }
    let ids: Vec<_> = module.globals.iter().map(|g| g.id()).collect();
    for id in ids {
        if let GlobalKind::Local(init) = &mut module.globals.get_mut(id).kind
            && !matches!(init, ConstExpr::Value(_))
            && let Some(v) = eval(init, &base_of)
        {
            *init = ConstExpr::Value(Value::I32(v));
        }
    }

    export_wrappers(&mut module);

    // The order walrus emits functions in: imports, then local functions by
    // decreasing size (see `used_local_functions` in walrus).
    let mut imports = Vec::new();
    let mut locals = Vec::new();
    for f in module.funcs.iter() {
        match &f.kind {
            FunctionKind::Local(l) => locals.push((f.id(), l.size())),
            _ => imports.push(f.id()),
        }
    }
    locals.sort_by_key(|&(id, size)| (std::cmp::Reverse(size), id));
    let original_index = imports
        .into_iter()
        .chain(locals.into_iter().map(|(id, _)| id))
        .map(|id| id.index() as u32)
        .collect();

    Ok(Prepared {
        wasm: module.emit_wasm(),
        original_index,
    })
}

/// Re-export each function exported more than once through a wrapper for
/// each name after its first.
fn export_wrappers(module: &mut Module) {
    let mut seen = std::collections::HashSet::new();
    let exports: Vec<_> = module
        .exports
        .iter()
        .filter_map(|e| match e.item {
            walrus::ExportItem::Function(f) => Some((e.id(), f)),
            _ => None,
        })
        .collect();
    for (export, f) in exports {
        if seen.insert(f) {
            continue;
        }
        let ty = module.types.get(module.funcs.get(f).ty()).clone();
        let mut builder =
            walrus::FunctionBuilder::new(&mut module.types, ty.params(), ty.results());
        let params: Vec<_> = ty.params().iter().map(|&t| module.locals.add(t)).collect();
        let mut body = builder.func_body();
        for &p in &params {
            body.local_get(p);
        }
        body.call(f);
        let wrapper = builder.finish(params, &mut module.funcs);
        module.exports.get_mut(export).item = walrus::ExportItem::Function(wrapper);
    }
}

/// Rewrite one instruction sequence, returning the sequences nested in it.
fn rewrite_seq(
    lf: &mut walrus::LocalFunction,
    seq: InstrSeqId,
    base_of: &impl Fn(GlobalId) -> Option<i32>,
) -> Vec<InstrSeqId> {
    let instrs = std::mem::take(&mut lf.block_mut(seq).instrs);
    let mut out = Vec::with_capacity(instrs.len());
    let mut nested = Vec::new();
    for (instr, loc) in instrs {
        match instr {
            Instr::GlobalGet(ir::GlobalGet { global }) if base_of(global).is_some() => {
                let value = Value::I32(base_of(global).unwrap());
                out.push((Instr::Const(ir::Const { value }), loc));
            }
            Instr::ReturnCall(ir::ReturnCall { func }) => {
                out.push((Instr::Call(ir::Call { func }), loc));
                out.push((Instr::Return(ir::Return {}), loc));
            }
            Instr::ReturnCallIndirect(ir::ReturnCallIndirect { ty, table }) => {
                out.push((Instr::CallIndirect(ir::CallIndirect { ty, table }), loc));
                out.push((Instr::Return(ir::Return {}), loc));
            }
            instr => {
                match &instr {
                    Instr::Block(ir::Block { seq }) | Instr::Loop(ir::Loop { seq }) => {
                        nested.push(*seq)
                    }
                    Instr::IfElse(ir::IfElse {
                        consequent,
                        alternative,
                    }) => nested.extend([*consequent, *alternative]),
                    _ => {}
                }
                out.push((instr, loc));
            }
        }
    }
    lf.block_mut(seq).instrs = out;
    nested
}

/// Evaluate a constant expression that may use `__memory_base` or
/// `__table_base`.
fn eval(e: &ConstExpr, base_of: &impl Fn(GlobalId) -> Option<i32>) -> Option<i32> {
    match e {
        ConstExpr::Value(Value::I32(v)) => Some(*v),
        ConstExpr::Global(g) => base_of(*g),
        ConstExpr::Extended(ops) => {
            let mut stack: Vec<i32> = Vec::new();
            for op in ops {
                match op {
                    ConstOp::I32Const(c) => stack.push(*c),
                    ConstOp::GlobalGet(g) => stack.push(base_of(*g)?),
                    ConstOp::I32Add | ConstOp::I32Sub | ConstOp::I32Mul => {
                        let (b, a) = (stack.pop()?, stack.pop()?);
                        stack.push(match op {
                            ConstOp::I32Add => a.wrapping_add(b),
                            ConstOp::I32Sub => a.wrapping_sub(b),
                            _ => a.wrapping_mul(b),
                        });
                    }
                    _ => return None,
                }
            }
            match stack.as_slice() {
                [v] => Some(*v),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Remove block parameters that always receive the same value (or
/// themselves, around a loop): most wasm locals are just carried through
/// blocks unchanged. Each becomes an alias of that value, which dominates
/// the block.
pub fn simplify_params(body: &mut waffle::FunctionBody) {
    use waffle::ValueDef;
    loop {
        let mut changed = false;
        let blocks: Vec<_> = body.blocks.iter().filter(|&b| b != body.entry).collect();
        for b in blocks {
            let mut i = body.blocks[b].params.len();
            while i > 0 {
                i -= 1;
                let p = body.blocks[b].params[i].1;
                let mut unique = None;
                let mut trivial = true;
                for (_, def) in body.blocks.entries() {
                    def.terminator.visit_targets(|t| {
                        if t.block != b {
                            return;
                        }
                        let a = body.resolve_alias(t.args[i]);
                        if a == p {
                            return;
                        }
                        match unique {
                            None => unique = Some(a),
                            Some(u) if u == a => {}
                            Some(_) => trivial = false,
                        }
                    });
                }
                let (true, Some(x)) = (trivial, unique) else {
                    continue;
                };
                body.blocks[b].params.remove(i);
                for (_, def) in body.blocks.entries_mut() {
                    def.terminator.update_targets(|t| {
                        if t.block == b {
                            t.args.remove(i);
                        }
                    });
                }
                body.set_alias(p, x);
                // Renumber the later parameters.
                for (j, &(ty, v)) in body.blocks[b].params.iter().enumerate().skip(i) {
                    body.values[v] = ValueDef::BlockParam(b, j as u32, ty);
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Merge each block that is only reached by an unconditional branch into
/// the block it comes from, so that values can be used across them without
/// a local. (waffle's own `merge_blocks` pass is buggy in 0.4.)
pub fn merge_blocks(body: &mut waffle::FunctionBody) {
    use waffle::Terminator;
    loop {
        let mut refs: std::collections::HashMap<waffle::Block, usize> = Default::default();
        for (_, def) in body.blocks.entries() {
            def.terminator
                .visit_targets(|t| *refs.entry(t.block).or_default() += 1);
        }
        let mut changed = false;
        let blocks: Vec<_> = body.blocks.iter().collect();
        for b in blocks {
            let Terminator::Br { target } = &body.blocks[b].terminator else {
                continue;
            };
            let t = target.block;
            if t == b || t == body.entry || refs.get(&t) != Some(&1) {
                continue;
            }
            let args = target.args.clone();
            let params = std::mem::take(&mut body.blocks[t].params);
            for (&(_, p), a) in params.iter().zip(args) {
                body.set_alias(p, a);
            }
            let insts = std::mem::take(&mut body.blocks[t].insts);
            let term = std::mem::replace(&mut body.blocks[t].terminator, Terminator::None);
            body.blocks[b].insts.extend(insts);
            body.blocks[b].terminator = term;
            changed = true;
            // `refs` is stale now (`b` took over `t`'s branches).
            break;
        }
        if !changed {
            break;
        }
    }
}

/// Rewrite `k + b`, `b + k` and `k - b` for a constant `k` and a comparison
/// result `b` (0 or 1) into a `select` between two constants. LLVM computes
/// `c ? f : g` this way when `f` and `g` are adjacent (e.g. function pointers
/// `__table_base + (x != 0)`); as a `select`, each constant gets its own
/// type from its use, rather than being arithmetic on a number.
pub fn bool_offsets_to_select(body: &mut waffle::FunctionBody) {
    use waffle::{Operator as O, Type, ValueDef};
    let is_bool = |body: &waffle::FunctionBody, v: waffle::Value| {
        matches!(
            body.values[body.resolve_alias(v)],
            ValueDef::Operator(
                O::I32Eqz
                    | O::I64Eqz
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
                    | O::I64Eq
                    | O::I64Ne
                    | O::I64LtS
                    | O::I64LtU
                    | O::I64GtS
                    | O::I64GtU
                    | O::I64LeS
                    | O::I64LeU
                    | O::I64GeS
                    | O::I64GeU
                    | O::F32Eq
                    | O::F32Ne
                    | O::F32Lt
                    | O::F32Gt
                    | O::F32Le
                    | O::F32Ge,
                _,
                _
            )
        )
    };
    let values: Vec<_> = body.values.iter().collect();
    for v in values {
        let ValueDef::Operator(op @ (O::I32Add | O::I32Sub), args, tys) = body.values[v] else {
            continue;
        };
        let [a, b] = body.arg_pool[args] else {
            continue;
        };
        let (k, cond, delta) = match (
            crate::infer::const_val(body, a),
            crate::infer::const_val(body, b),
        ) {
            (Some(k), None) if is_bool(body, b) => (k, b, if op == O::I32Add { 1 } else { -1 }),
            (None, Some(k)) if op == O::I32Add && is_bool(body, a) => (k, a, 1),
            _ => continue,
        };
        let ty = body.single_type_list(Type::I32);
        let konst = |body: &mut waffle::FunctionBody, c: i64| {
            body.add_value(ValueDef::Operator(
                O::I32Const { value: c as u32 },
                Default::default(),
                ty,
            ))
        };
        let hi = konst(body, k + delta);
        let lo = konst(body, k);
        let args = body.arg_pool.from_iter([hi, lo, cond].into_iter());
        body.values[v] = ValueDef::Operator(O::Select, args, tys);
    }
}

/// Rewrite sign tests of floats' bits, which Zig's `@min`/`@max` use for
/// signed zeros: `reinterpret(x) < 0` becomes `x < 0` (and `>= 0` likewise),
/// and `(reinterpret(a) ^ reinterpret(b)) < 0` (whether the signs differ)
/// becomes `(a < 0) != (b < 0)`. QuakeC can't see a float's bits; the only
/// difference is that -0.0 counts as positive. Bit reinterpretations left
/// unused are removed.
pub fn float_sign_tests(body: &mut waffle::FunctionBody) {
    use waffle::{Operator as O, Type, Value, ValueDef};
    let op_of = |body: &waffle::FunctionBody, v: Value| match &body.values[body.resolve_alias(v)] {
        ValueDef::Operator(op, args, _) => Some((*op, body.arg_pool[*args].to_vec())),
        _ => None,
    };
    let reinterpreted = |body: &waffle::FunctionBody, v: Value| match op_of(body, v) {
        Some((O::I32ReinterpretF32, a)) => Some(a[0]),
        _ => None,
    };
    let blocks: Vec<_> = body.blocks.iter().collect();
    let mut changed = false;
    for b in blocks {
        let mut i = 0;
        while i < body.blocks[b].insts.len() {
            let v = body.blocks[b].insts[i];
            i += 1;
            let Some((op @ (O::I32LtS | O::I32GeS), a)) = op_of(body, v) else {
                continue;
            };
            if crate::infer::const_val(body, a[1]) != Some(0) {
                continue;
            }
            let i32_ty = body.single_type_list(Type::I32);
            let f32_ty = body.single_type_list(Type::F32);
            let zero = body.add_value(ValueDef::Operator(
                O::F32Const { value: 0 },
                Default::default(),
                f32_ty,
            ));
            let cmp = if op == O::I32LtS { O::F32Lt } else { O::F32Ge };
            let new = if let Some(x) = reinterpreted(body, a[0]) {
                (cmp, vec![x, zero])
            } else if let Some((O::I32Xor, xs)) = op_of(body, a[0])
                && let (Some(x), Some(y)) = (reinterpreted(body, xs[0]), reinterpreted(body, xs[1]))
            {
                // Insert `x < 0` and `y < 0` before `v`.
                let mut sign = |body: &mut waffle::FunctionBody, f: Value| {
                    let args = body.arg_pool.from_iter([f, zero].into_iter());
                    let s = body.add_value(ValueDef::Operator(O::F32Lt, args, i32_ty));
                    body.blocks[b].insts.insert(i - 1, s);
                    i += 1;
                    s
                };
                let (sx, sy) = (sign(body, x), sign(body, y));
                let ne = if op == O::I32LtS { O::I32Ne } else { O::I32Eq };
                (ne, vec![sx, sy])
            } else {
                continue;
            };
            let args = body.arg_pool.from_iter(new.1.into_iter());
            body.values[v] = ValueDef::Operator(new.0, args, i32_ty);
            // The constant is declared by being in a block (its position
            // doesn't matter).
            body.blocks[b].insts.insert(i - 1, zero);
            i += 1;
            changed = true;
        }
    }
    if !changed {
        return;
    }
    // Remove the reinterpretations (and xors of them) now unused.
    remove_dead(body, |op| matches!(op, O::I32ReinterpretF32 | O::I32Xor));
}

/// Remove the instructions with an operator `pure` accepts whose results
/// are unused, and the block parameters that are only passed on to unused
/// parameters (including round a loop).
pub fn remove_dead(body: &mut waffle::FunctionBody, pure: impl Fn(&waffle::Operator) -> bool) {
    use waffle::{Value, ValueDef};
    let dead_op = |body: &waffle::FunctionBody, v: Value| matches!(&body.values[v], ValueDef::Operator(op, _, _) if pure(op));
    // Mark the live values, from the uses by everything else.
    let mut live: std::collections::HashSet<Value> = Default::default();
    let mut work: Vec<Value> = Vec::new();
    let mark = |v: Value, live: &mut std::collections::HashSet<Value>, work: &mut Vec<Value>| {
        let v = body.resolve_alias(v);
        if live.insert(v) {
            work.push(v);
        }
    };
    // Incoming branch arguments of each parameter.
    let mut incoming: std::collections::HashMap<Value, Vec<Value>> = Default::default();
    for (_, def) in body.blocks.entries() {
        for &v in &def.insts {
            if !dead_op(body, v) {
                mark(v, &mut live, &mut work);
            }
        }
        let mut cond_uses = Vec::new();
        match &def.terminator {
            waffle::Terminator::CondBr { cond, .. } => cond_uses.push(*cond),
            waffle::Terminator::Select { value, .. } => cond_uses.push(*value),
            waffle::Terminator::Return { values } => cond_uses.extend(values.iter().copied()),
            _ => {}
        }
        for v in cond_uses {
            mark(v, &mut live, &mut work);
        }
        def.terminator.visit_targets(|t| {
            for (&a, &(_, p)) in t.args.iter().zip(&body.blocks[t.block].params) {
                incoming.entry(p).or_default().push(a);
            }
        });
    }
    // The entry block's parameters are the function's.
    for &(_, p) in &body.blocks[body.entry].params {
        mark(p, &mut live, &mut work);
    }
    while let Some(v) = work.pop() {
        if let ValueDef::Operator(_, args, _) = &body.values[v] {
            for &a in &body.arg_pool[*args] {
                mark(a, &mut live, &mut work);
            }
        }
        for &a in incoming.get(&v).into_iter().flatten() {
            mark(a, &mut live, &mut work);
        }
    }
    // Sweep.
    for (_, def) in body.blocks.entries_mut() {
        def.insts.retain(|&v| live.contains(&v));
    }
    let blocks: Vec<_> = body.blocks.iter().filter(|&b| b != body.entry).collect();
    for b in blocks {
        let keep: Vec<bool> = body.blocks[b]
            .params
            .iter()
            .map(|(_, p)| live.contains(p))
            .collect();
        if keep.iter().all(|&k| k) {
            continue;
        }
        let params = std::mem::take(&mut body.blocks[b].params);
        body.blocks[b].params = params
            .into_iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(p, _)| p)
            .collect();
        for (j, &(ty, v)) in body.blocks[b].params.iter().enumerate() {
            body.values[v] = ValueDef::BlockParam(b, j as u32, ty);
        }
        for (_, def) in body.blocks.entries_mut() {
            def.terminator.update_targets(|t| {
                if t.block == b {
                    let mut k = keep.iter();
                    t.args.retain(|_| *k.next().unwrap());
                }
            });
        }
    }
}

/// Split 64-bit memory moves into pairs of 32-bit ones. LLVM merges two
/// adjacent 4-byte copies or constant stores (e.g. of two entity fields)
/// into one `i64` access, but QuakeC has no 64-bit values:
/// - `i64.store(p, k)` of a constant becomes `i32.store`s of its halves;
/// - an `i64.load` only used as the value of `i64.store`s becomes two
///   `i32.load`s (in its place), and each store two `i32.store`s of them.
///
/// Other `i64` values are left alone, and rejected later.
pub fn split_i64_moves(body: &mut waffle::FunctionBody) {
    use waffle::{MemoryArg, Operator as O, Type, Value, ValueDef};
    let op_of = |body: &waffle::FunctionBody, v: Value| match &body.values[body.resolve_alias(v)] {
        ValueDef::Operator(op, args, _) => Some((*op, body.arg_pool[*args].to_vec())),
        _ => None,
    };
    // `i64.load`s whose every use is as a stored value.
    let mut uses: std::collections::HashMap<Value, bool> = Default::default();
    let mut used = |v: Value, as_value: bool| {
        let ok = uses.entry(body.resolve_alias(v)).or_insert(true);
        *ok &= as_value;
    };
    for (_, def) in body.blocks.entries() {
        for &v in &def.insts {
            if let Some((op, args)) = op_of(body, v) {
                for (i, &a) in args.iter().enumerate() {
                    used(a, matches!(op, O::I64Store { .. }) && i == 1);
                }
            }
        }
        def.terminator.visit_uses(|v| used(v, false));
    }
    let halves = |m: MemoryArg| {
        let align = m.align.min(2);
        [
            MemoryArg { align, ..m },
            MemoryArg {
                align,
                offset: m.offset + 4,
                ..m
            },
        ]
    };
    let i32_ty = body.single_type_list(Type::I32);
    let blocks: Vec<_> = body.blocks.iter().collect();
    // Each split load's halves.
    let mut split: std::collections::HashMap<Value, [Value; 2]> = Default::default();
    // The loads, then the stores (which may be in blocks before them).
    let phases = [false, true].into_iter();
    for (stores, b) in phases.flat_map(|s| blocks.iter().map(move |&b| (s, b))) {
        let insts = std::mem::take(&mut body.blocks[b].insts);
        let mut out = Vec::with_capacity(insts.len());
        for v in insts {
            let Some((op, args)) = op_of(body, v) else {
                out.push(v);
                continue;
            };
            match op {
                O::I64Load { memory } if !stores && uses.get(&v) == Some(&true) => {
                    let pair = halves(memory).map(|memory| {
                        let a = body.arg_pool.from_iter([args[0]].into_iter());
                        let l =
                            body.add_value(ValueDef::Operator(O::I32Load { memory }, a, i32_ty));
                        out.push(l);
                        l
                    });
                    split.insert(v, pair);
                }
                O::I64Store { memory } if stores => {
                    let x = body.resolve_alias(args[1]);
                    let values = if let Some(&pair) = split.get(&x) {
                        pair
                    } else if let Some((O::I64Const { value }, _)) = op_of(body, x) {
                        [value as u32, (value >> 32) as u32].map(|value| {
                            let k = body.add_value(ValueDef::Operator(
                                O::I32Const { value },
                                Default::default(),
                                i32_ty,
                            ));
                            out.push(k);
                            k
                        })
                    } else {
                        out.push(v);
                        continue;
                    };
                    let ValueDef::Operator(_, _, tys) = body.values[v] else {
                        unreachable!()
                    };
                    for (memory, x) in halves(memory).into_iter().zip(values) {
                        let a = body.arg_pool.from_iter([args[0], x].into_iter());
                        out.push(body.add_value(ValueDef::Operator(
                            O::I32Store { memory },
                            a,
                            tys,
                        )));
                    }
                }
                _ => out.push(v),
            }
        }
        body.blocks[b].insts = out;
    }
}

/// Rewrite `v128.load32_splat(p)` (LLVM's load of one value into every
/// lane) into `f32x4.splat(f32.load(p))`, so that the load is an ordinary
/// scalar one (of a field, a global or a stack slot). QuakeC values are all
/// floats, so the load's lanes being `i32` or `f32` doesn't matter.
pub fn split_splat_loads(body: &mut waffle::FunctionBody) {
    use waffle::{Operator as O, Type, ValueDef};
    let f32_ty = body.single_type_list(Type::F32);
    let blocks: Vec<_> = body.blocks.iter().collect();
    for b in blocks {
        let mut i = 0;
        while i < body.blocks[b].insts.len() {
            let v = body.blocks[b].insts[i];
            i += 1;
            let ValueDef::Operator(O::V128Load32Splat { memory }, args, tys) = body.values[v]
            else {
                continue;
            };
            let l = body.add_value(ValueDef::Operator(O::F32Load { memory }, args, f32_ty));
            body.blocks[b].insts.insert(i - 1, l);
            i += 1;
            let args = body.arg_pool.from_iter([l].into_iter());
            body.values[v] = ValueDef::Operator(O::F32x4Splat, args, tys);
        }
    }
}

/// Duplicate each block that accesses memory at `p + k` (or `k + p`) for a
/// parameter `k` that every predecessor passes a constant, into a copy per
/// incoming branch with `k` replaced by the constant. LLVM sinks stores to
/// different fields of an entity from both sides of a branch into one, at
/// `e + phi(off1, off2)`; QuakeC has no field pointers, so each field
/// access needs a constant offset. The block's values mustn't be used
/// outside it (except by its own branch arguments).
pub fn duplicate_offset_blocks(body: &mut waffle::FunctionBody) {
    use std::collections::{HashMap, HashSet};
    use waffle::{Block, Operator as O, Terminator, Value, ValueDef};
    // Each edge into each block: its source and arguments.
    let mut incoming: HashMap<Block, Vec<Vec<Value>>> = HashMap::new();
    for (_, def) in body.blocks.entries() {
        def.terminator
            .visit_targets(|t| incoming.entry(t.block).or_default().push(t.args.clone()));
    }
    // Where each value is used: its block (`None` for anything else).
    let mut used_in: HashMap<Value, HashSet<Block>> = HashMap::new();
    for (b, def) in body.blocks.entries() {
        for &v in &def.insts {
            body.values[v].visit_uses(&body.arg_pool, |a| {
                used_in.entry(body.resolve_alias(a)).or_default().insert(b);
            });
        }
        def.terminator.visit_uses(|a| {
            used_in.entry(body.resolve_alias(a)).or_default().insert(b);
        });
    }
    let blocks: Vec<_> = body.blocks.iter().filter(|&b| b != body.entry).collect();
    let mut changed = false;
    for b in blocks {
        let def = &body.blocks[b];
        let Some(edges) = incoming.get(&b) else {
            continue;
        };
        let constant_param = |i: usize| {
            edges.len() > 1
                && edges
                    .iter()
                    .all(|args| crate::infer::const_val(body, args[i]).is_some())
        };
        let offset_params: HashSet<Value> = def
            .params
            .iter()
            .enumerate()
            .filter(|&(i, _)| constant_param(i))
            .map(|(_, &(_, p))| p)
            .collect();
        let is_offset_address = |addr: Value| match &body.values[body.resolve_alias(addr)] {
            ValueDef::Operator(O::I32Add, args, _) => body.arg_pool[*args]
                .iter()
                .any(|&a| offset_params.contains(&body.resolve_alias(a))),
            _ => false,
        };
        let wanted = def.insts.iter().any(|&v| match &body.values[v] {
            ValueDef::Operator(op, args, _) => {
                matches!(crate::memory::cells(op), Ok(Some(_)))
                    && is_offset_address(body.arg_pool[*args][0])
            }
            _ => false,
        });
        let local = def
            .params
            .iter()
            .map(|&(_, p)| p)
            .chain(def.insts.iter().copied())
            .all(|v| used_in.get(&v).is_none_or(|bs| bs.iter().all(|&u| u == b)));
        let copyable = def.insts.iter().all(|&v| {
            matches!(
                body.values[v],
                ValueDef::Operator(..) | ValueDef::PickOutput(..)
            )
        });
        if !wanted || !local || !copyable {
            continue;
        }
        // Copy the block for each incoming edge, retargeting the edge.
        let preds: Vec<Block> = body.blocks.iter().collect();
        for pred in preds {
            let mut term = body.blocks[pred].terminator.clone();
            let mut copies = Vec::new();
            term.update_targets(|t| {
                if t.block == b {
                    copies.push(std::mem::take(&mut t.args));
                }
            });
            if copies.is_empty() {
                continue;
            }
            let mut copies = copies.into_iter();
            let mut new_blocks = Vec::new();
            for args in copies.by_ref() {
                new_blocks.push(copy_block(body, b, &args));
            }
            let mut new_blocks = new_blocks.into_iter();
            term.update_targets(|t| {
                if t.block == b {
                    t.block = new_blocks.next().unwrap();
                }
            });
            body.blocks[pred].terminator = term;
        }
        body.blocks[b].insts.clear();
        body.blocks[b].terminator = Terminator::Unreachable;
        changed = true;
    }
    if changed {
        body.recompute_edges();
    }

    /// A copy of block `b`, with its parameters replaced by `args`.
    fn copy_block(body: &mut waffle::FunctionBody, b: Block, args: &[Value]) -> Block {
        let new = body.add_block();
        let mut map: HashMap<Value, Value> = body.blocks[b]
            .params
            .iter()
            .map(|&(_, p)| p)
            .zip(args.iter().copied())
            .collect();
        for v in body.blocks[b].insts.clone() {
            let sub = |body: &waffle::FunctionBody, map: &HashMap<Value, Value>, a: Value| {
                let r = body.resolve_alias(a);
                map.get(&r).copied().unwrap_or(a)
            };
            // A fresh argument list (the original's is shared).
            let def = match body.values[v] {
                ValueDef::Operator(op, args, tys) => {
                    let args: Vec<Value> = body.arg_pool[args]
                        .iter()
                        .map(|&a| sub(body, &map, a))
                        .collect();
                    ValueDef::Operator(op, body.arg_pool.from_iter(args.into_iter()), tys)
                }
                ValueDef::PickOutput(x, i, ty) => ValueDef::PickOutput(sub(body, &map, x), i, ty),
                _ => unreachable!("only operators are copied"),
            };
            let c = body.add_value(def);
            body.append_to_block(new, c);
            map.insert(v, c);
        }
        let mut term = body.blocks[b].terminator.clone();
        term.update_uses(|a| {
            let r = body.resolve_alias(*a);
            if let Some(&m) = map.get(&r) {
                *a = m;
            }
        });
        body.blocks[new].terminator = term;
        new
    }
}

/// Rewrite `i32x4.extract_lane 2i (i64x2.extend_{low,high}_i32x4_{s,u} x)`
/// (the low half of a 64-bit lane) into `i32x4.extract_lane j x` for the
/// lane `j` it came from. LLVM widens comparison masks this way. The
/// extensions left unused are removed.
pub fn narrow_extended_lanes(body: &mut waffle::FunctionBody) {
    use waffle::{Operator as O, ValueDef};
    let mut changed = false;
    let values: Vec<_> = body.values.iter().collect();
    for v in values {
        let ValueDef::Operator(O::I32x4ExtractLane { lane }, args, tys) = body.values[v] else {
            continue;
        };
        if lane % 2 != 0 {
            continue;
        }
        let x = body.resolve_alias(body.arg_pool[args][0]);
        let ValueDef::Operator(op, xargs, _) = body.values[x] else {
            continue;
        };
        let base = match op {
            O::I64x2ExtendLowI32x4S | O::I64x2ExtendLowI32x4U => 0,
            O::I64x2ExtendHighI32x4S | O::I64x2ExtendHighI32x4U => 2,
            _ => continue,
        };
        let src = body.arg_pool[xargs][0];
        let args = body.arg_pool.from_iter([src].into_iter());
        body.values[v] = ValueDef::Operator(
            O::I32x4ExtractLane {
                lane: base + lane / 2,
            },
            args,
            tys,
        );
        changed = true;
    }
    if changed {
        remove_dead(body, |op| {
            matches!(
                op,
                O::I64x2ExtendLowI32x4S
                    | O::I64x2ExtendLowI32x4U
                    | O::I64x2ExtendHighI32x4S
                    | O::I64x2ExtendHighI32x4U
            )
        });
    }
}
