//! Runs the tests in `zig-src/src/defs.zig` and `defs-vanilla.zig` (e.g. that the entity struct's
//! layout matches `Field`) on wasm32, under wasmtime. Skipped if `zig` or
//! `wasmtime` isn't available.

use std::path::Path;
use std::process::Command;

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

#[test]
fn defs_zig_tests() {
    if !on_path("zig") || !on_path("wasmtime") {
        eprintln!("skipping: zig or wasmtime not found");
        return;
    }
    for file in ["src/defs.zig", "src/defs-vanilla.zig"] {
        let out = Command::new("zig")
            .args(["test", file, "-target", "wasm32-wasi"])
            .args(["--test-cmd", "wasmtime", "--test-cmd-bin"])
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("zig-src"))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "zig test {file} failed:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// The checked-in `defs.zig` files are exactly what `zig-qc init` generates.
/// Run with `UPDATE_ZIG_DEFS=1` to regenerate them.
#[test]
fn generated_defs_are_up_to_date() {
    use wasm2qc::sysdefs;
    use zig_qc::zigdefs;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        (&sysdefs::QUAKEWORLD, "zig-src/src/defs.zig"),
        (&sysdefs::QUAKEWORLD, "qc-test/src/defs.zig"),
        (&sysdefs::QUAKEWORLD, "qw-zig/src/defs.zig"),
        (&sysdefs::VANILLA, "zig-src/src/defs-vanilla.zig"),
    ];
    let update = std::env::var_os("UPDATE_ZIG_DEFS").is_some();
    let mut stale = Vec::new();
    for (defs, path) in files {
        let path = root.join(path);
        let generated = zigdefs::generate(defs);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(&*generated) {
            if update {
                std::fs::write(&path, generated).unwrap();
            } else {
                stale.push(path.display().to_string());
            }
        }
    }
    assert!(
        stale.is_empty(),
        "out of date (rerun with UPDATE_ZIG_DEFS=1): {stale:?}"
    );
}
