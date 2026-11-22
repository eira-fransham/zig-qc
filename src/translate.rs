//! Module-level translation: classify imports/exports, lower each function,
//! then resolve types and render QuakeC source.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use anyhow::{Context as _, Result, bail, ensure};
use walrus::ir::Value;
use walrus::{
    ConstExpr, ElementItems, ElementKind, ExportItem, FunctionId, FunctionKind, GlobalId,
    GlobalKind, ImportKind, Module, TableId, ValType,
};

use crate::data::{DATA_BASE, DataKind, StaticData, relative_offset};
use crate::lower;
use crate::qc::{QcType, Sig, Ty, TypeVars};
use crate::sysdefs::{self, GlobalMap, SysDefs};

/// The name of the table LLVM uses to call `funcref`s.
pub const CALL_TABLE: &str = "__funcref_call_table";
/// Import module for numbered builtins. Vanilla NetQuake builtins are
/// recognised by name and emitted as `= #N`; any others are emitted as plain
/// prototypes, and the defs (e.g. `defs.qc`) must supply their definitions.
pub const BUILTIN_MODULE: &str = "builtin";
/// Import module for builtins without a number. These are emitted as
/// `= #0`, which the engine resolves by name.
pub const UNNAMED_BUILTIN_MODULE: &str = "unnamed-builtin";
/// Import module for memory, the call table and `__quakec_*` intrinsics.
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
    LoadFloat,
    LoadVector,
    StoreFloat,
    StoreVector,
    LoadEntity,
    LoadString,
    LoadFn,
    StoreEntity,
    StoreString,
    StoreFn,
    EqString,
    NeString,
    EqEntity,
    NeEntity,
    NotString,
    NotEntity,
    EqFn,
    NeFn,
    NotFn,
    EqVector,
    NeVector,
    NotVector,
    /// `__quakec_dot_vector(a, b)`: `a * b` on vectors (`MUL_V`).
    DotVector,
    /// `__quakec_func_ref(fnptr)`: a Zig function pointer as a QuakeC function.
    FuncRef,
    /// `__quakec_field_ref_string(field)`: a `.string` field reference.
    FieldRefString,
    /// `__quakec_string_literal(ptr, len)`
    StringLiteral,
    /// `__quakec_load_entity_field_<ty>(ent, field)`
    LoadField(QcType),
    /// `__quakec_store_entity_field_<ty>(ent, field, value)`
    StoreField(QcType),
}

impl Intrinsic {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "__quakec_load_float" => Self::LoadFloat,
            "__quakec_load_vector" => Self::LoadVector,
            "__quakec_store_float" => Self::StoreFloat,
            "__quakec_store_vector" => Self::StoreVector,
            "__quakec_load_entity" => Self::LoadEntity,
            "__quakec_load_string" => Self::LoadString,
            "__quakec_load_fn" => Self::LoadFn,
            "__quakec_store_entity" => Self::StoreEntity,
            "__quakec_store_string" => Self::StoreString,
            "__quakec_store_fn" => Self::StoreFn,
            "__quakec_eq_string" => Self::EqString,
            "__quakec_ne_string" => Self::NeString,
            "__quakec_eq_entity" => Self::EqEntity,
            "__quakec_ne_entity" => Self::NeEntity,
            "__quakec_not_string" => Self::NotString,
            "__quakec_not_entity" => Self::NotEntity,
            "__quakec_eq_fn" => Self::EqFn,
            "__quakec_ne_fn" => Self::NeFn,
            "__quakec_not_fn" => Self::NotFn,
            "__quakec_eq_vector" => Self::EqVector,
            "__quakec_ne_vector" => Self::NeVector,
            "__quakec_not_vector" => Self::NotVector,
            "__quakec_dot_vector" => Self::DotVector,
            "__quakec_func_ref" => Self::FuncRef,
            "__quakec_field_ref_string" => Self::FieldRefString,
            "__quakec_string_literal" => Self::StringLiteral,
            _ => {
                let (load, ty) = if let Some(t) = name.strip_prefix("__quakec_load_entity_field_") {
                    (true, t)
                } else {
                    (false, name.strip_prefix("__quakec_store_entity_field_")?)
                };
                let ty = match ty {
                    "float" => QcType::Float,
                    "vector" => QcType::Vector,
                    "string" => QcType::String,
                    "entity" => QcType::Entity,
                    "fn" => QcType::Func,
                    _ => return None,
                };
                if load {
                    Self::LoadField(ty)
                } else {
                    Self::StoreField(ty)
                }
            }
        })
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
    pub sig: Sig,
    pub kind: FuncKind,
}

/// State shared by all function lowerings.
pub struct Ctx<'m> {
    pub module: &'m Module,
    /// The target engine's system defs.
    pub defs: &'static SysDefs,
    pub globals: GlobalMap,
    pub funcs: HashMap<FunctionId, FuncInfo>,
    pub wasm_globals: HashMap<GlobalId, (String, Ty)>,
    pub call_table: Option<TableId>,
    pub tv: TypeVars,
    pub warnings: Vec<String>,
    /// The imported `__memory_base` global, if any.
    pub memory_base: Option<GlobalId>,
    pub data: StaticData,
    /// Accessor functions needed for dynamic loads from static data.
    pub data_accessors: BTreeSet<DataKind>,
    /// The imported `__table_base` global, if any.
    pub table_base: Option<GlobalId>,
    /// Functions in `__indirect_function_table`, by index.
    pub table_funcs: HashMap<u64, FunctionId>,
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

    /// Convert a wasm value type into a (possibly inferred) QuakeC type.
    pub fn val_ty(&mut self, t: ValType) -> Result<Ty> {
        Ok(match t {
            ValType::I32 | ValType::F32 => Ty::Known(QcType::Float),
            ValType::Ref(r) if r == walrus::RefType::EXTERNREF => self.tv.fresh(),
            ValType::Ref(r) if r == walrus::RefType::FUNCREF => Ty::Known(QcType::Func),
            // `@Vector(3, f32)`; the fourth lane is ignored.
            ValType::V128 => Ty::Known(QcType::Vector),
            other => bail!(
                "unsupported value type {other:?} (only i32, f32, v128, externref and funcref are supported)"
            ),
        })
    }

    pub fn sig_of_type(&mut self, ty: walrus::TypeId) -> Result<Sig> {
        let t = self.module.types.get(ty);
        let params = t
            .params()
            .iter()
            .map(|&p| self.val_ty(p))
            .collect::<Result<Vec<_>>>()?;
        if params.len() > 8 {
            bail!(
                "QuakeC functions take at most 8 parameters, found {}",
                params.len()
            );
        }
        let ret = match t.results() {
            [] => Ty::Known(QcType::Void),
            [r] => self.val_ty(*r)?,
            _ => bail!("multi-value returns are not supported"),
        };
        Ok(Sig { params, ret })
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
    let module = Module::from_buffer(wasm).context("failed to parse wasm module")?;

    // `__memory_base` and `__table_base` are needed to find static data
    // (and so the target) and the function table before anything else.
    let mut memory_base = None;
    let mut table_base = None;
    for import in module.imports.iter() {
        if let ImportKind::Global(g) = import.kind
            && import.module == ENV_MODULE
        {
            match import.name.as_str() {
                "__memory_base" => memory_base = Some(g),
                "__table_base" => table_base = Some(g),
                _ => {}
            }
        }
    }
    let mut data = StaticData::from_module(&module, memory_base)?;
    let defs = target(&module, &mut data)?;

    let mut ctx = Ctx {
        module: &module,
        defs,
        globals: defs.global_map(),
        funcs: HashMap::new(),
        wasm_globals: HashMap::new(),
        call_table: None,
        tv: TypeVars::default(),
        warnings: Vec::new(),
        memory_base,
        data,
        data_accessors: BTreeSet::new(),
        table_base,
        table_funcs: HashMap::new(),
        user_globals: BTreeMap::new(),
        user_fields: BTreeMap::new(),
    };

    // Tables: the LLVM funcref call table, and the table of Zig function
    // pointers (only used through `__quakec_func_ref`).
    let mut indirect_table = None;
    for table in module.tables.iter() {
        let imported_as = |name: &str| {
            module.imports.iter().any(|i| {
                i.module == ENV_MODULE
                    && i.name == name
                    && matches!(i.kind, ImportKind::Table(t) if t == table.id())
            })
        };
        if imported_as(INDIRECT_TABLE) {
            indirect_table = Some(table.id());
            continue;
        }
        let is_call_table = module.exports.iter().any(|e| {
            e.name == CALL_TABLE && matches!(e.item, ExportItem::Table(t) if t == table.id())
        }) || table.name.as_deref() == Some(CALL_TABLE)
            || imported_as(CALL_TABLE);
        if !is_call_table {
            bail!(
                "unsupported table {:?}; only `{CALL_TABLE}` and `{INDIRECT_TABLE}` may be used",
                table.name
            );
        }
        ensure!(ctx.call_table.is_none(), "more than one `{CALL_TABLE}`");
        ctx.call_table = Some(table.id());
    }
    for el in module.elements.iter() {
        let ElementKind::Active { table, offset } = &el.kind else {
            continue;
        };
        ensure!(
            Some(*table) == indirect_table,
            "element segment for an unsupported table"
        );
        let base = relative_offset(offset, table_base)
            .or_else(|| match offset {
                ConstExpr::Value(Value::I32(k)) => usize::try_from(*k).ok(),
                _ => None,
            })
            .with_context(|| {
                format!("element segment at {offset:?} is not relative to `__table_base`")
            })?;
        let ElementItems::Functions(fs) = &el.items else {
            bail!("unsupported element segment items")
        };
        for (i, &f) in fs.iter().enumerate() {
            ctx.table_funcs.insert(TABLE_BASE + (base + i) as u64, f);
        }
    }

    // Imports.
    for import in module.imports.iter() {
        match import.kind {
            ImportKind::Memory(_) => {}
            ImportKind::Function(f) => {
                let ty = module.funcs.get(f).ty();
                let sig = ctx
                    .sig_of_type(ty)
                    .with_context(|| format!("in signature of import `{}`", import.name))?;
                let kind = match import.module.as_str() {
                    BUILTIN_MODULE => match ctx.defs.builtin(&import.name) {
                        Some(b) => {
                            check_builtin_sig(&mut ctx.tv, b, &sig).with_context(|| {
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
                    ENV_MODULE => match Intrinsic::from_name(&import.name) {
                        Some(i) => FuncKind::Intrinsic(i),
                        None if import.name.starts_with("__quakec_") => {
                            bail!("unknown QuakeC intrinsic `{}`", import.name)
                        }
                        None => bail!(
                            "unknown import `{ENV_MODULE}.{}`; QuakeC builtins must be imported \
                             from `{BUILTIN_MODULE}` or `{UNNAMED_BUILTIN_MODULE}`",
                            import.name
                        ),
                    },
                    other => bail!(
                        "unsupported import module `{other}` (for `{}`); expected \
                         `{ENV_MODULE}`, `{BUILTIN_MODULE}` or `{UNNAMED_BUILTIN_MODULE}`",
                        import.name
                    ),
                };
                ctx.funcs.insert(
                    f,
                    FuncInfo {
                        name: import.name.clone(),
                        sig,
                        kind,
                    },
                );
            }
            ImportKind::Table(_) => {}
            // Static data is placed relative to `__memory_base` (see `data.rs`),
            // and function pointers relative to `__table_base`.
            ImportKind::Global(_)
                if import.module == ENV_MODULE
                    && matches!(import.name.as_str(), "__memory_base" | "__table_base") => {}
            ImportKind::Global(_) => {
                bail!(
                    "imported global `{}.{}` is not supported",
                    import.module,
                    import.name
                )
            }
            ImportKind::Tag(_) => bail!("imported tag `{}` is not supported", import.name),
        }
    }

    // Wasm globals become QuakeC float globals. Exported ones are data
    // symbols (like `__quakec_target`), which code doesn't use.
    for g in module.globals.iter() {
        let exported = module
            .exports
            .iter()
            .any(|e| matches!(e.item, ExportItem::Global(x) if x == g.id()));
        if let GlobalKind::Local(_) = g.kind
            && !exported
        {
            let name = format!("__wasm_g{}", ctx.wasm_globals.len());
            let ty = ctx.val_ty(g.ty).context("in wasm global")?;
            ctx.wasm_globals.insert(g.id(), (name, ty));
        }
    }

    // Local function names: exported name if any, else a generated one.
    let mut export_names: HashMap<FunctionId, String> = HashMap::new();
    for e in module.exports.iter() {
        if let ExportItem::Function(f) = e.item {
            export_names.entry(f).or_insert_with(|| e.name.clone());
        }
    }
    let mut local_ids = Vec::new();
    for (i, f) in module.funcs.iter().enumerate() {
        if let FunctionKind::Local(_) = &f.kind {
            // Internal functions are prefixed so they can't collide with
            // QuakeC globals, fields or builtins.
            let name = match (export_names.get(&f.id()), &f.name) {
                (Some(export), _) => export.clone(),
                (None, Some(debug_name)) => format!("__wasm_{}", sanitize(debug_name)),
                (None, None) => format!("__wasm_f{i}"),
            };
            let sig = ctx
                .sig_of_type(f.ty())
                .with_context(|| format!("in signature of function `{name}`"))?;
            ctx.funcs.insert(
                f.id(),
                FuncInfo {
                    name,
                    sig,
                    kind: FuncKind::Local,
                },
            );
            local_ids.push(f.id());
        }
    }

    // Lower all functions.
    let mut functions = Vec::new();
    for &id in &local_ids {
        let FunctionKind::Local(lf) = &module.funcs.get(id).kind else {
            unreachable!()
        };
        let name = ctx.funcs[&id].name.clone();
        let f = lower::lower_function(&mut ctx, id, lf)
            .with_context(|| format!("while translating function `{name}`"))?;
        functions.push(f);
    }

    let mut tv = std::mem::take(&mut ctx.tv);
    let mut warnings = std::mem::take(&mut ctx.warnings);
    warnings.extend(tv.conflicts.iter().cloned());

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
        let mut gs: Vec<_> = ctx.wasm_globals.values().cloned().collect();
        gs.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, ty) in gs {
            let _ = writeln!(out, "{} {name};", tv.resolve(ty).name());
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
            let sig = f.sig.render(&mut tv);
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

    if !ctx.data_accessors.is_empty() {
        out.push_str("// ---- static data accessors ----\n");
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

/// Find the system defs named by the exported `__quakec_target` string.
fn target(module: &Module, data: &mut StaticData) -> Result<&'static SysDefs> {
    let Some(export) = module.exports.iter().find(|e| e.name == TARGET_EXPORT) else {
        return Ok(&sysdefs::VANILLA);
    };
    let ExportItem::Global(g) = export.item else {
        bail!("`{TARGET_EXPORT}` must be exported data")
    };
    let GlobalKind::Local(ConstExpr::Value(Value::I32(offset))) = module.globals.get(g).kind else {
        bail!("`{TARGET_EXPORT}` must be exported data");
    };
    let name = data
        .c_string(DATA_BASE + offset as u64)
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

/// Check an imported builtin's wasm signature against its vanilla QuakeC
/// signature, fixing the types of its externref parameters and result.
fn check_builtin_sig(tv: &mut TypeVars, b: &sysdefs::Builtin, sig: &Sig) -> Result<()> {
    if sig.params.len() != b.params.len() {
        bail!(
            "expected {} parameters, but the import has {}",
            b.params.len(),
            sig.params.len()
        );
    }
    let slots = sig.params.iter().zip(b.params).map(|(&w, &q)| (w, q));
    for (i, (wasm, qc)) in slots.chain(std::iter::once((sig.ret, b.ret))).enumerate() {
        let what = if i < b.params.len() {
            format!("parameter {i}")
        } else {
            "return type".into()
        };
        let ok = match (wasm, qc) {
            (Ty::Known(w), q) => w == q,
            // An externref: entity, string or field.
            (Ty::Var(_), QcType::Entity | QcType::String | QcType::StringField) => {
                tv.unify(wasm, Ty::Known(qc), &format!("builtin `{}` {what}", b.name));
                true
            }
            (Ty::Var(_), _) => false,
        };
        if !ok {
            let found = match wasm {
                Ty::Known(w) => w.name().to_string(),
                Ty::Var(_) => "externref".into(),
            };
            bail!("{what} should be {}, but the import has {found}", qc.name());
        }
    }
    Ok(())
}
