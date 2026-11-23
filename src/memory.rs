//! The memory accesses of wasm loads and stores, as 4-byte (or narrower)
//! cells: QuakeC memory (globals, entity fields, the shadow stack) is all
//! 4-byte slots, and vector accesses are lane by lane.

use waffle::Operator as O;

/// What a 4-byte (or other) slot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    I32,
    F32,
    /// A vector lane: an `i32` or `f32`, whichever the other accesses say.
    Lane,
    /// The low 8 or 16 bits of an `i32` (sign-extended by `load8_s` and
    /// `load16_s`).
    Narrow(u8),
}

impl Cell {
    pub fn width(self) -> i64 {
        match self {
            Cell::I32 | Cell::F32 | Cell::Lane => 4,
            Cell::Narrow(w) => w as i64 / 8,
        }
    }

    pub fn name(self) -> String {
        match self {
            Cell::I32 => "i32".into(),
            Cell::F32 => "f32".into(),
            Cell::Lane => "a vector lane".into(),
            Cell::Narrow(w) => format!("an {w}-bit integer"),
        }
    }
}

/// One cell a memory access touches: its offset from the address, what it
/// holds, and its vector lane (for vector accesses).
pub struct CellAccess {
    pub rel: i64,
    pub cell: Cell,
    pub lane: Option<u8>,
}

/// The cells a load or store accesses, `Ok(None)` if `op` isn't one, or why
/// it isn't supported.
pub fn cells(op: &O) -> Result<Option<(bool, u32, Vec<CellAccess>)>, String> {
    let one = |cell| {
        vec![CellAccess {
            rel: 0,
            cell,
            lane: None,
        }]
    };
    let lanes = |ls: &[u8]| {
        ls.iter()
            .enumerate()
            .map(|(i, &l)| CellAccess {
                rel: 4 * i as i64,
                cell: Cell::Lane,
                lane: Some(l),
            })
            .collect()
    };
    Ok(Some(match op {
        O::I32Load { memory } => (false, memory.offset, one(Cell::I32)),
        O::F32Load { memory } => (false, memory.offset, one(Cell::F32)),
        O::I32Load8U { memory } | O::I32Load8S { memory } => {
            (false, memory.offset, one(Cell::Narrow(8)))
        }
        O::I32Load16U { memory } | O::I32Load16S { memory } => {
            (false, memory.offset, one(Cell::Narrow(16)))
        }
        O::I32Store { memory } => (true, memory.offset, one(Cell::I32)),
        O::F32Store { memory } => (true, memory.offset, one(Cell::F32)),
        O::I32Store8 { memory } => (true, memory.offset, one(Cell::Narrow(8))),
        O::I32Store16 { memory } => (true, memory.offset, one(Cell::Narrow(16))),
        O::V128Load { memory } => (false, memory.offset, lanes(&[0, 1, 2, 3])),
        O::V128Store { memory } => (true, memory.offset, lanes(&[0, 1, 2, 3])),
        O::V128Load32Zero { memory } => (false, memory.offset, lanes(&[0])),
        O::V128Load64Zero { memory } => (false, memory.offset, lanes(&[0, 1])),
        O::V128Load32Lane { memory, lane } => (false, memory.offset, lanes(&[*lane])),
        O::V128Load64Lane { memory, lane } => {
            (false, memory.offset, lanes(&[2 * lane, 2 * lane + 1]))
        }
        O::V128Store32Lane { memory, lane } => (true, memory.offset, lanes(&[*lane])),
        O::V128Store64Lane { memory, lane } => {
            (true, memory.offset, lanes(&[2 * lane, 2 * lane + 1]))
        }
        op if is_memory(op) => {
            return Err(format!("`{op}` is not supported (only 32-bit and vector accesses are)"));
        }
        _ => return Ok(None),
    }))
}

/// Whether `op` accesses memory (any load, store or bulk memory op).
pub fn is_memory(op: &O) -> bool {
    let s = format!("{op:?}");
    s.contains("Load") || s.contains("Store") || s.starts_with("Memory")
}

