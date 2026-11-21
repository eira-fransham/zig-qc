//! Module-level translation: classify imports/exports, lower each function,
//! then resolve types and render QuakeC source.

use std::collections::HashMap;
use std::fmt::Write as _;

use anyhow::{Context as _, Result, bail};
use walrus::{
    ExportItem, FunctionId, FunctionKind, GlobalId, GlobalKind, ImportKind, Module, TableId,
    ValType,
};

use crate::lower;
use crate::qc::{QcType, Sig, Ty, TypeVars};
use crate::sysdefs::{self, GlobalMap};

/// The name of the table LLVM uses to call `funcref`s.
pub const CALL_TABLE: &str = "__funcref_call_table";
/// Import module for known builtins. These are emitted as plain prototypes;
/// the defs (e.g. `defs.qc`) supply the actual `= #N` definitions.
pub const BUILTIN_MODULE: &str = "builtin";
/// Import module for builtins without a number. These are emitted as
/// `= #0`, which the engine resolves by name.
pub const UNNAMED_BUILTIN_MODULE: &str = "unnamed-builtin";
/// Import module for memory, the call table and `__quakec_*` intrinsics.
pub const ENV_MODULE: &str = "env";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intrinsic {
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
}

impl Intrinsic {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
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
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuncKind {
    /// Defined in this module.
    Local,
    /// Imported from the `builtin` module; emitted as a prototype.
    Builtin,
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
    pub globals: GlobalMap,
    pub funcs: HashMap<FunctionId, FuncInfo>,
    pub wasm_globals: HashMap<GlobalId, (String, Ty)>,
    pub call_table: Option<TableId>,
    pub tv: TypeVars,
    pub warnings: Vec<String>,
}

impl Ctx<'_> {
    /// Convert a wasm value type into a (possibly inferred) QuakeC type.
    pub fn val_ty(&mut self, t: ValType) -> Result<Ty> {
        Ok(match t {
            ValType::I32 | ValType::F32 => Ty::Known(QcType::Float),
            ValType::Ref(r) if r == walrus::RefType::EXTERNREF => self.tv.fresh(),
            ValType::Ref(r) if r == walrus::RefType::FUNCREF => Ty::Known(QcType::Func),
            other => bail!("unsupported value type {other:?} (only i32, f32, externref and funcref are supported)"),
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
            bail!("QuakeC functions take at most 8 parameters, found {}", params.len());
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

    if module.data.iter().next().is_some() {
        bail!("data segments are not supported yet (static data in linear memory)");
    }

    let mut ctx = Ctx {
        module: &module,
        globals: GlobalMap::vanilla(),
        funcs: HashMap::new(),
        wasm_globals: HashMap::new(),
        call_table: None,
        tv: TypeVars::default(),
        warnings: Vec::new(),
    };

    // Tables: only the LLVM funcref call table is allowed.
    for table in module.tables.iter() {
        let is_call_table = module.exports.iter().any(|e| {
            e.name == CALL_TABLE && matches!(e.item, ExportItem::Table(t) if t == table.id())
        }) || table.name.as_deref() == Some(CALL_TABLE);
        if !is_call_table {
            bail!("unsupported table {:?}; only `{CALL_TABLE}` may be used", table.name);
        }
        ctx.call_table = Some(table.id());
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
                    BUILTIN_MODULE => FuncKind::Builtin,
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
                ctx.funcs.insert(f, FuncInfo { name: import.name.clone(), sig, kind });
            }
            ImportKind::Table(_) => {}
            ImportKind::Global(_) => {
                bail!("imported global `{}.{}` is not supported", import.module, import.name)
            }
            ImportKind::Tag(_) => bail!("imported tag `{}` is not supported", import.name),
        }
    }

    // Wasm globals become QuakeC float globals.
    for g in module.globals.iter() {
        if let GlobalKind::Local(_) = g.kind {
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
            ctx.funcs.insert(f.id(), FuncInfo { name, sig, kind: FuncKind::Local });
            local_ids.push(f.id());
        }
    }

    // Lower all functions.
    let mut functions = Vec::new();
    for &id in &local_ids {
        let FunctionKind::Local(lf) = &module.funcs.get(id).kind else { unreachable!() };
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

    let sys_func_names: Vec<&str> = sysdefs::SYS_GLOBALS
        .iter()
        .filter(|(_, t)| *t == sysdefs::SysTy::Func)
        .map(|(n, _)| *n)
        .collect();

    if opts.prelude {
        let protos: HashMap<String, String> = functions
            .iter()
            .filter(|f| sys_func_names.contains(&f.name.as_str()))
            .map(|f| (f.name.clone(), f.render_proto(&mut tv)))
            .collect();
        out.push_str(&sysdefs::prelude(|n| protos.get(n).cloned()));
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
        .filter(|f| matches!(f.kind, FuncKind::Builtin | FuncKind::UnnamedBuiltin))
        .collect();
    imports.sort_by(|a, b| a.name.cmp(&b.name));
    if !imports.is_empty() {
        out.push_str("// ---- imports ----\n");
        for f in imports {
            let sig = f.sig.render(&mut tv);
            match f.kind {
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

    Ok(Output { source: out, warnings })
}

/// Make a wasm debug name usable as a QuakeC identifier.
fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}
