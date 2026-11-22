//! Builds `qc-test` with `zig-qc` and runs its functions with `run-qc`,
//! checking their results. Skipped if `zig`, `wasm-opt`, `run-qc` or fteqcc
//! isn't available.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

/// Build `qc-test` (once per test run), returning its `progs.dat`, or `None`
/// if the tools aren't available.
fn progs() -> Option<&'static Path> {
    static PROGS: OnceLock<Option<PathBuf>> = OnceLock::new();
    PROGS
        .get_or_init(|| {
            let fteqcc = std::env::var_os("FTEQCC").map(PathBuf::from).unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../fteqcc-mac/fteqcc")
            });
            if !on_path("zig") || !on_path("wasm-opt") || !on_path("run-qc") || !fteqcc.is_file()
            {
                eprintln!("skipping: zig, wasm-opt, run-qc or fteqcc not found");
                return None;
            }
            // `zig-qc` runs fteqcc as `fteqcc`: put a link to it first on the
            // PATH.
            let bin = std::env::temp_dir().join(format!("run-qc-test-{}", std::process::id()));
            std::fs::create_dir_all(&bin).unwrap();
            let link = bin.join("fteqcc");
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(std::fs::canonicalize(&fteqcc).unwrap(), &link).unwrap();
            let mut paths = vec![bin.clone()];
            paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));

            let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("qc-test");
            let out = Command::new(env!("CARGO_BIN_EXE_zig-qc"))
                .args(["build", "-O1"])
                .current_dir(&project)
                .env("PATH", std::env::join_paths(paths).unwrap())
                .output()
                .unwrap();
            std::fs::remove_dir_all(&bin).ok();
            assert!(
                out.status.success(),
                "building qc-test failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            Some(project.join("zig-out/lib/progs.dat"))
        })
        .as_deref()
}

/// Run `func(args)` with `globals` set (a JSON object, or `None`), and
/// return what `run-qc` prints.
fn run(func: &str, args: &[f32], globals: Option<&str>) -> Option<String> {
    let progs = progs()?;
    let mut cmd = Command::new("run-qc");
    if let Some(g) = globals {
        cmd.args(["-g", g]);
    }
    cmd.arg(progs).arg(func);
    // `--` so that negative numbers aren't taken as options.
    cmd.arg("--");
    cmd.args(args.iter().map(f32::to_string));
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "run-qc {func} {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8(out.stdout).unwrap().trim().to_string())
}

/// Check that `func(args)` returns `expected`.
fn check(func: &str, args: &[f32], globals: Option<&str>, expected: f32) {
    let Some(out) = run(func, args, globals) else {
        return;
    };
    let got: f32 = out
        .parse()
        .unwrap_or_else(|_| panic!("unexpected output from `{func}`: {out:?}"));
    assert_eq!(got, expected, "{func}({args:?}) with globals {globals:?}");
}

#[test]
fn arithmetic() {
    check("add", &[2.0, 3.5], None, 5.5);
    check("add", &[-1.0, 1.0], None, 0.0);
}

#[test]
fn loops() {
    check("factorial", &[0.0], None, 1.0);
    check("factorial", &[5.0], None, 120.0);
    check("factorial", &[10.0], None, 3628800.0);
}

#[test]
fn recursion() {
    check("fib", &[0.0], None, 0.0);
    check("fib", &[1.0], None, 1.0);
    check("fib", &[10.0], None, 55.0);
}

#[test]
fn switch_tables() {
    for (m, days) in [(1, 31), (2, 28), (4, 30), (7, 31), (11, 30), (12, 31), (0, 0), (13, 0)] {
        check("days_in_month", &[m as f32], None, days as f32);
    }
}

#[test]
fn truncation_and_comparisons() {
    check("clamp_trunc", &[3.7, 0.0, 10.0], None, 3.0);
    check("clamp_trunc", &[-3.7, -10.0, 10.0], None, -3.0);
    check("clamp_trunc", &[-5.5, 0.0, 10.0], None, 0.0);
    check("clamp_trunc", &[12.0, 0.0, 10.0], None, 10.0);
}

#[test]
fn function_pointers() {
    check("apply", &[0.0, 4.0], None, 8.0);
    check("apply", &[1.0, 4.0], None, -4.0);
}

#[test]
fn vectors() {
    // (3, 5, 7) · (3, 5, 7)
    check("vector_math", &[1.0, 2.0, 3.0], None, 83.0);
}

#[test]
fn user_globals() {
    check("bump", &[1.0], Some(r#"{"counter": 41}"#), 42.0);
    check("bump", &[2.5], None, 2.5);
}

#[test]
fn strings() {
    check("name_code", &[], Some(r#"{"name": "player"}"#), 1.0);
    check("name_code", &[], Some(r#"{"name": "monster"}"#), 2.0);
    check("name_code", &[], Some(r#"{"name": "zombie"}"#), 3.0);
    check("name_code", &[], None, 0.0);
}
