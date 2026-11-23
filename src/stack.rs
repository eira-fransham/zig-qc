//! Reifying the shadow stack into SSA values.
//!
//! LLVM keeps locals in memory when it must (volatile locals, locals whose
//! address is used, most locals in Debug builds), in a frame below
//! `__stack_pointer`. QuakeC has no such memory, but most accesses are at
//! the function's entry stack pointer plus a constant, so each slot of the
//! frame can become an ordinary SSA value.
//!
//! Stack addresses are tracked with a small dependent type: each value gets
//! the set of [`QcType::StackOffset`]s it may be, relative to the stack
//! pointer on entry (`global.get __stack_pointer` is offset 0, and adding a
//! constant shifts the offsets). Block parameters and `select`s join them,
//! up to [`MAX_OFFSETS`]; anything else (arithmetic with a non-constant)
//! makes the offset unknown.
//!
//! If every stack address is only used to access memory at known offsets,
//! the function is rewritten: each slot becomes an SSA variable (a block
//! parameter of every block, removed again by
//! [`prepare::simplify_params`](crate::prepare::simplify_params) where
//! trivial), loads read it and stores assign it. Otherwise the function is
//! left alone and the stack values are reported in a [`StackFailure`]; type
//! inference gives them the kind `StackOffset`, which nothing accepts.
//!
//! Pointers to locals passed to other functions (including byval and sret
//! arguments) aren't supported.

use std::collections::{BTreeMap, HashMap, HashSet};

use waffle::{Block, FunctionBody, Global, Operator as O, Terminator, Type, Value, ValueDef};

use crate::memory::{Cell, CellAccess, cells};
use crate::qc::QcType;

/// The most offsets a stack address may have before it is unknown.
pub const MAX_OFFSETS: usize = 8;

/// Why a function's stack use can't be reified, and its stack values.
#[derive(Debug)]
pub struct StackFailure {
    pub values: HashSet<Value>,
    pub reason: String,
}

/// The type of a stack address.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Fact {
    /// One of these offsets (`QcType::StackOffset`s, sorted).
    Offsets(Vec<QcType>),
    Unknown,
}

impl Fact {
    fn at(o: i32) -> Fact {
        Fact::Offsets(vec![QcType::StackOffset(o)])
    }

    fn offsets(&self) -> Option<Vec<i32>> {
        match self {
            Fact::Offsets(ts) => Some(
                ts.iter()
                    .map(|t| match t {
                        QcType::StackOffset(o) => *o,
                        _ => unreachable!("only stack offsets are tracked"),
                    })
                    .collect(),
            ),
            Fact::Unknown => None,
        }
    }

    fn from_offsets(mut os: Vec<i32>) -> Fact {
        os.sort_unstable();
        os.dedup();
        if os.len() > MAX_OFFSETS {
            return Fact::Unknown;
        }
        Fact::Offsets(os.into_iter().map(QcType::StackOffset).collect())
    }

    fn join(&self, other: &Fact) -> Fact {
        match (self.offsets(), other.offsets()) {
            (Some(mut a), Some(b)) => {
                a.extend(b);
                Fact::from_offsets(a)
            }
            _ => Fact::Unknown,
        }
    }

    fn shift(&self, k: i32) -> Fact {
        match self.offsets() {
            Some(os) => Fact::from_offsets(os.into_iter().map(|o| o.wrapping_add(k)).collect()),
            None => Fact::Unknown,
        }
    }
}

fn args(body: &FunctionBody, v: Value) -> Vec<Value> {
    match &body.values[v] {
        ValueDef::Operator(_, args, _) => body.arg_pool[*args]
            .iter()
            .map(|&a| body.resolve_alias(a))
            .collect(),
        _ => Vec::new(),
    }
}

fn const_i32(body: &FunctionBody, v: Value) -> Option<i32> {
    match &body.values[body.resolve_alias(v)] {
        ValueDef::Operator(O::I32Const { value }, _, _) => Some(*value as i32),
        _ => None,
    }
}

/// Reachable blocks in reverse postorder.
fn rpo(body: &FunctionBody) -> Vec<Block> {
    let mut seen = HashSet::from([body.entry]);
    let mut post = Vec::new();
    let mut stack = vec![(body.entry, 0usize)];
    while let Some((b, i)) = stack.pop() {
        let mut succs = Vec::new();
        body.blocks[b]
            .terminator
            .visit_targets(|t| succs.push(t.block));
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

/// The stack types of a function's values.
struct Analysis {
    facts: HashMap<Value, Fact>,
    /// The type of `__stack_pointer` on entry to each reachable block.
    sp_in: HashMap<Block, Fact>,
}

impl Analysis {
    fn fact(&self, v: Value) -> Option<&Fact> {
        self.facts.get(&v)
    }

    /// The type of the result of `v`, given the types of its operands, and
    /// the stack pointer's type `sp`.
    fn transfer(
        &self,
        body: &FunctionBody,
        sp_global: Global,
        v: Value,
        sp: &mut Fact,
    ) -> Option<Fact> {
        let ValueDef::Operator(op, _, _) = &body.values[v] else {
            return None;
        };
        let a = args(body, v);
        let f = |i: usize| self.fact(a[i]);
        match op {
            O::GlobalGet { global_index } if *global_index == sp_global => Some(sp.clone()),
            O::GlobalSet { global_index } if *global_index == sp_global => {
                *sp = f(0).cloned().unwrap_or(Fact::Unknown);
                None
            }
            O::I32Add | O::I32Sub | O::I32Or => {
                let (x, y) = (f(0), f(1));
                if x.is_none() && y.is_none() {
                    return None;
                }
                let (k0, k1) = (const_i32(body, a[0]), const_i32(body, a[1]));
                Some(match (op, x, y, k0, k1) {
                    (O::I32Add | O::I32Sub, Some(x), None, _, Some(k)) => {
                        x.shift(if matches!(op, O::I32Sub) {
                            k.wrapping_neg()
                        } else {
                            k
                        })
                    }
                    (O::I32Add, None, Some(y), Some(k), _) => y.shift(k),
                    // The stack pointer is 16-byte aligned, so `p | k` is
                    // `p + k` if `p`'s low bits are clear.
                    (O::I32Or, Some(x), None, _, Some(k))
                        if (0..16).contains(&k)
                            && x.offsets().is_some_and(|os| os.iter().all(|o| o & k == 0)) =>
                    {
                        x.shift(k)
                    }
                    _ => Fact::Unknown,
                })
            }
            O::Select => match (f(0), f(1)) {
                (None, None) => None,
                (Some(x), Some(y)) => Some(x.join(y)),
                _ => Some(Fact::Unknown),
            },
            _ => None,
        }
    }

    fn run(body: &FunctionBody, sp_global: Global, order: &[Block]) -> Analysis {
        let mut an = Analysis {
            facts: HashMap::new(),
            sp_in: HashMap::from([(body.entry, Fact::at(0))]),
        };
        loop {
            let mut changed = false;
            for &b in order {
                let Some(mut sp) = an.sp_in.get(&b).cloned() else {
                    continue;
                };
                for &v in &body.blocks[b].insts {
                    if let Some(f) = an.transfer(body, sp_global, v, &mut sp) {
                        let new = match an.facts.get(&v) {
                            Some(old) => old.join(&f),
                            None => f,
                        };
                        if an.facts.get(&v) != Some(&new) {
                            an.facts.insert(v, new);
                            changed = true;
                        }
                    }
                }
                let mut updates = Vec::new();
                body.blocks[b].terminator.visit_targets(|t| {
                    updates.push((t.block, None));
                    for (&a, &(_, p)) in t.args.iter().zip(&body.blocks[t.block].params) {
                        let a = body.resolve_alias(a);
                        if let Some(f) = an.facts.get(&a) {
                            updates.push((t.block, Some((p, f.clone()))));
                        }
                    }
                });
                for (t, u) in updates {
                    let (key, f) = match u {
                        None => {
                            let new = match an.sp_in.get(&t) {
                                Some(old) => old.join(&sp),
                                None => sp.clone(),
                            };
                            if an.sp_in.get(&t) != Some(&new) {
                                an.sp_in.insert(t, new);
                                changed = true;
                            }
                            continue;
                        }
                        Some(pf) => pf,
                    };
                    let new = match an.facts.get(&key) {
                        Some(old) => old.join(&f),
                        None => f,
                    };
                    if an.facts.get(&key) != Some(&new) {
                        an.facts.insert(key, new);
                        changed = true;
                    }
                }
            }
            if !changed {
                return an;
            }
        }
    }
}

/// A memory access of a slot, by an instruction.
struct Access {
    at: i64,
    cell: Cell,
    /// A store of a constant.
    const_store: bool,
}

/// A slot of the frame.
struct Slot {
    width: i64,
    ty: Type,
    /// Narrow: loads mask with this.
    mask: Option<u32>,
}

/// Check the uses of stack values in `body`, returning the accesses or why
/// they can't be reified.
fn check(
    body: &FunctionBody,
    sp_global: Global,
    order: &[Block],
    an: &Analysis,
) -> Result<Vec<Access>, String> {
    let stack = |v: Value| an.fact(body.resolve_alias(v)).is_some();
    let mut accesses = Vec::new();
    for &b in order {
        let mut sp = an.sp_in[&b].clone();
        for &v in &body.blocks[b].insts {
            let ValueDef::Operator(op, _, _) = &body.values[v] else {
                continue;
            };
            let a = args(body, v);
            match op {
                O::GlobalGet { global_index }
                    if *global_index == sp_global
                        && sp.offsets().is_none_or(|os| os.len() != 1) =>
                {
                    return Err("the stack pointer differs between paths".into());
                }
                O::GlobalSet { global_index } if *global_index == sp_global && !stack(a[0]) => {
                    return Err("the stack pointer is set to a value that isn't an offset \
                                from it (e.g. by `alloca`)"
                        .into());
                }
                _ => {}
            }
            an.transfer(body, sp_global, v, &mut sp);
            if !a.iter().any(|&x| stack(x)) {
                continue;
            }
            if matches!(op, O::I32Load8S { .. } | O::I32Load16S { .. }) {
                return Err("a sign-extending load of a narrow local is not supported".into());
            }
            if let Some((store, offset, cs)) = cells(op)? {
                if store && stack(a[1]) {
                    return Err("the address of a local is stored in memory".into());
                }
                let addr = an.fact(a[0]).expect("checked above");
                let Some(os) = addr.offsets() else {
                    return Err(
                        "a local is accessed at a non-constant offset (e.g. a local \
                                array indexed by a variable)"
                            .into(),
                    );
                };
                if os.len() > 1 && cs.len() > 1 {
                    return Err("a vector local is accessed through a pointer to one of \
                                several locals"
                        .into());
                }
                let const_store = store
                    && matches!(
                        body.values[a[1]],
                        ValueDef::Operator(O::I32Const { .. } | O::F32Const { .. }, _, _)
                    );
                for o in os {
                    for c in &cs {
                        accesses.push(Access {
                            at: o as i64 + offset as i64 + c.rel,
                            cell: c.cell,
                            const_store,
                        });
                    }
                }
                continue;
            }
            match op {
                O::I32Add | O::I32Sub | O::I32Or => {}
                O::GlobalSet { global_index } if *global_index == sp_global => {}
                O::Select if !stack(a[2]) => {}
                O::Call { .. } | O::CallIndirect { .. } => {
                    return Err("the address of a local is passed to a function (pointers \
                                to locals can't be passed to functions)"
                        .into());
                }
                op => return Err(format!("the address of a local is used by `{op}`")),
            }
        }
        let term = &body.blocks[b].terminator;
        match term {
            Terminator::Return { values } => {
                if values.iter().any(|&v| stack(v)) {
                    return Err("the address of a local is returned".into());
                }
                if sp != Fact::at(0) {
                    return Err("the stack pointer isn't restored on return".into());
                }
            }
            Terminator::CondBr { cond, .. } if stack(*cond) => {
                return Err("the address of a local is used as a condition".into());
            }
            Terminator::Select { value, .. } if stack(*value) => {
                return Err("the address of a local is used as a `br_table` index".into());
            }
            _ => {}
        }
        let mut mixed = false;
        term.visit_targets(|t| {
            for (&a, &(_, p)) in t.args.iter().zip(&body.blocks[t.block].params) {
                mixed |= stack(p) != stack(a);
            }
        });
        if mixed {
            return Err("a value is the address of a local on one path and not on another".into());
        }
    }
    Ok(accesses)
}

/// The slots of the frame, from its accesses.
fn slots(accesses: &[Access]) -> Result<BTreeMap<i64, Slot>, String> {
    let mut by_at: BTreeMap<i64, Vec<&Access>> = BTreeMap::new();
    for a in accesses {
        by_at.entry(a.at).or_default().push(a);
    }
    let mut slots = BTreeMap::new();
    for (&at, accs) in &by_at {
        let width = accs[0].cell.width();
        if let Some(b) = accs.iter().find(|a| a.cell.width() != width) {
            return Err(format!(
                "the local at stack offset {at} is accessed as both {} and {}",
                accs[0].cell.name(),
                b.cell.name()
            ));
        }
        // The type, from the accesses that aren't stores of constants
        // (which can be converted) or vector lanes (which are either).
        let typed = |a: &&&Access| !a.const_store && a.cell != Cell::Lane;
        let mut ty = None;
        for a in accs.iter().filter(typed) {
            match ty {
                None => ty = Some(a.cell),
                Some(t) if t == a.cell => {}
                Some(t) => {
                    return Err(format!(
                        "the local at stack offset {at} is accessed as both {} and {}",
                        t.name(),
                        a.cell.name()
                    ));
                }
            }
        }
        let cell = ty
            .or_else(|| accs.iter().find(|a| a.cell != Cell::Lane).map(|a| a.cell))
            .unwrap_or(Cell::F32);
        let (ty, mask) = match cell {
            Cell::I32 => (Type::I32, None),
            Cell::F32 | Cell::Lane => (Type::F32, None),
            Cell::Narrow(w) => (Type::I32, Some(if w == 8 { 0xff } else { 0xffff })),
        };
        slots.insert(at, Slot { width, ty, mask });
    }
    // Overlaps between slots.
    let mut prev: Option<(i64, i64)> = None;
    for (&at, s) in &slots {
        if let Some((p, end)) = prev
            && at < end
        {
            return Err(format!(
                "locals at stack offsets {p} and {at} overlap (e.g. a struct copied in pieces)"
            ));
        }
        prev = Some((at, at + s.width));
    }
    Ok(slots)
}

/// Reify the stack slots of `body`, whose stack pointer global is `sp`.
pub fn reify(body: &mut FunctionBody, sp: Global) -> Result<(), StackFailure> {
    let uses_sp = body.blocks.entries().any(|(_, b)| {
        b.insts.iter().any(|&v| {
            matches!(
                &body.values[v],
                ValueDef::Operator(O::GlobalGet { global_index } | O::GlobalSet { global_index }, _, _)
                    if *global_index == sp
            )
        })
    });
    if !uses_sp {
        return Ok(());
    }
    let order = rpo(body);
    let an = Analysis::run(body, sp, &order);
    let fail = |reason: String| StackFailure {
        values: an.facts.keys().copied().collect(),
        reason,
    };
    let accesses = check(body, sp, &order, &an).map_err(fail)?;
    let slots = slots(&accesses).map_err(fail)?;
    rewrite(body, sp, &order, &an, &slots);
    Ok(())
}

fn add_inst(
    body: &mut FunctionBody,
    insts: &mut Vec<Value>,
    op: O,
    args: &[Value],
    ty: Type,
) -> Value {
    let args = body.arg_pool.from_iter(args.iter().copied());
    let tys = body.single_type_list(ty);
    let v = body.add_value(ValueDef::Operator(op, args, tys));
    insts.push(v);
    v
}

fn zero(ty: Type) -> O {
    match ty {
        Type::F32 => O::F32Const { value: 0 },
        _ => O::I32Const { value: 0 },
    }
}

fn rewrite(
    body: &mut FunctionBody,
    sp: Global,
    order: &[Block],
    an: &Analysis,
    slots: &BTreeMap<i64, Slot>,
) {
    // Unreachable blocks can't be reasoned about (and are never emitted).
    let reachable: HashSet<Block> = order.iter().copied().collect();
    let all: Vec<Block> = body.blocks.iter().collect();
    for b in all {
        if !reachable.contains(&b) {
            body.blocks[b].insts.clear();
            body.blocks[b].terminator = Terminator::Unreachable;
        }
    }

    // Addresses become their offsets: only those with several offsets are
    // still needed (to choose between slots), and they are then just
    // numbers.
    for (&v, f) in &an.facts {
        if let ValueDef::Operator(..) = body.values[v]
            && let Some([o]) = f.offsets().as_deref()
        {
            let tys = body.single_type_list(Type::I32);
            body.values[v] =
                ValueDef::Operator(O::I32Const { value: *o as u32 }, Default::default(), tys);
        }
    }

    let index: HashMap<i64, usize> = slots.keys().enumerate().map(|(i, &at)| (at, i)).collect();
    let tys: Vec<Type> = slots.values().map(|s| s.ty).collect();
    let masks: Vec<Option<u32>> = slots.values().map(|s| s.mask).collect();

    // A parameter per slot on every block but the entry.
    let mut cur_in: HashMap<Block, Vec<Value>> = HashMap::new();
    for &b in order {
        if b == body.entry {
            continue;
        }
        let ps = tys.iter().map(|&t| body.add_blockparam(b, t)).collect();
        cur_in.insert(b, ps);
    }

    for &b in order {
        let old = std::mem::take(&mut body.blocks[b].insts);
        let mut insts = Vec::new();
        let mut cur = match cur_in.get(&b) {
            Some(ps) => ps.clone(),
            None => tys
                .iter()
                .map(|&t| add_inst(body, &mut insts, zero(t), &[], t))
                .collect(),
        };
        for v in old {
            let ValueDef::Operator(op, _, _) = body.values[v].clone() else {
                insts.push(v);
                continue;
            };
            if matches!(op, O::GlobalSet { global_index } if global_index == sp) {
                continue;
            }
            let a = args(body, v);
            let Some(addr) = a.first().and_then(|&x| an.fact(x)) else {
                insts.push(v);
                continue;
            };
            let Ok(Some((store, offset, cs))) = cells(&op) else {
                // Arithmetic on addresses (now constants), or a `select`.
                insts.push(v);
                continue;
            };
            let os = addr.offsets().expect("checked");
            let slot_of = |o: i32, c: &CellAccess| index[&(o as i64 + offset as i64 + c.rel)];
            if store {
                let mut x = a[1];
                // A constant of the other type, converted.
                let s = slot_of(os[0], &cs[0]);
                if cs.len() == 1 {
                    match (&body.values[x], tys[s]) {
                        (ValueDef::Operator(O::I32Const { value }, _, _), Type::F32) => {
                            let value = *value;
                            x = add_inst(body, &mut insts, O::F32Const { value }, &[], Type::F32);
                        }
                        (ValueDef::Operator(O::F32Const { value }, _, _), Type::I32) => {
                            let value = *value;
                            x = add_inst(body, &mut insts, O::I32Const { value }, &[], Type::I32);
                        }
                        _ => {}
                    }
                }
                if let [o] = os[..] {
                    for c in &cs {
                        let s = slot_of(o, c);
                        cur[s] = match c.lane {
                            Some(lane) => {
                                let op = if tys[s] == Type::I32 {
                                    O::I32x4ExtractLane { lane }
                                } else {
                                    O::F32x4ExtractLane { lane }
                                };
                                add_inst(body, &mut insts, op, &[x], tys[s])
                            }
                            None => x,
                        };
                    }
                } else {
                    // Through a pointer to one of several slots.
                    for &o in &os {
                        let s = slot_of(o, &cs[0]);
                        let k = add_inst(
                            body,
                            &mut insts,
                            O::I32Const { value: o as u32 },
                            &[],
                            Type::I32,
                        );
                        let eq = add_inst(body, &mut insts, O::I32Eq, &[a[0], k], Type::I32);
                        cur[s] = add_inst(body, &mut insts, O::Select, &[x, cur[s], eq], tys[s]);
                    }
                }
                continue;
            }
            // A load.
            if cs[0].lane.is_some() {
                // Lanes into a vector: the loaded-into vector, or zeros.
                let mut vec = match op {
                    O::V128Load32Lane { .. } | O::V128Load64Lane { .. } => a[1],
                    _ => add_inst(body, &mut insts, O::V128Const { value: 0 }, &[], Type::V128),
                };
                let o = os[0];
                for (i, c) in cs.iter().enumerate() {
                    let s = slot_of(o, c);
                    let lane = c.lane.unwrap();
                    let rop = if tys[s] == Type::I32 {
                        O::I32x4ReplaceLane { lane }
                    } else {
                        O::F32x4ReplaceLane { lane }
                    };
                    if i + 1 == cs.len() {
                        let args = body.arg_pool.from_iter([vec, cur[s]].into_iter());
                        let vt = body.single_type_list(Type::V128);
                        body.values[v] = ValueDef::Operator(rop, args, vt);
                        insts.push(v);
                    } else {
                        vec = add_inst(body, &mut insts, rop, &[vec, cur[s]], Type::V128);
                    }
                }
                continue;
            }
            let value = |body: &mut FunctionBody, o: i32, insts: &mut Vec<Value>| -> Value {
                let s = slot_of(o, &cs[0]);
                match masks[s] {
                    Some(m) => {
                        let k = add_inst(body, insts, O::I32Const { value: m }, &[], Type::I32);
                        add_inst(body, insts, O::I32And, &[cur[s], k], Type::I32)
                    }
                    None => cur[s],
                }
            };
            let mut r = value(body, *os.last().unwrap(), &mut insts);
            for &o in os[..os.len() - 1].iter().rev() {
                let x = value(body, o, &mut insts);
                let k = add_inst(
                    body,
                    &mut insts,
                    O::I32Const { value: o as u32 },
                    &[],
                    Type::I32,
                );
                let eq = add_inst(body, &mut insts, O::I32Eq, &[a[0], k], Type::I32);
                let ty = tys[slot_of(o, &cs[0])];
                r = add_inst(body, &mut insts, O::Select, &[x, r, eq], ty);
            }
            body.set_alias(v, r);
        }
        body.blocks[b].insts = insts;
        let mut term = std::mem::replace(&mut body.blocks[b].terminator, Terminator::None);
        term.update_targets(|t| {
            if cur_in.contains_key(&t.block) {
                t.args.extend(cur.iter().copied());
            }
        });
        body.blocks[b].terminator = term;
    }
    crate::prepare::remove_dead(body, |op| {
        matches!(
            op,
            O::I32Const { .. }
                | O::I64Const { .. }
                | O::F32Const { .. }
                | O::V128Const { .. }
                | O::I32Add
                | O::I32Sub
                | O::I32Or
                | O::I32And
                | O::I32Eq
                | O::Select
                | O::I32x4ExtractLane { .. }
                | O::F32x4ExtractLane { .. }
                | O::I32x4ReplaceLane { .. }
                | O::F32x4ReplaceLane { .. }
        ) || matches!(op, O::GlobalGet { global_index } if *global_index == sp)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_and_shifts() {
        let a = Fact::at(-16).shift(8);
        assert_eq!(a, Fact::at(-8));
        let j = a.join(&Fact::at(-4)).join(&Fact::at(-8));
        assert_eq!(j.offsets(), Some(vec![-8, -4]));
        let mut many = Fact::at(0);
        for i in 1..=MAX_OFFSETS as i32 {
            many = many.join(&Fact::at(4 * i));
        }
        assert_eq!(many, Fact::Unknown);
        assert_eq!(Fact::Unknown.shift(4), Fact::Unknown);
    }

    #[test]
    fn overlapping_slots_are_rejected() {
        let acc = |at, cell| Access {
            at,
            cell,
            const_store: false,
        };
        assert!(slots(&[acc(-16, Cell::I64), acc(-12, Cell::I32)]).is_err());
        assert!(slots(&[acc(-16, Cell::F32), acc(-16, Cell::I32)]).is_err());
        let ok = slots(&[
            acc(-16, Cell::F32),
            Access {
                at: -16,
                cell: Cell::I32,
                const_store: true,
            },
            acc(-12, Cell::Lane),
        ])
        .unwrap();
        assert_eq!(ok[&-16].ty, Type::F32);
        assert_eq!(ok[&-12].ty, Type::F32);
    }
}
