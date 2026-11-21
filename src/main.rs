use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

/// Convert a WebAssembly module into QuakeC source.
#[derive(Parser)]
struct Args {
    /// Input `.wasm` file.
    input: PathBuf,
    /// Output `.qc` file (defaults to stdout).
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Don't emit the vanilla system defs prelude (provide your own defs.qc).
    #[arg(long)]
    no_prelude: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let wasm = std::fs::read(&args.input)
        .with_context(|| format!("reading {}", args.input.display()))?;
    let opts = wasm_to_qc::Options { prelude: !args.no_prelude };
    let out = wasm_to_qc::translate(&wasm, &opts)?;
    for w in &out.warnings {
        eprintln!("warning: {w}");
    }
    match args.output {
        Some(p) => std::fs::write(&p, out.source).with_context(|| format!("writing {}", p.display()))?,
        None => print!("{}", out.source),
    }
    Ok(())
}
