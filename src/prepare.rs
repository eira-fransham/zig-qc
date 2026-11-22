//! Rewrite a module into the form `waffle` (and the rest of the converter)
//! expects, before it is parsed into SSA.
//!
//! - `__memory_base` and `__table_base` are host-controlled, so they are
//!   replaced by the fixed values [`DATA_BASE`] and [`TABLE_BASE`] everywhere:
//!   in function bodies, data and element segment offsets (which `waffle`
//!   requires to be constants) and global initializers.
//! - `return_call` becomes `call` then `return`: QuakeC has no tail calls,
//!   and `waffle` doesn't support them.

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
            def.terminator.visit_targets(|t| *refs.entry(t.block).or_default() += 1);
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
