//! Type inference: a QuakeC type for every SSA value.
//!
//! Wasm types say little about QuakeC types. An `i32` may be a number, a
//! function pointer (a table index), an entity field offset or a string
//! literal address; an `i64` may be an entity, a string or a field reference.
//! So every SSA value gets a type variable, and operators, intrinsics and
//! builtins constrain them.
//!
//! # Offsets
//!
//! Function pointers, field offsets and string addresses are computed with
//! constant offsets: `__table_base + f`, or a merged function taking the
//! offset of a string from `__memory_base`. The variables form a *weighted*
//! union-find: each has a potential relative to its class root, and
//! `r = x + k` puts `r` in `x`'s class with potential `pot(x) + k`. A class of
//! a non-float kind has an *anchor* `A`, and a value `v` in it denotes the
//! function/field/string at address `v + A - pot(v)` (its *bias* is
//! `A - pot(v)`). Uses that need the address itself (e.g. the field offset of
//! an entity field access) fix the anchor, so all values in a class denote the
//! same QuakeC value and `r = x + k` needs no code.
//!
//! # Polymorphism
//!
//! Identical functions taking different types may be merged, so signatures
//! are generalised: call-graph SCCs are solved bottom-up, the classes of a
//! function's parameters and result that have no kind yet are quantified,
//! and each call site instantiates them afresh. [`Infer::instances`] then
//! enumerates the distinct ground instantiations of each function, which
//! become separate QuakeC functions.
//!
//! # Constants
//!
//! The same constant (e.g. `i32.const 0`) may be shared between unrelated
//! uses, so constants have no variable of their own: each *use* of one gets a
//! fresh variable, and the constant is materialised according to it.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use anyhow::{Result, anyhow, bail};
use waffle::{Block, Func, FunctionBody, Global, Operator, Terminator, Type, Value, ValueDef};

use crate::qc::QcType;
use crate::translate::{Ctx, FuncKind, Intrinsic};

/// The QuakeC kind of a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Float,
    Vector,
    Entity,
    String,
    Func,
    /// A field reference, `.T`.
    Field(QcType),
}

impl Kind {
    pub fn qc(self) -> QcType {
        match self {
            Kind::Float => QcType::Float,
            Kind::Vector => QcType::Vector,
            Kind::Entity => QcType::Entity,
            Kind::String => QcType::String,
            Kind::Func => QcType::Func,
            Kind::Field(t) => t.field_of().expect("fields have value types"),
        }
    }

    pub fn from_qc(t: QcType) -> Option<Kind> {
        Some(match t {
            QcType::Float => Kind::Float,
            QcType::Vector => Kind::Vector,
            QcType::Entity => Kind::Entity,
            QcType::String => Kind::String,
            QcType::Func => Kind::Func,
            QcType::FloatField => Kind::Field(QcType::Float),
            QcType::VectorField => Kind::Field(QcType::Vector),
            QcType::EntityField => Kind::Field(QcType::Entity),
            QcType::StringField => Kind::Field(QcType::String),
            QcType::FuncField => Kind::Field(QcType::Func),
            QcType::Void => return None,
        })
    }

    /// Whether values of this kind are addresses resolved at compile time
    /// (see the module docs on offsets).
    fn addressed(self) -> bool {
        matches!(self, Kind::Func | Kind::String | Kind::Field(_))
    }

    /// The kind of an unconstrained value of wasm type `t`.
    fn default_for(t: Type) -> Kind {
        match t {
            Type::I64 => Kind::Entity,
            Type::V128 => Kind::Vector,
            _ => Kind::Float,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.qc().name())
    }
}

/// Where a value is used: by an instruction, or by a block's terminator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    Inst(Value),
    Term(Block),
}

/// The operand index of a terminator's branch argument.
pub fn term_arg(target: usize, arg: usize) -> u32 {
    ((target as u32) << 16) | arg as u32
}
/// The operand index of a terminator's condition or `br_table` index.
pub const TERM_COND: u32 = 0xfffe_0000;
/// The operand index of the `i`th returned value.
pub fn term_ret(i: usize) -> u32 {
    0xffff_0000 | i as u32
}

type Var = u32;
/// A quantified root, and its kind and bias in an instance.
type Subst = (Var, Kind, i64);

#[derive(Clone)]
struct Class {
    kind: Option<(Kind, String)>,
    anchor: Option<(i64, String)>,
    /// Some potentials in the class contradict each other (fine for floats).
    inconsistent: bool,
    /// Uses that fix the anchor differently (fine for floats).
    conflict: Option<String>,
}

/// The union-find over type variables.
#[derive(Default)]
struct Uf {
    parent: Vec<Var>,
    /// Potential relative to the parent.
    rel: Vec<i64>,
    wasm: Vec<Type>,
    class: Vec<Class>,
}

impl Uf {
    fn fresh(&mut self, t: Type) -> Var {
        let v = self.parent.len() as Var;
        self.parent.push(v);
        self.rel.push(0);
        self.wasm.push(t);
        self.class.push(Class {
            kind: None,
            anchor: None,
            inconsistent: false,
            conflict: None,
        });
        v
    }

    /// The root of `v` and the potential of `v` relative to it.
    fn find(&mut self, v: Var) -> (Var, i64) {
        let p = self.parent[v as usize];
        if p == v {
            return (v, 0);
        }
        let (r, pp) = self.find(p);
        let pot = self.rel[v as usize].wrapping_add(pp);
        self.parent[v as usize] = r;
        self.rel[v as usize] = pot;
        (r, pot)
    }

    fn set_kind(&mut self, v: Var, k: Kind, why: &str) -> Result<()> {
        let (r, _) = self.find(v);
        let c = &mut self.class[r as usize];
        match &c.kind {
            None => c.kind = Some((k, why.to_string())),
            Some((old, _)) if *old == k => {}
            Some((old, old_why)) => {
                bail!("type conflict: a value is used as {old} ({old_why}) and as {k} ({why})")
            }
        }
        Ok(())
    }

    /// `a = b + k`.
    fn union(&mut self, a: Var, b: Var, k: i64, why: &str) -> Result<()> {
        let (ra, pa) = self.find(a);
        let (rb, pb) = self.find(b);
        if ra == rb {
            if pa != pb.wrapping_add(k) {
                self.class[ra as usize].inconsistent = true;
            }
            return Ok(());
        }
        // pot(rb) relative to ra.
        let rel = pa.wrapping_sub(pb).wrapping_sub(k);
        self.parent[rb as usize] = ra;
        self.rel[rb as usize] = rel;
        let cb = self.class[rb as usize].clone();
        if let Some((k, kwhy)) = cb.kind {
            self.set_kind(ra, k, &kwhy)
                .map_err(|e| anyhow!("{e} (joined by {why})"))?;
        }
        if let Some((anchor, awhy)) = cb.anchor {
            self.set_anchor(ra, anchor.wrapping_add(rel), &awhy)?;
        }
        if cb.inconsistent {
            self.class[ra as usize].inconsistent = true;
        }
        if let Some(c) = cb.conflict
            && self.class[ra as usize].conflict.is_none()
        {
            self.class[ra as usize].conflict = Some(c);
        }
        Ok(())
    }

    /// Fix the anchor of `root`'s class. A conflict is only an error if
    /// the class turns out to hold addresses (see [`Infer::ground`]).
    fn set_anchor(&mut self, root: Var, anchor: i64, why: &str) -> Result<()> {
        let c = &mut self.class[root as usize];
        match &c.anchor {
            None => c.anchor = Some((anchor, why.to_string())),
            Some((a, _)) if *a == anchor => {}
            Some((a, awhy)) => {
                if c.conflict.is_none() {
                    c.conflict = Some(format!(
                        "values are used at different offsets ({awhy}, and {why} at {})",
                        anchor.wrapping_sub(*a)
                    ));
                }
            }
        }
        Ok(())
    }

    /// `v` is a QuakeC value of kind `k` (so used as the address itself).
    fn typed(&mut self, v: Var, k: Kind, why: &str) -> Result<()> {
        self.set_kind(v, k, why)?;
        self.exact(v, why)
    }

    /// `v` is used as the address itself.
    fn exact(&mut self, v: Var, why: &str) -> Result<()> {
        let (r, p) = self.find(v);
        self.set_anchor(r, p, why)
    }
}

/// Per-function inference state.
pub struct FuncTypes {
    vals: HashMap<Value, Var>,
    uses: HashMap<(Site, u32), Var>,
    params: Vec<Var>,
    rets: Vec<Var>,
    /// For calls to functions in other SCCs: the callee's quantified roots
    /// and their instantiations here.
    calls: HashMap<Value, Vec<(Var, Var)>>,
    scc: usize,
}

/// A ground instantiation of a function.
#[derive(Clone, Debug)]
pub struct Instance {
    pub func: Func,
    pub name: String,
    /// Kind and bias of each quantified root of the function's SCC.
    subst: BTreeMap<Var, (Kind, i64)>,
    /// The instance called by each direct call.
    pub callees: HashMap<Value, usize>,
}

pub struct Infer {
    uf: Uf,
    pub funcs: HashMap<Func, FuncTypes>,
    globals: HashMap<Global, Var>,
    /// Imported functions without a known signature (unknown builtins).
    imports: HashMap<Func, (Vec<Var>, Vec<Var>)>,
    quantified: Vec<Vec<Var>>,
}

/// Follow aliases.
pub fn resolve(body: &FunctionBody, v: Value) -> Value {
    body.resolve_alias(v)
}

/// The arguments of an operator value.
pub fn args(body: &FunctionBody, v: Value) -> Vec<Value> {
    match &body.values[v] {
        ValueDef::Operator(_, args, _) => body.arg_pool[*args]
            .iter()
            .map(|&a| body.resolve_alias(a))
            .collect(),
        _ => vec![],
    }
}

/// The wasm type of a value.
pub fn wasm_ty(body: &FunctionBody, v: Value) -> Option<Type> {
    match &body.values[resolve(body, v)] {
        ValueDef::BlockParam(_, _, t) | ValueDef::PickOutput(_, _, t) => Some(*t),
        ValueDef::Operator(_, _, tys) => body.type_pool[*tys].first().copied(),
        _ => None,
    }
}

/// The value of `v` if it is a constant (or arithmetic on constants).
pub fn const_val(body: &FunctionBody, v: Value) -> Option<i64> {
    let v = resolve(body, v);
    let ValueDef::Operator(op, _, _) = &body.values[v] else {
        return None;
    };
    match op {
        Operator::I32Const { value } => Some(*value as i32 as i64),
        Operator::I64Const { value } => Some(*value as i64),
        Operator::I32Add | Operator::I32Sub | Operator::I32Mul => {
            let a = args(body, v);
            let (x, y) = (const_val(body, a[0])? as i32, const_val(body, a[1])? as i32);
            Some(match op {
                Operator::I32Add => x.wrapping_add(y),
                Operator::I32Sub => x.wrapping_sub(y),
                _ => x.wrapping_mul(y),
            } as i64)
        }
        _ => None,
    }
}

/// The branch targets of a terminator.
pub fn targets(t: &Terminator) -> Vec<&waffle::BlockTarget> {
    match t {
        Terminator::Br { target } => vec![target],
        Terminator::CondBr {
            if_true, if_false, ..
        } => vec![if_true, if_false],
        Terminator::Select {
            targets, default, ..
        } => targets.iter().chain([default]).collect(),
        _ => vec![],
    }
}

/// The local functions called directly by `body`.
fn callees(body: &FunctionBody) -> Vec<Func> {
    let mut out = Vec::new();
    for (_, b) in body.blocks.entries() {
        for &v in &b.insts {
            if let ValueDef::Operator(Operator::Call { function_index }, _, _) = &body.values[v] {
                out.push(*function_index);
            }
        }
    }
    out
}

impl Infer {
    /// Infer types for every local function in `ctx`.
    pub fn run(ctx: &Ctx<'_>) -> Result<Self> {
        let mut inf = Infer {
            uf: Uf::default(),
            funcs: HashMap::new(),
            globals: HashMap::new(),
            imports: HashMap::new(),
            quantified: Vec::new(),
        };
        for (g, data) in ctx.module.globals.entries() {
            let v = inf.uf.fresh(data.ty);
            inf.uf.exact(v, "a wasm global")?;
            inf.globals.insert(g, v);
        }
        for (&f, info) in &ctx.funcs {
            if matches!(
                info.kind,
                FuncKind::Builtin(None) | FuncKind::UnnamedBuiltin
            ) {
                let sig = &ctx.module.signatures[ctx.module.funcs[f].sig()];
                let mut vs = |ts: &[Type]| -> Result<Vec<Var>> {
                    ts.iter()
                        .map(|&t| {
                            let v = inf.uf.fresh(t);
                            inf.uf.exact(v, &format!("`{}`", info.name))?;
                            Ok(v)
                        })
                        .collect()
                };
                let ps = vs(&sig.params)?;
                let rs = vs(&sig.returns)?;
                inf.imports.insert(f, (ps, rs));
            }
        }

        let locals: Vec<Func> = ctx.local_funcs();
        for scc in sccs(ctx, &locals) {
            let id = inf.quantified.len();
            for &f in &scc {
                inf.declare(ctx, f, id)?;
            }
            for &f in &scc {
                let name = &ctx.funcs[&f].name;
                inf.constrain(ctx, f)
                    .map_err(|e| e.context(format!("while inferring types in `{name}`")))?;
            }
            let mut q = Vec::new();
            for &f in &scc {
                let ft = &inf.funcs[&f];
                let sig: Vec<Var> = ft.params.iter().chain(&ft.rets).copied().collect();
                for v in sig {
                    let (r, _) = inf.uf.find(v);
                    if inf.uf.class[r as usize].kind.is_none() && !q.contains(&r) {
                        q.push(r);
                    }
                }
            }
            inf.quantified.push(q);
        }
        Ok(inf)
    }

    fn body<'a>(ctx: &'a Ctx<'_>, f: Func) -> &'a FunctionBody {
        ctx.module.funcs[f]
            .body()
            .expect("local function has a body")
    }

    /// Create the variables of `f`.
    fn declare(&mut self, ctx: &Ctx<'_>, f: Func, scc: usize) -> Result<()> {
        let body = Self::body(ctx, f);
        let mut vals = HashMap::new();
        for (block, b) in body.blocks.entries() {
            for &(t, v) in &b.params {
                vals.insert(v, self.uf.fresh(t));
            }
            for &v in &b.insts {
                if const_val(body, v).is_some() {
                    continue;
                }
                if let ValueDef::Operator(_, _, tys) = &body.values[v] {
                    match body.type_pool[*tys] {
                        [] => {}
                        [t] => {
                            vals.insert(v, self.uf.fresh(t));
                        }
                        _ => bail!("multi-value results are not supported (in block {block})"),
                    }
                }
            }
        }
        let params = body.blocks[body.entry]
            .params
            .iter()
            .map(|&(_, v)| vals[&v])
            .collect();
        let rets = body.rets.iter().map(|&t| self.uf.fresh(t)).collect();
        self.funcs.insert(
            f,
            FuncTypes {
                vals,
                uses: HashMap::new(),
                params,
                rets,
                calls: HashMap::new(),
                scc,
            },
        );
        Ok(())
    }

    /// The variable for operand `idx` of `site`, which is `v`.
    fn operand(&mut self, body: &FunctionBody, f: Func, site: Site, idx: u32, v: Value) -> Var {
        let v = resolve(body, v);
        if const_val(body, v).is_some() {
            let t = wasm_ty(body, v).unwrap_or(Type::I32);
            let fresh = self.uf.fresh(t);
            let ft = self.funcs.get_mut(&f).unwrap();
            return *ft.uses.entry((site, idx)).or_insert(fresh);
        }
        self.funcs[&f].vals[&v]
    }

    fn val(&self, f: Func, v: Value) -> Option<Var> {
        self.funcs[&f].vals.get(&v).copied()
    }

    fn constrain(&mut self, ctx: &Ctx<'_>, f: Func) -> Result<()> {
        let body = Self::body(ctx, f);
        for (block, b) in body.blocks.entries() {
            for &v in &b.insts {
                if const_val(body, v).is_some() {
                    continue;
                }
                self.inst(ctx, f, body, v)
                    .map_err(|e| e.context(format!("at {v} = {}", describe(body, v))))?;
            }
            // Branch arguments flow into block params.
            let site = Site::Term(block);
            for (t, target) in targets(&b.terminator).into_iter().enumerate() {
                let params = &body.blocks[target.block].params;
                for (a, (&arg, &(_, p))) in target.args.iter().zip(params).enumerate() {
                    let av = self.operand(body, f, site, term_arg(t, a), arg);
                    let pv = self.funcs[&f].vals[&p];
                    self.uf
                        .union(pv, av, 0, &format!("branch to {}", target.block))?;
                }
            }
            if let Terminator::Return { values } = &b.terminator {
                for (i, &r) in values.iter().enumerate() {
                    let rv = self.operand(body, f, site, term_ret(i), r);
                    let ret = self.funcs[&f].rets[i];
                    self.uf.union(ret, rv, 0, "return value")?;
                }
            }
        }
        Ok(())
    }

    fn inst(&mut self, ctx: &Ctx<'_>, f: Func, body: &FunctionBody, v: Value) -> Result<()> {
        let ValueDef::Operator(op, _, _) = &body.values[v] else {
            return Ok(());
        };
        let a = args(body, v);
        let site = Site::Inst(v);
        let fname = &ctx.funcs[&f].name;
        let why = |what: &str| format!("{what} in `{fname}`");
        // Operand variables, and the result.
        let opnd = |this: &mut Self, i: usize| this.operand(body, f, site, i as u32, a[i]);
        let res = self.val(f, v);
        // Every constant operand gets its own variable, even if no rule below
        // constrains it (it then has its default kind).
        for i in 0..a.len() {
            opnd(self, i);
        }
        let kind_all = |this: &mut Self, vars: &[Var], k: Kind, what: &str| -> Result<()> {
            for &x in vars {
                this.uf.set_kind(x, k, &why(what))?;
            }
            Ok(())
        };

        use Operator as O;
        match op {
            // `x + k`, `x - k`: an offset (see the module docs).
            O::I32Add | O::I32Sub
                if const_val(body, a[1]).is_some() || const_val(body, a[0]).is_some() =>
            {
                let r = res.unwrap();
                let (x, k) = match (const_val(body, a[0]), const_val(body, a[1])) {
                    (None, Some(k)) => (a[0], if *op == O::I32Sub { -k } else { k }),
                    (Some(k), None) if *op == O::I32Add => (a[1], k),
                    _ => {
                        // `k - x`: arithmetic.
                        let xs = [opnd(self, 0), opnd(self, 1), r];
                        return kind_all(self, &xs, Kind::Float, "integer arithmetic");
                    }
                };
                let xv = self.val(f, x).unwrap();
                self.uf.union(r, xv, k, &why("an offset"))?;
            }
            // Comparisons that work on any kind of value.
            O::I32Eq | O::I32Ne | O::I64Eq | O::I64Ne => {
                let (x, y) = (opnd(self, 0), opnd(self, 1));
                self.uf.union(x, y, 0, &why("a comparison"))?;
                kind_all(self, &[res.unwrap()], Kind::Float, "a comparison result")?;
            }
            O::I32Eqz | O::I64Eqz => {
                kind_all(self, &[res.unwrap()], Kind::Float, "a comparison result")?;
            }
            O::Select | O::TypedSelect { .. } => {
                let r = res.unwrap();
                let (x, y) = (opnd(self, 0), opnd(self, 1));
                self.uf.union(r, x, 0, &why("a select"))?;
                self.uf.union(r, y, 0, &why("a select"))?;
            }
            O::GlobalGet { global_index } | O::GlobalSet { global_index }
                if Some(*global_index) == ctx.stack_pointer =>
            {
                bail!(
                    "this function uses the shadow stack (`__stack_pointer`), which QuakeC can't \
                     support. Usually a struct was passed by reference or a local's address was \
                     taken; pass `Entity`, `String` and `Vector` values by value instead"
                )
            }
            O::GlobalGet { global_index } => {
                let g = self.globals[global_index];
                self.uf.union(res.unwrap(), g, 0, &why("a wasm global"))?;
            }
            O::GlobalSet { global_index } => {
                let g = self.globals[global_index];
                let x = opnd(self, 0);
                self.uf.union(g, x, 0, &why("a wasm global"))?;
            }
            O::Call { function_index } => self.call(ctx, f, body, v, *function_index)?,
            O::CallIndirect { sig_index, .. } => {
                let idx = opnd(self, a.len() - 1);
                self.uf
                    .set_kind(idx, Kind::Func, &why("an indirect call"))?;
                self.uf.exact(idx, &why("an indirect call"))?;
                // Arguments and results are QuakeC values, but their kinds
                // are only constrained by their wasm types (unknown kinds
                // default as usual).
                let _ = sig_index;
                for i in 0..a.len() - 1 {
                    let x = opnd(self, i);
                    self.uf.exact(x, &why("an indirect call"))?;
                }
                if let Some(r) = res {
                    self.uf.exact(r, &why("an indirect call"))?;
                }
            }
            O::F32Const { .. } => kind_all(self, &[res.unwrap()], Kind::Float, "a constant")?,
            O::V128Const { .. } => kind_all(self, &[res.unwrap()], Kind::Vector, "a constant")?,
            O::Nop | O::Unreachable => {}
            O::RefFunc { .. }
            | O::RefNull { .. }
            | O::RefIsNull
            | O::TableGet { .. }
            | O::TableSet { .. }
            | O::CallRef { .. } => bail!(
                "reference types are not supported; use plain function pointers \
                 (`*const fn`) for QuakeC functions"
            ),
            // A number, or a pointer stored in static data (a string
            // literal or a function, see `__wasm_apply_data_relocs`): typed by
            // its uses.
            O::I32Load { .. } => {
                let x = opnd(self, 0);
                self.uf.set_kind(x, Kind::Float, &why("a memory address"))?;
                if let Some(r) = res {
                    self.uf.exact(r, &why("a load"))?;
                }
            }
            op => {
                // Everything else works on numbers or vectors, by wasm type.
                let mut vars: Vec<Var> = (0..a.len()).map(|i| opnd(self, i)).collect();
                vars.extend(res);
                for x in vars {
                    let t = self.uf.wasm[x as usize];
                    let k = match t {
                        Type::V128 => Kind::Vector,
                        Type::F32 | Type::I32 => Kind::Float,
                        other => bail!("unsupported operator {op:?} on {other:?}"),
                    };
                    self.uf.set_kind(x, k, &why(&format!("`{op:?}`")))?;
                }
            }
        }
        Ok(())
    }

    fn call(
        &mut self,
        ctx: &Ctx<'_>,
        f: Func,
        body: &FunctionBody,
        v: Value,
        callee: Func,
    ) -> Result<()> {
        let a = args(body, v);
        let site = Site::Inst(v);
        let info = &ctx.funcs[&callee];
        let cname = &info.name;
        let fname = &ctx.funcs[&f].name;
        let ops: Vec<Var> = (0..a.len())
            .map(|i| self.operand(body, f, site, i as u32, a[i]))
            .collect();
        let res = self.val(f, v);
        let why = |i: Option<usize>| match i {
            Some(i) => format!("argument {i} of `{cname}` in `{fname}`"),
            None => format!("result of `{cname}` in `{fname}`"),
        };
        match info.kind {
            FuncKind::Local => {
                let callee_ft = &self.funcs[&callee];
                let sig: Vec<Var> = callee_ft
                    .params
                    .iter()
                    .chain(&callee_ft.rets)
                    .copied()
                    .collect();
                let callee_scc = callee_ft.scc;
                let same_scc = callee_scc == self.funcs[&f].scc;
                let actual: Vec<Var> = ops.iter().copied().chain(res).collect();
                let mut inst: Vec<(Var, Var)> = Vec::new();
                for (i, (&p, &x)) in sig.iter().zip(&actual).enumerate() {
                    let what = why((i < ops.len()).then_some(i));
                    let (r, pot) = self.uf.find(p);
                    let quantified = !same_scc && self.quantified[callee_scc].contains(&r);
                    if quantified {
                        let fresh = match inst.iter().find(|(q, _)| *q == r) {
                            Some(&(_, fresh)) => fresh,
                            None => {
                                let t = self.uf.wasm[r as usize];
                                let fresh = self.uf.fresh(t);
                                // The same uses (relative to the root).
                                let c = self.uf.class[r as usize].clone();
                                self.uf.class[fresh as usize] = c;
                                inst.push((r, fresh));
                                fresh
                            }
                        };
                        self.uf.union(x, fresh, pot, &what)?;
                    } else {
                        self.uf.union(x, p, 0, &what)?;
                    }
                }
                if !same_scc {
                    self.funcs.get_mut(&f).unwrap().calls.insert(v, inst);
                }
            }
            FuncKind::Builtin(Some(_)) => {
                let b = ctx.defs.builtin(cname).expect("known builtin");
                for (i, (&x, &t)) in ops.iter().zip(b.params).enumerate() {
                    let k = Kind::from_qc(t).ok_or_else(|| anyhow!("void builtin parameter"))?;
                    self.uf.typed(x, k, &why(Some(i)))?;
                }
                if let (Some(r), Some(k)) = (res, Kind::from_qc(b.ret)) {
                    self.uf.typed(r, k, &why(None))?;
                }
            }
            FuncKind::Builtin(None) | FuncKind::UnnamedBuiltin => {
                let (ps, rs) = self.imports[&callee].clone();
                for (i, (&x, &p)) in ops.iter().zip(&ps).enumerate() {
                    self.uf.union(x, p, 0, &why(Some(i)))?;
                }
                if let (Some(r), Some(&p)) = (res, rs.first()) {
                    self.uf.union(r, p, 0, &why(None))?;
                }
            }
            FuncKind::Intrinsic(intr) => self.intrinsic(intr, &ops, res, &why)?,
        }
        Ok(())
    }

    fn intrinsic(
        &mut self,
        intr: Intrinsic,
        ops: &[Var],
        res: Option<Var>,
        why: &dyn Fn(Option<usize>) -> String,
    ) -> Result<()> {
        use Intrinsic as I;
        let uf = &mut self.uf;
        let kind = |uf: &mut Uf, x: Var, k: Kind, i: Option<usize>| uf.typed(x, k, &why(i));
        let value_kind = |t: QcType| Kind::from_qc(t).expect("value type");
        match intr {
            // Global addresses are numbers (resolved where they are used).
            I::LoadGlobal(t) => {
                kind(uf, ops[0], Kind::Float, Some(0))?;
                kind(uf, res.unwrap(), value_kind(t), None)?;
            }
            I::StoreGlobal(t) => {
                kind(uf, ops[0], Kind::Float, Some(0))?;
                kind(uf, ops[1], value_kind(t), Some(1))?;
            }
            I::LoadField(t) | I::StoreField(t) => {
                kind(uf, ops[0], Kind::Entity, Some(0))?;
                kind(uf, ops[1], Kind::Field(t), Some(1))?;
                uf.exact(ops[1], &why(Some(1)))?;
                match intr {
                    I::LoadField(_) => kind(uf, res.unwrap(), value_kind(t), None)?,
                    _ => kind(uf, ops[2], value_kind(t), Some(2))?,
                }
            }
            I::Eq(t) | I::Ne(t) | I::Not(t) => {
                for (i, &x) in ops.iter().enumerate() {
                    kind(uf, x, value_kind(t), Some(i))?;
                }
                kind(uf, res.unwrap(), Kind::Float, None)?;
            }
            I::DotVector => {
                kind(uf, ops[0], Kind::Vector, Some(0))?;
                kind(uf, ops[1], Kind::Vector, Some(1))?;
                kind(uf, res.unwrap(), Kind::Float, None)?;
            }
            I::FieldRefString => {
                kind(uf, ops[0], Kind::Field(QcType::String), Some(0))?;
                uf.exact(ops[0], &why(Some(0)))?;
                uf.union(res.unwrap(), ops[0], 0, &why(None))?;
            }
            I::StringLiteral => {
                kind(uf, ops[0], Kind::String, Some(0))?;
                uf.exact(ops[0], &why(Some(0)))?;
                uf.union(res.unwrap(), ops[0], 0, &why(None))?;
            }
        }
        Ok(())
    }

    /// Enumerate the instances to emit, starting from `roots` (exported and
    /// table functions), which get default types.
    pub fn instances(&mut self, ctx: &Ctx<'_>, roots: &[Func]) -> Result<Vec<Instance>> {
        let mut out: Vec<Instance> = Vec::new();
        let mut index: HashMap<(Func, Vec<Subst>), usize> = HashMap::new();
        let mut names: HashMap<Func, usize> = HashMap::new();
        let mut todo = Vec::new();

        let mut add = |f: Func,
                       subst: BTreeMap<Var, (Kind, i64)>,
                       out: &mut Vec<Instance>,
                       todo: &mut Vec<usize>| {
            let key = (f, subst.iter().map(|(&v, &(k, b))| (v, k, b)).collect());
            if let Some(&i) = index.get(&key) {
                return i;
            }
            let n = names.entry(f).or_insert(0);
            let base = &ctx.funcs[&f].name;
            let name = if *n == 0 {
                base.clone()
            } else {
                format!("{base}__{n}")
            };
            *n += 1;
            let i = out.len();
            out.push(Instance {
                func: f,
                name,
                subst,
                callees: HashMap::new(),
            });
            index.insert(key, i);
            todo.push(i);
            i
        };

        for &f in roots {
            let scc = self.funcs[&f].scc;
            let subst = self.quantified[scc]
                .iter()
                .map(|&q| (q, self.default_subst(q)))
                .collect();
            add(f, subst, &mut out, &mut todo);
        }

        while let Some(i) = todo.pop() {
            let f = out[i].func;
            let body = Self::body(ctx, f);
            let mut callees = HashMap::new();
            for (_, b) in body.blocks.entries() {
                for &v in &b.insts {
                    let ValueDef::Operator(Operator::Call { function_index: g }, _, _) =
                        &body.values[v]
                    else {
                        continue;
                    };
                    if ctx.funcs[g].kind != FuncKind::Local {
                        continue;
                    }
                    let subst = match self.funcs[&f].calls.get(&v) {
                        // Same SCC: the same substitution.
                        None => out[i].subst.clone(),
                        Some(inst) => {
                            let inst = inst.clone();
                            let mut s = BTreeMap::new();
                            for (q, fresh) in inst {
                                s.insert(q, self.ground(&out[i].subst, fresh)?);
                            }
                            // Quantified roots not reached by the call.
                            for &q in &self.quantified[self.funcs[g].scc] {
                                s.entry(q).or_insert(self.default_subst(q));
                            }
                            s
                        }
                    };
                    let j = add(*g, subst, &mut out, &mut todo);
                    callees.insert(v, j);
                }
            }
            out[i].callees = callees;
        }
        Ok(out)
    }

    /// The kind and bias of `v` under `subst`.
    fn ground(&mut self, subst: &BTreeMap<Var, (Kind, i64)>, v: Var) -> Result<(Kind, i64)> {
        let (r, pot) = self.uf.find(v);
        let c = &self.uf.class[r as usize];
        let (k, anchor) = match &c.kind {
            Some((k, _)) => (*k, c.anchor.as_ref().map_or(0, |(a, _)| *a)),
            None => match subst.get(&r) {
                Some(&(k, bias)) => (k, bias),
                None => (Kind::default_for(self.uf.wasm[r as usize]), 0),
            },
        };
        if k.addressed() && c.inconsistent {
            bail!("{k} values are computed with inconsistent offsets (e.g. in a loop)");
        }
        if k.addressed()
            && let Some(msg) = &c.conflict
        {
            bail!("{k} {msg}");
        }
        Ok((k, anchor.wrapping_sub(pot)))
    }

    /// The default instantiation of quantified root `q`.
    fn default_subst(&self, q: Var) -> (Kind, i64) {
        let c = &self.uf.class[q as usize];
        let anchor = c.anchor.as_ref().map_or(0, |(a, _)| *a);
        (Kind::default_for(self.uf.wasm[q as usize]), anchor)
    }

    /// The kind and bias of value `v` of `inst`.
    pub fn value(&mut self, inst: &Instance, v: Value) -> Result<(Kind, i64)> {
        let var = self.funcs[&inst.func].vals[&v];
        self.ground(&inst.subst, var)
    }

    /// The kind and bias of operand `idx` of `site`, which is `v`.
    pub fn operand_of(
        &mut self,
        inst: &Instance,
        body: &FunctionBody,
        site: Site,
        idx: u32,
        v: Value,
    ) -> Result<(Kind, i64)> {
        let v = resolve(body, v);
        let var = match self.funcs[&inst.func].uses.get(&(site, idx)) {
            Some(&u) if const_val(body, v).is_some() => u,
            _ => self.funcs[&inst.func].vals[&v],
        };
        self.ground(&inst.subst, var)
    }

    /// The kinds of `inst`'s parameters and results.
    pub fn signature(&mut self, inst: &Instance) -> Result<(Vec<Kind>, Vec<Kind>)> {
        let ft = &self.funcs[&inst.func];
        let (ps, rs) = (ft.params.clone(), ft.rets.clone());
        let mut kinds = |vs: Vec<Var>| -> Result<Vec<Kind>> {
            vs.into_iter()
                .map(|v| Ok(self.ground(&inst.subst, v)?.0))
                .collect()
        };
        Ok((kinds(ps)?, kinds(rs)?))
    }

    /// The kind of a wasm global.
    pub fn global(&mut self, g: Global) -> Kind {
        let v = self.globals[&g];
        self.ground(&BTreeMap::new(), v)
            .map_or(Kind::Float, |(k, _)| k)
    }

    /// The kinds of an unknown builtin's parameters and results.
    pub fn import(&mut self, f: Func) -> Option<(Vec<Kind>, Vec<Kind>)> {
        let (ps, rs) = self.imports.get(&f)?.clone();
        let mut kinds = |vs: Vec<Var>| -> Vec<Kind> {
            vs.into_iter()
                .map(|v| {
                    self.ground(&BTreeMap::new(), v)
                        .map_or(Kind::Float, |(k, _)| k)
                })
                .collect()
        };
        Some((kinds(ps), kinds(rs)))
    }
}

/// A short description of a value's definition, for errors.
pub fn describe(body: &FunctionBody, v: Value) -> String {
    match &body.values[v] {
        ValueDef::Operator(op, _, _) => format!("{op:?}"),
        ValueDef::BlockParam(b, i, _) => format!("parameter {i} of {b}"),
        other => format!("{other:?}"),
    }
}

/// The strongly connected components of the call graph of `funcs`, callees
/// before callers (Tarjan).
fn sccs(ctx: &Ctx<'_>, funcs: &[Func]) -> Vec<Vec<Func>> {
    struct T {
        edges: HashMap<Func, Vec<Func>>,
        index: HashMap<Func, usize>,
        low: HashMap<Func, usize>,
        on: Vec<Func>,
        out: Vec<Vec<Func>>,
    }
    fn visit(t: &mut T, f: Func) {
        let i = t.index.len();
        t.index.insert(f, i);
        t.low.insert(f, i);
        t.on.push(f);
        for g in t.edges[&f].clone() {
            if !t.index.contains_key(&g) {
                visit(t, g);
                let l = t.low[&g].min(t.low[&f]);
                t.low.insert(f, l);
            } else if t.on.contains(&g) {
                let l = t.index[&g].min(t.low[&f]);
                t.low.insert(f, l);
            }
        }
        if t.low[&f] == t.index[&f] {
            let mut scc = Vec::new();
            loop {
                let g = t.on.pop().unwrap();
                scc.push(g);
                if g == f {
                    break;
                }
            }
            scc.reverse();
            t.out.push(scc);
        }
    }
    let edges = funcs
        .iter()
        .map(|&f| {
            let body = ctx.module.funcs[f].body().expect("local function");
            let mut cs = callees(body);
            cs.retain(|g| ctx.funcs[g].kind == FuncKind::Local);
            cs.dedup();
            (f, cs)
        })
        .collect();
    let mut t = T {
        edges,
        index: HashMap::new(),
        low: HashMap::new(),
        on: Vec::new(),
        out: Vec::new(),
    };
    for &f in funcs {
        if !t.index.contains_key(&f) {
            visit(&mut t, f);
        }
    }
    t.out
}
