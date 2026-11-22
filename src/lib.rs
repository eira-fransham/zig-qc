//! Translate WebAssembly modules produced from Zig (in a specific, known
//! form) into QuakeC source code.

pub mod data;
pub mod emit;
pub mod infer;
pub mod prepare;
pub mod qc;
pub mod sysdefs;
pub mod translate;

pub use translate::{Options, Output, translate};
