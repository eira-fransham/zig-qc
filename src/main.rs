//! `zig-qc`: create and build Zig projects that compile to a QuakeC `progs.dat`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use wasm2qc::sysdefs;

use zig_qc::zigdefs;

const BUILD_ZIG: &str = include_str!("templates/build.zig.in");
const BUILD_ZIG_ZON: &str = include_str!("templates/build.zig.zon.in");
const ROOT_ZIG: &str = include_str!("templates/root.zig.in");

/// Name of the wasm file `zig-qc build` asks `zig build` for.
const BUILD_WASM: &str = "zig-qc-build.wasm";
const BUILD_QC: &str = "zig-qc-build.qc";
const OUT_DIR: &str = "zig-out/lib";

/// Build QuakeC progs from Zig.
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Set up a new project in the current directory.
    Init {
        /// Project name; defaults to the current directory's name. Must be a
        /// valid Zig identifier.
        name: Option<String>,
        /// Which engine's system defs (`defs.qc` layout) to target. The
        /// generated `src/defs.zig` tells the converter, via
        /// `__quakec_target`.
        #[arg(long, value_enum, default_value_t = Target::Vanilla)]
        target: Target,
    },
    /// Build the project in the current directory into `zig-out/lib/progs.dat`.
    #[clap(arg_required_else_help = true)]
    Build {
        /// Extra args to pass to `FTEQCC`. You almost certainly want to pass at least `-O1`, as the WebAssembly-to-QuakeC
        /// conversion step can create a lot of code.
        #[clap(trailing_var_arg = true, required = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// A supported engine, named as in `sysdefs`.
#[derive(Clone, Copy, ValueEnum)]
enum Target {
    /// NetQuake (`defs.qc`).
    Vanilla,
    /// QuakeWorld (`qw-qc/defs.qc`).
    Quakeworld,
}

impl Target {
    fn defs(self) -> &'static sysdefs::SysDefs {
        match self {
            Target::Vanilla => &sysdefs::VANILLA,
            Target::Quakeworld => &sysdefs::QUAKEWORLD,
        }
    }
}

fn main() -> Result<()> {
    match Args::parse().command {
        Cmd::Init { name, target } => init(name, target),
        Cmd::Build { args } => build(args),
    }
}

fn init(name: Option<String>, target: Target) -> Result<()> {
    let name = match name {
        Some(name) => {
            check_name(&name)?;
            name
        }
        None => {
            let cwd = std::env::current_dir().context("getting the current directory")?;
            let dir = cwd.file_name().and_then(|d| d.to_str()).with_context(|| {
                format!("can't make a project name from `{}`; pass one explicitly: `zig-qc init <name>`", cwd.display())
            })?;
            let name = sanitize_name(dir);
            check_name(&name).with_context(|| {
                format!("can't make a project name from directory `{dir}`; pass one explicitly: `zig-qc init <name>`")
            })?;
            println!("using project name `{name}` (from the current directory)");
            name
        }
    };
    let name = name.as_str();
    let files = [
        ("build.zig", BUILD_ZIG.replace("@NAME@", name)),
        (
            "build.zig.zon",
            BUILD_ZIG_ZON
                .replace("@NAME@", name)
                .replace("@FINGERPRINT@", &format!("{:#018x}", fingerprint(name))),
        ),
        ("src/defs.zig", zigdefs::generate(target.defs())),
        ("src/root.zig", ROOT_ZIG.replace("@NAME@", name)),
    ];
    for (path, _) in &files {
        ensure!(
            !Path::new(path).exists(),
            "`{path}` already exists; not overwriting it"
        );
    }
    std::fs::create_dir_all("src").context("creating src")?;
    for (path, contents) in &files {
        std::fs::write(path, contents).with_context(|| format!("writing {path}"))?;
        println!("created {path}");
    }
    println!("run `zig-qc build` to build zig-out/lib/progs.dat");
    Ok(())
}

/// The name becomes the package's `.name` enum literal in `build.zig.zon`,
/// so it must be a bare Zig identifier.
fn check_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    ensure!(
        valid,
        "invalid project name `{name}`: it must be a Zig identifier (letters, digits and `_`, not starting with a digit)"
    );
    ensure!(
        !zigdefs::ZIG_KEYWORDS.contains(&name),
        "invalid project name `{name}`: it is a Zig keyword"
    );
    ensure!(
        name.len() <= 32,
        "invalid project name `{name}`: it must be at most 32 characters"
    );
    Ok(())
}

/// Turn a directory name into a project name, like `zig init` does: `-`, `.`
/// and spaces become `_`, other non-identifier characters are dropped, and a
/// leading digit gets a `_` prefix. The result still goes through
/// [`check_name`].
fn sanitize_name(dir: &str) -> String {
    let mut name = String::new();
    for c in dir.chars() {
        match c {
            '0'..='9' if name.is_empty() => name.extend(['_', c]),
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => name.push(c),
            '-' | '.' | ' ' => name.push('_'),
            _ => {}
        }
    }
    name.truncate(32);
    name
}

/// A package fingerprint as Zig generates it: the CRC-32 of the name in the
/// high half and a random id (never 0 or `u32::MAX`) in the low half. Zig
/// 0.17 refuses to build without one.
fn fingerprint(name: &str) -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    let id = (h.finish() as u32).clamp(1, u32::MAX - 1);
    (u64::from(crc32(name.as_bytes())) << 32) | u64::from(id)
}

/// CRC-32 (IEEE), as used by `std.hash.Crc32`.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn build(fte_extra_args: Vec<String>) -> Result<()> {
    let tools = [
        ("zig", "install it from https://ziglang.org"),
        (
            "wasm-opt",
            "install Binaryen from https://github.com/WebAssembly/binaryen",
        ),
        ("fteqcc", "install it from https://www.fteqcc.org"),
    ];
    let missing: Vec<String> = tools
        .iter()
        .filter(|(tool, _)| find_in_path(tool).is_none())
        .map(|(tool, hint)| format!("`{tool}` was not found on the PATH; {hint}"))
        .collect();
    if !missing.is_empty() {
        bail!("missing tools:\n  {}", missing.join("\n  "));
    }
    ensure!(
        Path::new("build.zig").exists(),
        "no build.zig in the current directory; run `zig-qc init <name>` first"
    );

    let status = Command::new("zig")
        .args(["build", &format!("-Dout-name={BUILD_WASM}")])
        .status()
        .context("running zig build")?;
    ensure!(status.success(), "zig build failed");

    let out_dir = Path::new(OUT_DIR);
    let wasm_path = out_dir.join(BUILD_WASM);
    let wasm =
        std::fs::read(&wasm_path).with_context(|| format!("reading {}", wasm_path.display()))?;
    let out = wasm2qc::translate(&wasm, &wasm2qc::Options::default())
        .with_context(|| format!("converting {}", wasm_path.display()))?;
    for w in &out.warnings {
        eprintln!("warning: {w}");
    }
    let qc_path = out_dir.join(BUILD_QC);
    std::fs::write(&qc_path, out.source)
        .with_context(|| format!("writing {}", qc_path.display()))?;

    let src_path = out_dir.join("progs.src");
    std::fs::write(&src_path, format!("progs.dat\n{BUILD_QC}\n"))
        .with_context(|| format!("writing {}", src_path.display()))?;
    let progs = out_dir.join("progs.dat");
    if progs.exists() {
        std::fs::remove_file(&progs).with_context(|| format!("removing {}", progs.display()))?;
    }

    let status = Command::new("fteqcc")
        .arg("-Tid")
        .args(fte_extra_args)
        .current_dir(out_dir)
        .status()
        .context("running fteqcc")?;
    ensure!(status.success() && progs.exists(), "fteqcc failed");

    println!("wrote {}", progs.display());
    Ok(())
}

/// Find an executable on the PATH, without running it.
fn find_in_path(name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&file))
        .find(|p| p.is_file())
}
