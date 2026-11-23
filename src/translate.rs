//! Module-level translation: prepare the module and parse it into SSA,
//! classify imports and exports, infer types, emit each function instance,
//! then render QuakeC source.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use anyhow::{Context as _, Result, bail, ensure};
use waffle::entity::EntityRef;
use waffle::{ExportKind, Func, FuncDecl, Global, ImportKind, Module, Type};

use crate::data::{self, DATA_BASE, DataKind, StaticData};
use crate::emit;
use crate::infer::{Infer, Kind};
use crate::prepare::prepare;
use crate::qc::{Expr, QcType, Sig, Ty, TypeVars};
use crate::sysdefs::{self, GlobalMap, SysDefs};

/// Import module for numbered builtins. Vanilla NetQuake builtins are
/// recognised by name and emitted as `= #N`; any others are emitted as plain
/// prototypes, and the defs (e.g. `defs.qc`) must supply their definitions.
pub const BUILTIN_MODULE: &str = "builtin";
/// Import module for builtins without a number. These are emitted as
/// `= #0`, which the engine resolves by name.
pub const UNNAMED_BUILTIN_MODULE: &str = "unnamed-builtin";
/// Import module for memory, the function table and `__quakec_*` intrinsics.
pub const ENV_MODULE: &str = "env";
/// The table holding the targets of Zig function pointers.
pub const INDIRECT_TABLE: &str = "__indirect_function_table";
/// An exported (data) symbol naming the system defs to use, e.g.
/// `"quakeworld"`. Defaults to vanilla NetQuake.
pub const TARGET_EXPORT: &str = "__quakec_target";
/// The value given to the imported `__table_base`, so that function pointers
/// are plain table indices.
pub const TABLE_BASE: u64 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intrinsic {
    /// `__quakec_load_<ty>(addr)`: read the QuakeC global at `addr`.
    LoadGlobal(QcType),
    /// `__quakec_store_<ty>(addr, value)`
    StoreGlobal(QcType),
    /// `__quakec_eq_<ty>(a, b)`
    Eq(QcType),
    /// `__quakec_ne_<ty>(a, b)`
    Ne(QcType),
    /// `__quakec_not_<ty>(a)`
    Not(QcType),
    /// `__quakec_dot_vector(a, b)`: `a * b` on vectors (`MUL_V`).
    DotVector,
    /// `__quakec_field_ref_string(field)`: a `.string` field reference.
    FieldRefString,
    /// `__quakec_string_literal(ptr)`: the NUL-terminated literal at `ptr`.
    StringLiteral,
    /// `__quakec_load_entity_field_<ty>(ent, field)`
    LoadField(QcType),
    /// `__quakec_store_entity_field_<ty>(ent, field, value)`
    StoreField(QcType),
}

impl Intrinsic {
    fn from_name(name: &str) -> Option<Self> {
        let ty = |t: &str| {
            Some(match t {
                "float" => QcType::Float,
                "vector" => QcType::Vector,
                "string" => QcType::String,
                "entity" => QcType::Entity,
                "fn" => QcType::Func,
                _ => return None,
            })
        };
        let name = name.strip_prefix("__quakec_")?;
        Some(match name {
            "dot_vector" => Self::DotVector,
            "field_ref_string" => Self::FieldRefString,
            "string_literal" => Self::StringLiteral,
            _ => {
                let (op, t) = name.split_once('_')?;
                match op {
                    "load" => match t.strip_prefix("entity_field_") {
                        Some(t) => Self::LoadField(ty(t)?),
                        None => Self::LoadGlobal(ty(t)?),
                    },
                    "store" => match t.strip_prefix("entity_field_") {
                        Some(t) => Self::StoreField(ty(t)?),
                        None => Self::StoreGlobal(ty(t)?),
                    },
                    "eq" => Self::Eq(ty(t)?),
                    "ne" => Self::Ne(ty(t)?),
                    "not" => Self::Not(ty(t)?),
                    _ => return None,
                }
            }
        })
    }

    /// The number of wasm parameters and results it must have.
    fn arity(self) -> (usize, usize) {
        match self {
            Self::LoadGlobal(_) | Self::Not(_) | Self::FieldRefString | Self::StringLiteral => {
                (1, 1)
            }
            Self::StoreGlobal(_) => (2, 0),
            Self::Eq(_) | Self::Ne(_) | Self::DotVector | Self::LoadField(_) => (2, 1),
            Self::StoreField(_) => (3, 0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuncKind {
    /// Defined in this module.
    Local,
    /// Imported from the `builtin` module: `= #N` if it is a known vanilla
    /// builtin, otherwise a prototype.
    Builtin(Option<u32>),
    /// Imported from the `unnamed-builtin` module; becomes `= #0`.
    UnnamedBuiltin,
    Intrinsic(Intrinsic),
}

pub struct FuncInfo {
    pub name: String,
    pub kind: FuncKind,
}

/// State shared by all of the translation.
pub struct Ctx<'m> {
    pub module: &'m Module<'m>,
    /// The target engine's system defs.
    pub defs: &'static SysDefs,
    pub globals: GlobalMap,
    pub funcs: HashMap<Func, FuncInfo>,
    /// Wasm globals that become QuakeC globals, by name.
    pub wasm_globals: HashMap<Global, String>,
    pub warnings: Vec<String>,
    pub data: StaticData,
    /// Accessor functions needed for dynamic loads from static data.
    pub data_accessors: BTreeSet<DataKind>,
    /// Accessors needed for dynamic loads of pointers (string literals or
    /// functions) from static data, by the type loaded.
    pub pointer_accessors: BTreeSet<QcType>,
    /// The imported `__stack_pointer` global, if any. Functions' uses of it
    /// are reified (see `stack.rs`), except those in `stack_failures`.
    pub stack_pointer: Option<Global>,
    pub stack_failures: HashMap<Func, crate::stack::StackFailure>,
    /// Functions in `__indirect_function_table`, by index.
    pub table_funcs: HashMap<u64, Func>,
    /// Globals and fields declared by the module (by name, see
    /// [`Ctx::user_name`]), with their types.
    pub user_globals: BTreeMap<String, QcType>,
    pub user_fields: BTreeMap<String, QcType>,
}

impl Ctx<'_> {
    /// Read the name of a user global or field: `addr` points to a
    /// NUL-terminated identifier in static data. Records it with type `ty`.
    pub fn user_name(&mut self, addr: u64, ty: QcType, field: bool) -> Result<String> {
        let what = if field { "field" } else { "global" };
        let bytes = self.data.c_string(addr).with_context(|| {
            format!("{what} name at address {addr} is not a NUL-terminated string in static data")
        })?;
        let name = String::from_utf8(bytes)
            .ok()
            .filter(|n| is_identifier(n))
            .with_context(|| {
                format!("{what} name at address {addr} is not a valid QuakeC identifier")
            })?;
        ensure!(
            !self.defs.is_sys_name(&name),
            "user {what} `{name}` has the same name as a system global or field"
        );
        let map = if field {
            &mut self.user_fields
        } else {
            &mut self.user_globals
        };
        match map.get(&name) {
            Some(&t) if t != ty => bail!(
                "user {what} `{name}` is accessed as both {} and {}",
                t.name(),
                ty.name()
            ),
            _ => {
                map.insert(name.clone(), ty);
            }
        }
        Ok(name)
    }

    /// The local functions, in index order.
    pub fn local_funcs(&self) -> Vec<Func> {
        let mut fs: Vec<Func> = self
            .funcs
            .iter()
            .filter(|(_, i)| i.kind == FuncKind::Local)
            .map(|(&f, _)| f)
            .collect();
        fs.sort();
        fs
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Emit the vanilla system defs prelude at the top of the output.
    pub prelude: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { prelude: true }
    }
}

pub struct Output {
    pub source: String,
    pub warnings: Vec<String>,
}

pub fn translate(wasm: &[u8], opts: &Options) -> Result<Output> {
    let prepared = prepare(wasm)?;
    let mut module = Module::from_wasm_bytes(&prepared.wasm, &Default::default())
        .context("failed to parse wasm module")?;
    module
        .expand_all_funcs()
        .context("failed to parse wasm functions")?;
    let stack_pointer = module.imports.iter().find_map(|i| match i.kind {
        ImportKind::Global(g) if i.module == ENV_MODULE && i.name == "__stack_pointer" => Some(g),
        _ => None,
    });
    let mut stack_failures = HashMap::new();
    for (func, f) in module.funcs.entries_mut() {
        if let Some(body) = f.body_mut() {
            if let Some(sp) = stack_pointer
                && let Err(failure) = crate::stack::reify(body, sp)
            {
                stack_failures.insert(func, failure);
            }
            crate::prepare::simplify_params(body);
            crate::prepare::merge_blocks(body);
            crate::prepare::split_i64_moves(body);
            crate::prepare::split_splat_loads(body);
            crate::prepare::duplicate_offset_blocks(body);
            crate::prepare::narrow_extended_lanes(body);
            crate::prepare::float_sign_tests(body);
            crate::prepare::bool_offsets_to_select(body);
            // Passes rewrite terminators directly; emission uses the CFG.
            body.recompute_edges();
        }
    }
    let module = module;

    ensure!(
        module.passive_data_segments.is_empty(),
        "passive data segments are not supported"
    );
    let mut segments = Vec::new();
    for (_, mem) in module.memories.entries() {
        for seg in &mem.segments {
            segments.push((seg.offset as u64, seg.data.as_slice()));
        }
    }
    let mut data = StaticData::from_segments(&segments)?;
    // Position-independent modules store the absolute addresses of pointers
    // in static data at load time. Do it now.
    let relocs = module.exports.iter().find_map(|e| match e.kind {
        ExportKind::Func(f) if e.name == "__wasm_apply_data_relocs" => Some(f),
        _ => None,
    });
    if let Some(f) = relocs {
        apply_relocs(&module, f, &mut data).context("in `__wasm_apply_data_relocs`")?;
    }
    let defs = target(&module, &mut data)?;

    let mut ctx = Ctx {
        module: &module,
        defs,
        globals: defs.global_map(),
        funcs: HashMap::new(),
        wasm_globals: HashMap::new(),
        warnings: Vec::new(),
        data,
        data_accessors: BTreeSet::new(),
        pointer_accessors: BTreeSet::new(),
        stack_pointer: None,
        stack_failures,
        table_funcs: HashMap::new(),
        user_globals: BTreeMap::new(),
        user_fields: BTreeMap::new(),
    };

    // Imports.
    let mut indirect_table = None;
    let mut imported_globals = Vec::new();
    for import in &module.imports {
        match import.kind {
            ImportKind::Memory(_) => {}
            ImportKind::Table(t) => {
                if import.module == ENV_MODULE && import.name == INDIRECT_TABLE {
                    indirect_table = Some(t);
                }
            }
            ImportKind::Func(f) => {
                let kind = import_kind(&ctx, f, &import.module, &import.name)?;
                ctx.funcs.insert(
                    f,
                    FuncInfo {
                        name: import.name.clone(),
                        kind,
                    },
                );
            }
            // Static data and function pointers are placed at fixed bases
            // (see `prepare.rs`). `__stack_pointer` is handled by
            // `stack.rs`.
            ImportKind::Global(g) => {
                imported_globals.push(g);
                match (import.module.as_str(), import.name.as_str()) {
                    (ENV_MODULE, "__stack_pointer") => ctx.stack_pointer = Some(g),
                    (ENV_MODULE, "__memory_base" | "__table_base") => {}
                    _ => bail!(
                        "imported global `{}.{}` is not supported",
                        import.module,
                        import.name
                    ),
                }
            }
        }
    }

    // The function table.
    if let Some(t) = indirect_table
        && let Some(elems) = &module.tables[t].func_elements
    {
        for (i, &f) in elems.iter().enumerate() {
            if f.is_valid() {
                ctx.table_funcs.insert(i as u64, f);
            }
        }
    }

    // Wasm globals become QuakeC globals. Exported ones are data symbols
    // (like `__quakec_target`), which code doesn't use.
    let exported_globals: Vec<Global> = module
        .exports
        .iter()
        .filter_map(|e| match e.kind {
            ExportKind::Global(g) => Some(g),
            _ => None,
        })
        .collect();
    for (g, _) in module.globals.entries() {
        if !imported_globals.contains(&g) && !exported_globals.contains(&g) {
            let name = format!("__wasm_g{}", ctx.wasm_globals.len());
            ctx.wasm_globals.insert(g, name);
        }
    }

    // Local function names: exported name if any, else a generated one.
    let mut export_names: HashMap<Func, String> = HashMap::new();
    for e in &module.exports {
        if let ExportKind::Func(f) = e.kind {
            export_names.entry(f).or_insert_with(|| e.name.clone());
        }
    }
    for (f, decl) in module.funcs.entries() {
        if let FuncDecl::Body(_, debug_name, _) = decl {
            // Internal functions are prefixed so they can't collide with
            // QuakeC globals, fields or builtins.
            let name = match export_names.get(&f) {
                Some(export) => export.clone(),
                None if !debug_name.is_empty() => format!("__wasm_{}", sanitize(debug_name)),
                None => format!("__wasm_f{}", prepared.original_index[f.index()]),
            };
            ctx.funcs.insert(
                f,
                FuncInfo {
                    name,
                    kind: FuncKind::Local,
                },
            );
        }
    }

    // Types, and the instances of each function to emit.
    let mut inf = Infer::run(&ctx)?;
    let mut roots: Vec<Func> = export_names.keys().copied().collect();
    roots.extend(ctx.table_funcs.values().copied());
    roots.extend(module.start_func);
    roots.retain(|f| ctx.funcs[f].kind == FuncKind::Local && Some(*f) != relocs);
    roots.sort();
    roots.dedup();
    let instances = inf.instances(&ctx, &roots)?;

    let mut sigs = HashMap::new();
    for inst in &instances {
        let (ps, rs) = inf.signature(inst)?;
        ensure!(
            ps.len() <= 8,
            "QuakeC functions take at most 8 parameters, but `{}` takes {}",
            inst.name,
            ps.len()
        );
        ensure!(
            rs.len() <= 1,
            "multi-value returns are not supported (in `{}`)",
            inst.name
        );
        sigs.insert(inst.name.clone(), sig_of(&ps, &rs));
    }
    for (f, info) in &ctx.funcs {
        if let FuncKind::Builtin(_) | FuncKind::UnnamedBuiltin = info.kind {
            let (ps, rs) = match ctx
                .defs
                .builtin(&info.name)
                .filter(|_| matches!(info.kind, FuncKind::Builtin(Some(_))))
            {
                Some(b) => (
                    b.params
                        .iter()
                        .map(|&t| Kind::from_qc(t).unwrap())
                        .collect(),
                    Kind::from_qc(b.ret).into_iter().collect(),
                ),
                None => inf.import(*f).unwrap_or_default(),
            };
            sigs.insert(info.name.clone(), sig_of(&ps, &rs));
        }
    }

    let mut functions = Vec::new();
    for (i, inst) in instances.iter().enumerate() {
        let ((f, i), func) = emit::emit(&mut ctx, &mut inf, &instances, i, &sigs)
            .with_context(|| format!("while translating function `{}`", inst.name))?;
        functions.push(((prepared.original_index[f.index()], i), func));
    }
    // In the original index order, so that output is stable.
    functions.sort_by_key(|(order, _)| *order);
    let functions: Vec<_> = functions.into_iter().map(|(_, f)| f).collect();

    let mut tv = TypeVars::default();
    let warnings = std::mem::take(&mut ctx.warnings);

    // ---- render ----
    let mut out = String::new();
    out.push_str("// Generated by wasm-to-qc. Do not edit.\n\n");

    let sys_func_names = ctx.defs.func_names();

    if opts.prelude {
        let protos: HashMap<String, String> = functions
            .iter()
            .filter(|f| sys_func_names.contains(&f.name.as_str()))
            .map(|f| (f.name.clone(), f.render_proto(&mut tv)))
            .collect();
        out.push_str(&ctx.defs.prelude(|n| protos.get(n).cloned()));
        out.push('\n');
    }

    if !ctx.user_globals.is_empty() || !ctx.user_fields.is_empty() {
        out.push_str("// ---- user globals and fields ----\n");
        for (name, ty) in &ctx.user_globals {
            match ty {
                QcType::Func => writeln!(out, "var void() {name};"),
                ty => writeln!(out, "{} {name};", ty.name()),
            }
            .unwrap();
        }
        for (name, ty) in &ctx.user_fields {
            let _ = writeln!(out, ".{} {name};", ty.name());
        }
        out.push('\n');
    }

    if !ctx.wasm_globals.is_empty() {
        out.push_str("// ---- wasm globals ----\n");
        let mut gs: Vec<_> = ctx.wasm_globals.iter().collect();
        gs.sort_by(|a, b| a.1.cmp(b.1));
        for (&g, name) in gs {
            let _ = writeln!(out, "{} {name};", inf.global(g).qc().name());
        }
        out.push('\n');
    }

    let mut imports: Vec<_> = ctx
        .funcs
        .values()
        .filter(|f| matches!(f.kind, FuncKind::Builtin(_) | FuncKind::UnnamedBuiltin))
        .collect();
    imports.sort_by(|a, b| a.name.cmp(&b.name));
    if !imports.is_empty() {
        out.push_str("// ---- imports ----\n");
        for f in imports {
            let sig = sigs[&f.name].render(&mut tv);
            match f.kind {
                FuncKind::Builtin(Some(n)) => {
                    let _ = writeln!(out, "{sig} {} = #{n};", f.name);
                }
                FuncKind::UnnamedBuiltin => {
                    let _ = writeln!(out, "{sig} {} = #0;", f.name);
                }
                _ => {
                    let _ = writeln!(out, "{sig} {};", f.name);
                }
            }
        }
        out.push('\n');
    }

    let mut pointer_accessors = Vec::new();
    for &ty in &ctx.pointer_accessors {
        let mut entries = Vec::new();
        for (addr, value) in ctx.data.pointers() {
            let e = match ty {
                QcType::String => match ctx.data.c_string(value as u64) {
                    Some(bytes) => Expr::Str(bytes),
                    None => continue,
                },
                _ => match ctx.table_funcs.get(&(value as u64)) {
                    Some(f) => {
                        // Function values are `void()` (see `emit`).
                        let name = ctx.funcs[f].name.clone();
                        let void = sigs.get(&name) == Some(&Sig::void());
                        let f = Expr::FuncName(name);
                        if void {
                            f
                        } else {
                            Expr::FnCast(Sig::void(), Box::new(f))
                        }
                    }
                    None => continue,
                },
            };
            entries.push((addr as f32, e));
        }
        pointer_accessors.push(data::lookup(
            &pointer_accessor_name(ty),
            ty,
            &entries,
            Expr::Null,
        ));
    }
    if !ctx.data_accessors.is_empty() || !pointer_accessors.is_empty() {
        out.push_str("// ---- static data accessors ----\n");
        for f in &pointer_accessors {
            f.render(&mut tv, &mut out);
            out.push('\n');
        }
        for &kind in &ctx.data_accessors {
            ctx.data.accessor(kind).render(&mut tv, &mut out);
            out.push('\n');
        }
    }

    out.push_str("// ---- forward declarations ----\n");
    for f in &functions {
        if !(opts.prelude && sys_func_names.contains(&f.name.as_str())) {
            let _ = writeln!(out, "{};", f.render_proto(&mut tv));
        }
    }
    out.push('\n');

    for f in &functions {
        f.render(&mut tv, &mut out);
        out.push('\n');
    }

    Ok(Output {
        source: out,
        warnings,
    })
}

fn sig_of(ps: &[Kind], rs: &[Kind]) -> Sig {
    Sig {
        params: ps.iter().map(|k| Ty::Known(k.qc())).collect(),
        ret: Ty::Known(rs.first().map_or(QcType::Void, |k| k.qc())),
    }
}

/// Classify an imported function.
fn import_kind(ctx: &Ctx<'_>, f: Func, module: &str, name: &str) -> Result<FuncKind> {
    let sig = &ctx.module.signatures[ctx.module.funcs[f].sig()];
    Ok(match module {
        BUILTIN_MODULE => match ctx.defs.builtin(name) {
            Some(b) => {
                check_builtin_sig(b, &sig.params, &sig.returns).with_context(|| {
                    format!(
                        "in import of {} builtin `{}` (#{})",
                        ctx.defs.name, b.name, b.number
                    )
                })?;
                FuncKind::Builtin(Some(b.number))
            }
            None => FuncKind::Builtin(None),
        },
        UNNAMED_BUILTIN_MODULE => FuncKind::UnnamedBuiltin,
        ENV_MODULE => match Intrinsic::from_name(name) {
            Some(i) => {
                let (np, nr) = i.arity();
                ensure!(
                    sig.params.len() == np && sig.returns.len() == nr,
                    "intrinsic `{name}` should take {np} parameters and return {nr} values"
                );
                FuncKind::Intrinsic(i)
            }
            None if name.starts_with("__quakec_") => {
                bail!("unknown QuakeC intrinsic `{name}`")
            }
            None => bail!(
                "unknown import `{ENV_MODULE}.{name}`; QuakeC builtins must be imported \
                 from `{BUILTIN_MODULE}` or `{UNNAMED_BUILTIN_MODULE}`"
            ),
        },
        other => bail!(
            "unsupported import module `{other}` (for `{name}`); expected \
             `{ENV_MODULE}`, `{BUILTIN_MODULE}` or `{UNNAMED_BUILTIN_MODULE}`"
        ),
    })
}

/// Find the system defs named by the exported `__quakec_target` string.
fn target(module: &Module<'_>, data: &mut StaticData) -> Result<&'static SysDefs> {
    let Some(export) = module.exports.iter().find(|e| e.name == TARGET_EXPORT) else {
        return Ok(&sysdefs::VANILLA);
    };
    let ExportKind::Global(g) = export.kind else {
        bail!("`{TARGET_EXPORT}` must be exported data")
    };
    let Some(offset) = module.globals[g].value else {
        bail!("`{TARGET_EXPORT}` must be exported data");
    };
    let name = data
        .c_string(DATA_BASE + offset as u32 as u64)
        .and_then(|b| String::from_utf8(b).ok())
        .with_context(|| format!("`{TARGET_EXPORT}` must be a NUL-terminated string"))?;
    sysdefs::target(&name).with_context(|| format!("unknown `{TARGET_EXPORT}` `{name}`"))
}

pub fn is_identifier(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Make a wasm debug name usable as a QuakeC identifier.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Whether a wasm value of type `w` can hold a QuakeC value of type `q`.
fn compatible(w: Type, q: QcType) -> bool {
    match w {
        Type::F32 => q == QcType::Float,
        Type::V128 => q == QcType::Vector,
        Type::I32 => !matches!(q, QcType::Vector | QcType::Void),
        _ => false,
    }
}

/// Check an imported builtin's wasm signature against its QuakeC signature.
fn check_builtin_sig(b: &sysdefs::Builtin, params: &[Type], rets: &[Type]) -> Result<()> {
    if params.len() != b.params.len() {
        bail!(
            "expected {} parameters, but the import has {}",
            b.params.len(),
            params.len()
        );
    }
    for (i, (&w, &q)) in params.iter().zip(b.params).enumerate() {
        ensure!(
            compatible(w, q),
            "parameter {i} should be {}, but the import has {}",
            q.name(),
            describe_type(w)
        );
    }
    match (rets, b.ret) {
        ([], QcType::Void) => {}
        ([w], q) if compatible(*w, q) => {}
        ([w], q) => bail!(
            "return type should be {}, but the import returns {}",
            q.name(),
            describe_type(*w)
        ),
        (rets, q) => bail!(
            "return type should be {}, but the import returns {} values",
            q.name(),
            rets.len()
        ),
    }
    Ok(())
}

/// The accessor for dynamic loads of pointers of type `ty` (see
/// [`Ctx::pointer_accessors`]).
pub fn pointer_accessor_name(ty: QcType) -> String {
    match ty {
        QcType::String => "__wasm_data_string".into(),
        _ => "__wasm_data_fn".into(),
    }
}

/// Run `__wasm_apply_data_relocs` (function `f`): constant stores of
/// pointers into static data.
fn apply_relocs(module: &Module, f: Func, data: &mut StaticData) -> Result<()> {
    let body = module.funcs[f].body().expect("local function");
    for (_, block) in body.blocks.entries() {
        for &v in &block.insts {
            if crate::infer::const_val(body, v).is_some() {
                continue;
            }
            let waffle::ValueDef::Operator(op, args, _) = &body.values[v] else {
                continue;
            };
            let waffle::Operator::I32Store { memory } = op else {
                bail!("unsupported relocation `{op:?}`");
            };
            let a = &body.arg_pool[*args];
            let (Some(addr), Some(value)) = (
                crate::infer::const_val(body, a[0]),
                crate::infer::const_val(body, a[1]),
            ) else {
                bail!("relocation with a non-constant address or value");
            };
            data.relocate((addr + memory.offset as i64) as u64, value as u32)?;
        }
    }
    Ok(())
}

fn describe_type(t: Type) -> String {
    match t {
        Type::I32 => "an i32".into(),
        Type::F32 => "an f32".into(),
        Type::V128 => "a v128".into(),
        Type::I64 => "an i64 (64-bit integers aren't supported)".into(),
        t => format!("{t:?}"),
    }
}
