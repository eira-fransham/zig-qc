use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn translate_bytes(wasm: &[u8]) -> String {
    let opts = wasm_to_qc::Options { prelude: false };
    let out = wasm_to_qc::translate(wasm, &opts).expect("translation failed");
    assert!(out.warnings.is_empty(), "warnings: {:?}", out.warnings);
    out.source
}

fn translate_wat(name: &str) -> String {
    let wasm = wat::parse_file(fixture(name)).expect("invalid wat");
    translate_bytes(&wasm)
}

/// Compile the full output (with prelude) using fteqcc, if available.
/// Set `FTEQCC` to the compiler path; defaults to `../fteqcc-mac/fteqcc`.
fn check_compiles(wasm: &[u8], extra: &str) {
    let fteqcc = std::env::var_os("FTEQCC")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../fteqcc-mac/fteqcc"));
    if !fteqcc.exists() {
        eprintln!("skipping fteqcc check: {} not found", fteqcc.display());
        return;
    }
    let src = wasm_to_qc::translate(wasm, &wasm_to_qc::Options::default()).unwrap().source;
    let dir = std::env::temp_dir().join(format!("wasm-to-qc-test-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("out.qc"), src).unwrap();
    std::fs::write(dir.join("extra.qc"), extra).unwrap();
    std::fs::write(dir.join("progs.src"), "progs.dat\nout.qc\nextra.qc\n").unwrap();
    let out = Command::new(&fteqcc).arg("-Tid").current_dir(&dir).output().unwrap();
    let log = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success() && dir.join("progs.dat").exists(), "fteqcc failed:\n{log}");
    assert!(log.contains("0 warnings"), "fteqcc warnings:\n{log}");
    std::fs::remove_dir_all(&dir).ok();
}

fn rand_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

#[test]
fn zig_test() {
    let wasm = std::fs::read(fixture("zig_test.wasm")).unwrap();
    insta::assert_snapshot!(translate_bytes(&wasm));
    check_compiles(&wasm, "");
}

#[test]
fn control_flow() {
    insta::assert_snapshot!(translate_wat("control_flow.wat"));
    check_compiles(&wat::parse_file(fixture("control_flow.wat")).unwrap(), "");
}

#[test]
fn intrinsics() {
    insta::assert_snapshot!(translate_wat("intrinsics.wat"));
    check_compiles(&wat::parse_file(fixture("intrinsics.wat")).unwrap(), "");
}

#[test]
fn builtins() {
    insta::assert_snapshot!(translate_wat("builtins.wat"));
    // Unknown `builtin` imports must be defined by the defs.
    check_compiles(
        &wat::parse_file(fixture("builtins.wat")).unwrap(),
        "float(float x) my_engine_builtin = #500;\n",
    );
}

#[test]
fn builtin_signature_mismatch_is_an_error() {
    let err = translate_err(r#"(module (import "builtin" "floor" (func (param externref) (result f32))))"#);
    assert!(err.contains("parameter 0 should be float, but the import has externref"), "{err}");
    let err = translate_err(r#"(module (import "builtin" "spawn" (func (param f32) (result externref))))"#);
    assert!(err.contains("expected 0 parameters, but the import has 1"), "{err}");
}

#[test]
fn zig_fields() {
    // Built from `zig_fields.zig` against `zig-src/src/defs.zig`.
    let wasm = std::fs::read(fixture("zig_fields.wasm")).unwrap();
    insta::assert_snapshot!(translate_bytes(&wasm));
    check_compiles(&wasm, "");
}

#[test]
fn fields() {
    insta::assert_snapshot!(translate_wat("fields.wat"));
    check_compiles(&wat::parse_file(fixture("fields.wat")).unwrap(), "");
}

#[test]
fn field_errors() {
    let load = r#"(import "env" "__quakec_load_entity_field_float" (func (param externref i32) (result f32)))"#;
    // A float access to a string field.
    let err = translate_err(&format!(
        r#"(module {load} (func (export "f") (param externref) (result f32) (call 0 (local.get 0) (i32.const 28))))"#
    ));
    assert!(err.contains("entity field offset 28 is not a known float field"), "{err}");
    // Past the end of the system fields.
    let err = translate_err(&format!(
        r#"(module {load} (func (export "f") (param externref) (result f32) (call 0 (local.get 0) (i32.const 105))))"#
    ));
    assert!(err.contains("entity field offset 105 is not a known float field"), "{err}");
    // A non-constant offset.
    let err = translate_err(&format!(
        r#"(module {load} (func (export "f") (param externref i32) (result f32) (call 0 (local.get 0) (local.get 1))))"#
    ));
    assert!(err.contains("entity field offset must be a compile-time constant"), "{err}");
}

fn translate_err(wat: &str) -> String {
    let wasm = wat::parse_str(wat).unwrap();
    match wasm_to_qc::translate(&wasm, &wasm_to_qc::Options::default()) {
        Ok(_) => panic!("expected translation to fail"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn unknown_env_import_is_an_error() {
    let err = translate_err(r#"(module (import "env" "doSomething" (func (param f32))))"#);
    assert!(err.contains("unknown import `env.doSomething`"), "{err}");
}

#[test]
fn unknown_import_module_is_an_error() {
    let err = translate_err(r#"(module (import "other" "foo" (func)))"#);
    assert!(err.contains("unsupported import module `other`"), "{err}");
}

#[test]
fn unknown_intrinsic_is_an_error() {
    let err = translate_err(r#"(module (import "env" "__quakec_bogus" (func)))"#);
    assert!(err.contains("unknown QuakeC intrinsic `__quakec_bogus`"), "{err}");
}
