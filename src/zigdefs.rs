//! Generate a target's Zig `defs.zig` (the system globals, fields and
//! builtins, plus the generic glue for the converter's intrinsics) from its
//! [`SysDefs`], so that the Zig side always matches what the converter
//! expects. `zig-qc init --target <name>` writes this.

use std::fmt::Write;

use crate::qc::QcType;
use crate::sysdefs::{SYS_GLOBALS_START, SysDefs, SysTy};

const TEMPLATE: &str = include_str!("zigdefs/defs.zig.in");

/// Per-target text that isn't part of [`SysDefs`].
struct Doc {
    /// Engine name, as used in prose (`QuakeWorld`).
    title: &'static str,
    /// The `defs.qc` the names come from.
    defs_file: &'static str,
    /// Appended to the first doc line, describing what else the file holds.
    header_extra: &'static str,
    /// Game definitions after the builtins.
    tail: &'static str,
}

fn doc(defs: &SysDefs) -> Doc {
    match defs.name {
        "quakeworld" => Doc {
            title: "QuakeWorld",
            defs_file: "qw-qc/defs.qc",
            header_extra: ", plus the game definitions from that file",
            tail: include_str!("zigdefs/tail_quakeworld.zig"),
        },
        "vanilla" => Doc {
            title: "NetQuake",
            defs_file: "defs.qc",
            header_extra: "",
            tail: include_str!("zigdefs/tail_vanilla.zig"),
        },
        other => unreachable!("no defs.zig documentation for target `{other}`"),
    }
}

/// Zig keywords, which need `@"..."` to be used as identifiers.
pub const ZIG_KEYWORDS: &[&str] = &[
    "addrspace",
    "align",
    "allowzero",
    "and",
    "anyframe",
    "anytype",
    "asm",
    "break",
    "callconv",
    "catch",
    "comptime",
    "const",
    "continue",
    "defer",
    "else",
    "enum",
    "errdefer",
    "error",
    "export",
    "extern",
    "fn",
    "for",
    "if",
    "inline",
    "linksection",
    "noalias",
    "noinline",
    "nosuspend",
    "opaque",
    "or",
    "orelse",
    "packed",
    "pub",
    "resume",
    "return",
    "struct",
    "suspend",
    "switch",
    "test",
    "threadlocal",
    "try",
    "union",
    "unreachable",
    "var",
    "volatile",
    "while",
];

/// `name` as a Zig identifier.
fn ident(name: &str) -> String {
    if ZIG_KEYWORDS.contains(&name) {
        format!("@\"{name}\"")
    } else {
        name.to_string()
    }
}

fn zig_type(t: QcType) -> &'static str {
    match t {
        QcType::Void => "void",
        QcType::Float => "f32",
        QcType::Vector => "Vector",
        QcType::String => "String",
        QcType::Entity => "Entity",
        QcType::Func => "Function",
        QcType::StringField => "Field",
        other => unreachable!("no Zig type for builtin parameter type {other:?}"),
    }
}

/// Field declarations as `(type, name)`, with the type as the Zig type of
/// `Field.Type()`.
fn field_zig_type(t: &str) -> &'static str {
    match t {
        "float" => "f32",
        "vector" => "Vector",
        "string" => "String",
        "entity" => "Entity",
        // Function fields can hold the null function (`__NULL__`).
        "void()" => "?Function",
        other => unreachable!("unknown field type {other}"),
    }
}

/// Generate `defs.zig` for `defs`.
pub fn generate(defs: &SysDefs) -> String {
    let doc = doc(defs);

    let mut field_enum = String::new();
    let mut entity_fields = String::new();
    let mut offset = 0u32;
    for &(ty, name) in defs.fields {
        let _ = writeln!(field_enum, "    {} = {offset},", ident(name));
        let ety = if ty == "vector" {
            "[3]f32"
        } else {
            field_zig_type(ty)
        };
        let _ = writeln!(entity_fields, "    {}: {ety},", ident(name));
        offset += if ty == "vector" { 3 } else { 1 };
    }

    let mut field_types = String::new();
    for zt in ["f32", "Vector", "String", "?Function", "Entity"] {
        let names: Vec<String> = defs
            .fields
            .iter()
            .filter(|&&(t, _)| field_zig_type(t) == zt)
            .map(|&(_, n)| format!(".{}", ident(n)))
            .collect();
        if !names.is_empty() {
            let _ = writeln!(field_types, "            {} => {zt},", names.join(", "));
        }
    }

    let mut globals = String::new();
    let mut slot = SYS_GLOBALS_START;
    for &(name, ty) in defs.globals {
        let zt = match ty {
            SysTy::Float => "f32",
            SysTy::Entity => "Entity",
            SysTy::String => "String",
            SysTy::Vector => "Vector",
            SysTy::Func => "fn () callconv(.c) void",
        };
        let _ = writeln!(globals, "pub const {} = global({zt}, {slot});", ident(name));
        if ty == SysTy::Vector {
            for (i, c) in ["x", "y", "z"].iter().enumerate() {
                let _ = writeln!(
                    globals,
                    "pub const {name}_{c} = global(f32, {});",
                    slot + i as u32
                );
            }
            slot += 3;
        } else {
            slot += 1;
        }
    }

    let mut builtins = String::new();
    for b in defs.builtins {
        let params: Vec<String> = b
            .param_names
            .iter()
            .zip(b.params)
            .map(|(&n, &t)| format!("{}: {}", ident(n), zig_type(t)))
            .collect();
        let decl = format!(
            "extern \"builtin\" fn {}({}) {}; // #{}",
            ident(b.name),
            params.join(", "),
            zig_type(b.ret),
            b.number
        );
        if b.name == "find" {
            // Wrapped by the `find` in the template, which takes a `Field`.
            let _ = writeln!(
                builtins,
                "// Not `pub` so we can write an adapter function, above.\n\
                 const find_mod = struct {{\n    {decl}\n}};"
            );
        } else {
            let _ = writeln!(builtins, "pub {decl}");
        }
    }

    TEMPLATE
        .replace("@HEADER_EXTRA@", doc.header_extra)
        .replace("@TITLE@", doc.title)
        .replace("@DEFS_FILE@", doc.defs_file)
        .replace("@TARGET_LEN@", &defs.name.len().to_string())
        .replace("@TARGET@", defs.name)
        .replace("@FIELD_ENUM@\n", &field_enum)
        .replace("@FIELD_TYPES@\n", &field_types)
        .replace("@ENTITY_FIELDS@\n", &entity_fields)
        .replace("@GLOBALS@\n", &globals)
        .replace("@BUILTINS@\n", &builtins)
        .replace("@TAIL@", doc.tail)
}
