//! Read-only static data (wasm data segments).
//!
//! The module is position-independent: its data segments are placed at
//! `__memory_base`. We give `__memory_base` the fixed value [`DATA_BASE`], so
//! data addresses are ordinary numbers. Loads from constant addresses are
//! folded to constants, and loads from dynamic addresses (e.g. the lookup
//! tables LLVM makes from `switch`es) call a generated accessor function that
//! binary-searches the data.

use std::ops::Range;

use anyhow::{Result, bail};
use walrus::ir::{ExtendedLoad, LoadKind};
use walrus::{ConstExpr, ConstOp, GlobalId, Module};

use crate::qc::{BinOp, Expr, Function, QcType, Stmt, Ty};

/// Where static data starts. Far above every QuakeC global slot, and small
/// enough that all data addresses are exact as f32 (< 2^24).
pub const DATA_BASE: u64 = 1 << 20;

/// How a load reads static data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DataKind {
    F32,
    I32,
    U8,
    S8,
    U16,
    S16,
}

impl DataKind {
    pub fn from_load(kind: LoadKind) -> Option<Self> {
        let signed = |k: ExtendedLoad| matches!(k, ExtendedLoad::SignExtend);
        Some(match kind {
            LoadKind::F32 => Self::F32,
            LoadKind::I32 { atomic: false } => Self::I32,
            LoadKind::I32_8 { kind } => if signed(kind) { Self::S8 } else { Self::U8 },
            LoadKind::I32_16 { kind } => if signed(kind) { Self::S16 } else { Self::U16 },
            _ => return None,
        })
    }

    pub fn size(self) -> u64 {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::U16 | Self::S16 => 2,
            Self::U8 | Self::S8 => 1,
        }
    }

    /// Name of the generated accessor function.
    pub fn accessor_name(self) -> &'static str {
        match self {
            Self::F32 => "__wasm_data_f32",
            Self::I32 => "__wasm_data_i32",
            Self::U8 => "__wasm_data_u8",
            Self::S8 => "__wasm_data_s8",
            Self::U16 => "__wasm_data_u16",
            Self::S16 => "__wasm_data_s16",
        }
    }

    fn decode(self, b: &[u8]) -> f32 {
        match self {
            Self::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Self::I32 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
            Self::U16 => u16::from_le_bytes([b[0], b[1]]) as f32,
            Self::S16 => i16::from_le_bytes([b[0], b[1]]) as f32,
            Self::U8 => b[0] as f32,
            Self::S8 => b[0] as i8 as f32,
        }
    }
}

/// The contents of static memory, starting at [`DATA_BASE`].
#[derive(Default)]
pub struct StaticData {
    bytes: Vec<u8>,
    /// Absolute address ranges read as string literals. Accessors leave these
    /// out: string bytes are only read whole, by `__quakec_string_literal`.
    strings: Vec<Range<u64>>,
}

impl StaticData {
    /// Collect the module's data segments. They must all be active segments
    /// placed relative to `memory_base`.
    pub fn from_module(module: &Module, memory_base: Option<GlobalId>) -> Result<Self> {
        let mut bytes = Vec::new();
        for seg in module.data.iter() {
            let walrus::DataKind::Active { offset, .. } = &seg.kind else {
                bail!("passive data segments are not supported");
            };
            let Some(offset) = relative_offset(offset, memory_base) else {
                bail!("data segment at {offset:?} is not relative to `__memory_base`");
            };
            let end = offset + seg.value.len();
            if bytes.len() < end {
                bytes.resize(end, 0);
            }
            bytes[offset..end].copy_from_slice(&seg.value);
        }
        if DATA_BASE + bytes.len() as u64 >= 1 << 24 {
            bail!("too much static data ({} bytes)", bytes.len());
        }
        Ok(Self { bytes, strings: Vec::new() })
    }

    /// Whether `addr` falls in the static data region (or past it, which is
    /// still not a QuakeC global).
    pub fn is_data_addr(addr: u64) -> bool {
        addr >= DATA_BASE
    }

    /// Read a value at an absolute address.
    pub fn read(&self, addr: u64, kind: DataKind) -> Option<f32> {
        let start = usize::try_from(addr.checked_sub(DATA_BASE)?).ok()?;
        let b = self.bytes.get(start..start + kind.size() as usize)?;
        Some(kind.decode(b))
    }

    /// Read the `len` bytes of a string literal at an absolute address, and
    /// remember that they are string data.
    pub fn string(&mut self, addr: u64, len: u64) -> Option<Vec<u8>> {
        let start = usize::try_from(addr.checked_sub(DATA_BASE)?).ok()?;
        let bytes = self.bytes.get(start..start.checked_add(usize::try_from(len).ok()?)?)?.to_vec();
        self.strings.push(addr..addr + len);
        Some(bytes)
    }

    /// Generate `float __wasm_data_<kind>(float addr)`: a binary search over
    /// every aligned address outside of string literals, returning 0 for
    /// anything else.
    pub fn accessor(&self, kind: DataKind) -> Function {
        let in_string = |a: u64| self.strings.iter().any(|r| a < r.end && r.start < a + kind.size());
        let entries: Vec<(f32, f32)> = (0..self.bytes.len() as u64 / kind.size())
            .map(|i| DATA_BASE + i * kind.size())
            .filter(|&a| !in_string(a))
            .map(|a| (a as f32, self.read(a, kind).unwrap()))
            .collect();
        let mut body = search(&entries);
        body.push(Stmt::Return(Some(Expr::Float(0.0))));
        Function {
            name: kind.accessor_name().to_string(),
            params: vec![("addr".into(), Ty::Known(QcType::Float))],
            ret: Ty::Known(QcType::Float),
            locals: vec![],
            body,
        }
    }
}

/// Binary search over `(address, value)` pairs, sorted by address. Falls
/// through (to the caller's `return 0`) if no address matches. Zero values are
/// skipped since they are the default.
fn search(entries: &[(f32, f32)]) -> Vec<Stmt> {
    let addr = || Box::new(Expr::Local("addr".into()));
    if entries.len() <= 4 {
        return entries
            .iter()
            .filter(|(_, v)| *v != 0.0)
            .map(|&(a, v)| {
                let cond = Expr::Bin(BinOp::Eq, addr(), Box::new(Expr::Float(a)));
                Stmt::If(cond, vec![Stmt::Return(Some(Expr::Float(v)))], vec![])
            })
            .collect();
    }
    if entries.iter().all(|(_, v)| *v == 0.0) {
        return vec![];
    }
    let (lo, hi) = entries.split_at(entries.len() / 2);
    let mid = Box::new(Expr::Float(hi[0].0));
    match (search(lo), search(hi)) {
        (lo, hi) if hi.is_empty() => vec![Stmt::If(Expr::Bin(BinOp::Lt, addr(), mid), lo, vec![])],
        (lo, hi) if lo.is_empty() => vec![Stmt::If(Expr::Bin(BinOp::Ge, addr(), mid), hi, vec![])],
        (lo, hi) => vec![Stmt::If(Expr::Bin(BinOp::Lt, addr(), mid), lo, hi)],
    }
}

/// The offset of a data segment relative to `__memory_base`.
fn relative_offset(offset: &ConstExpr, memory_base: Option<GlobalId>) -> Option<usize> {
    let base = memory_base?;
    match offset {
        ConstExpr::Global(g) if *g == base => Some(0),
        ConstExpr::Extended(ops) => match ops.as_slice() {
            [ConstOp::GlobalGet(g), ConstOp::I32Const(c), ConstOp::I32Add]
            | [ConstOp::I32Const(c), ConstOp::GlobalGet(g), ConstOp::I32Add]
                if *g == base =>
            {
                usize::try_from(*c).ok()
            }
            _ => None,
        },
        _ => None,
    }
}
