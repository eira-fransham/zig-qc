use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
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
    let src = wasm_to_qc::translate(wasm, &wasm_to_qc::Options::default())
        .unwrap()
        .source;
    let dir = std::env::temp_dir().join(format!(
        "wasm-to-qc-test-{}-{}",
        std::process::id(),
        rand_suffix()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("out.qc"), src).unwrap();
    std::fs::write(dir.join("extra.qc"), extra).unwrap();
    std::fs::write(dir.join("progs.src"), "progs.dat\nout.qc\nextra.qc\n").unwrap();
    let out = Command::new(&fteqcc)
        .arg("-Tid")
        .current_dir(&dir)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && dir.join("progs.dat").exists(),
        "fteqcc failed:\n{log}"
    );
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
    let err =
        translate_err(r#"(module (import "builtin" "floor" (func (param i64) (result f32))))"#);
    assert!(
        err.contains("parameter 0 should be float, but the import has an i64 handle"),
        "{err}"
    );
    let err =
        translate_err(r#"(module (import "builtin" "spawn" (func (param f32) (result i64))))"#);
    assert!(
        err.contains("expected 0 parameters, but the import has 1"),
        "{err}"
    );
}

#[test]
fn zig_stress_test() {
    // `zig-src/zig-out/lib/stress-test.wasm`, built from `zig-src/src/stress-test.zig`.
    let wasm = std::fs::read(fixture("zig_stress_test.wasm")).unwrap();
    insta::assert_snapshot!(translate_bytes(&wasm));
    check_compiles(&wasm, "");
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
    let load =
        r#"(import "env" "__quakec_load_entity_field_float" (func (param i64 i32) (result f32)))"#;
    // A float access to a string field.
    let err = translate_err(&format!(
        r#"(module {load} (func (export "f") (param i64) (result f32) (call 0 (local.get 0) (i32.const 28))))"#
    ));
    assert!(
        err.contains("entity field offset 28 is not a known float field"),
        "{err}"
    );
    // Past the end of the system fields.
    let err = translate_err(&format!(
        r#"(module {load} (func (export "f") (param i64) (result f32) (call 0 (local.get 0) (i32.const 105))))"#
    ));
    assert!(
        err.contains("entity field offset 105 is not a known float field"),
        "{err}"
    );
    // A non-constant offset.
    let err = translate_err(&format!(
        r#"(module {load} (func (export "f") (param i64 i32) (result f32) (call 0 (local.get 0) (local.get 1))))"#
    ));
    assert!(
        err.contains("entity field offset must be a compile-time constant"),
        "{err}"
    );
}

#[test]
fn static_data() {
    insta::assert_snapshot!(translate_wat("static_data.wat"));
    check_compiles(&wat::parse_file(fixture("static_data.wat")).unwrap(), "");
}

#[test]
fn static_data_errors() {
    let base =
        r#"(import "env" "memory" (memory 1)) (import "env" "__memory_base" (global $b i32))"#;
    let err = translate_err(&format!(
        r#"(module {base} (data (global.get $b) "\00\00\00\00")
            (func (export "f") (f32.store (global.get $b) (f32.const 1))))"#
    ));
    assert!(
        err.contains("writes to static data are not supported"),
        "{err}"
    );
    let err = translate_err(r#"(module (memory 1) (data (i32.const 64) "\01"))"#);
    assert!(err.contains("is not relative to `__memory_base`"), "{err}");
    let err = translate_err(&format!(
        r#"(module {base} (data (global.get $b) "\00\00\00\00")
            (func (export "f") (result f32) (f32.load offset=8 (global.get $b))))"#
    ));
    assert!(err.contains("outside of static data"), "{err}");
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
    assert!(
        err.contains("unknown QuakeC intrinsic `__quakec_bogus`"),
        "{err}"
    );
}

#[test]
fn string_literals() {
    insta::assert_snapshot!(translate_wat("strings.wat"));
    check_compiles(&wat::parse_file(fixture("strings.wat")).unwrap(), "");
}

#[test]
fn string_literal_errors() {
    let base = r#"(import "env" "memory" (memory 1)) (import "env" "__memory_base" (global $b i32))
        (import "env" "__quakec_string_literal" (func $lit (param i32 i32) (result i64)))
        (data (global.get $b) "abcd")"#;
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32) (result i64)
            (call $lit (i32.add (global.get $b) (local.get 0)) (i32.const 1))))"#
    ));
    assert!(
        err.contains("string literal address is not a constant"),
        "{err}"
    );
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32) (result i64)
            (call $lit (global.get $b) (local.get 0))))"#
    ));
    assert!(err.contains("string literal length"), "{err}");
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (result i64)
            (call $lit (global.get $b) (i32.const 5))))"#
    ));
    assert!(err.contains("outside of static data"), "{err}");
}

#[test]
fn merged_constants() {
    insta::assert_snapshot!(translate_wat("merged_constants.wat"));
    check_compiles(
        &wat::parse_file(fixture("merged_constants.wat")).unwrap(),
        "",
    );
}

#[test]
fn merged_functions() {
    insta::assert_snapshot!(translate_wat("merged_functions.wat"));
    check_compiles(
        &wat::parse_file(fixture("merged_functions.wat")).unwrap(),
        "",
    );
}

#[test]
fn merged_function_errors() {
    let base = r#"(import "env" "__indirect_function_table" (table 1 funcref))
        (import "env" "__table_base" (global $tb i32))
        (import "env" "__quakec_func_ref" (func $func_ref (param i32) (result funcref)))
        (elem (global.get $tb) func $a)
        (func $a)
        (func $use (param i32) (result funcref) (call $func_ref (local.get 0)))"#;
    // A caller passing a computed function pointer.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32) (result funcref)
            (call $use (i32.mul (local.get 0) (i32.const 2)))))"#
    ));
    assert!(err.contains("in argument 0 of `__wasm_use`"), "{err}");
    assert!(err.contains("is not a constant"), "{err}");
    // A parameter also used as a number isn't retyped.
    let err = translate_err(&format!(
        r#"(module {base}
            (func $g (param i32) (result funcref)
              (drop (i32.mul (local.get 0) (i32.const 2)))
              (call $func_ref (local.get 0)))
            (func (export "f") (result funcref) (call $g (global.get $tb))))"#
    ));
    assert!(
        err.contains("function pointer in local `p0` is not a constant on every path here"),
        "{err}"
    );
}

#[test]
fn merged_constant_errors() {
    let base = r#"(import "env" "__indirect_function_table" (table 1 funcref))
        (import "env" "__table_base" (global $tb i32))
        (import "env" "__quakec_func_ref" (func $func_ref (param i32) (result funcref)))
        (elem (global.get $tb) func $a)
        (func $a)"#;
    // A function pointer passed in from the caller.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32) (result funcref)
            (call $func_ref (local.get 0))))"#
    ));
    assert!(
        err.contains("function pointer in local `p0` is not a constant on every path here"),
        "{err}"
    );
    // A local that is sometimes assigned a computed value.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32) (result funcref) (local i32)
            (local.set 1 (global.get $tb))
            (if (local.get 0) (then (local.set 1 (i32.mul (local.get 0) (i32.const 2)))))
            (call $func_ref (local.get 1))))"#
    ));
    assert!(
        err.contains("function pointer in local `l0` is not a constant on every path here"),
        "{err}"
    );
}

#[test]
fn stack_pointer_use_is_an_error() {
    let err = translate_err(
        r#"(module (import "env" "__stack_pointer" (global $sp (mut i32)))
            (func (export "f") (global.set $sp (i32.sub (global.get $sp) (i32.const 16)))))"#,
    );
    assert!(
        err.contains("uses the shadow stack (`__stack_pointer`)"),
        "{err}"
    );
}

#[test]
fn simd_lanes() {
    insta::assert_snapshot!(translate_wat("simd_lanes.wat"));
    check_compiles(&wat::parse_file(fixture("simd_lanes.wat")).unwrap(), "");
}
