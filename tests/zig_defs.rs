//! Runs the tests in `zig-src/src/defs.zig` (e.g. that the entity struct's
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
    let out = Command::new("zig")
        .args(["test", "src/defs.zig", "-target", "wasm32-wasi"])
        .args(["--test-cmd", "wasmtime", "--test-cmd-bin"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("zig-src"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "zig test failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
