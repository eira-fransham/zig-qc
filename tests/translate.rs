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
fn loops() {
    insta::assert_snapshot!(translate_wat("loops.wat"));
    check_compiles(&wat::parse_file(fixture("loops.wat")).unwrap(), "");
}

#[test]
fn joins() {
    insta::assert_snapshot!(translate_wat("joins.wat"));
    check_compiles(&wat::parse_file(fixture("joins.wat")).unwrap(), "");
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
        translate_err(r#"(module (import "builtin" "floor" (func (param v128) (result f32))))"#);
    assert!(
        err.contains("parameter 0 should be float, but the import has a v128"),
        "{err}"
    );
    let err =
        translate_err(r#"(module (import "builtin" "spawn" (func (param f32) (result i32))))"#);
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
        r#"(import "env" "__quakec_load_entity_field_float" (func (param i32 i32) (result f32)))"#;
    // A float access to a string field.
    let err = translate_err(&format!(
        r#"(module {load} (func (export "f") (param i32) (result f32) (call 0 (local.get 0) (i32.const 28))))"#
    ));
    assert!(
        err.contains("entity field offset 28 is not a known float field"),
        "{err}"
    );
    // Past the end of the system fields.
    let err = translate_err(&format!(
        r#"(module {load} (func (export "f") (param i32) (result f32) (call 0 (local.get 0) (i32.const 105))))"#
    ));
    assert!(
        err.contains("entity field offset 105 is not a known float field"),
        "{err}"
    );
    // A computed offset.
    let err = translate_err(&format!(
        r#"(module {load} (func (export "f") (param i32 i32) (result f32)
            (call 0 (local.get 0) (i32.mul (local.get 1) (i32.const 3)))))"#
    ));
    assert!(err.contains("type conflict"), "{err}");
    assert!(err.contains("as .float"), "{err}");
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
        (import "env" "__quakec_string_literal" (func $lit (param i32) (result i32)))
        (data (global.get $b) "abcd")"#;
    // A computed address.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32) (result i32)
            (call $lit (i32.mul (local.get 0) (i32.const 2)))))"#
    ));
    assert!(err.contains("type conflict"), "{err}");
    assert!(err.contains("as string"), "{err}");
    // No NUL terminator.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (result i32)
            (call $lit (global.get $b))))"#
    ));
    assert!(err.contains("not a NUL-terminated string"), "{err}");
    // Outside of static data.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (result i32)
            (call $lit (i32.add (global.get $b) (i32.const 10)))))"#
    ));
    assert!(err.contains("not a NUL-terminated string"), "{err}");
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
fn polymorphic() {
    insta::assert_snapshot!(translate_wat("polymorphic.wat"));
    check_compiles(&wat::parse_file(fixture("polymorphic.wat")).unwrap(), "");
}

#[test]
fn integers() {
    insta::assert_snapshot!(translate_wat("integers.wat"));
    check_compiles(&wat::parse_file(fixture("integers.wat")).unwrap(), "");
}

#[test]
fn llvm_idioms() {
    insta::assert_snapshot!(translate_wat("llvm_idioms.wat"));
    check_compiles(&wat::parse_file(fixture("llvm_idioms.wat")).unwrap(), "");
}

#[test]
fn function_pointer_errors() {
    let base = r#"(import "env" "__indirect_function_table" (table 2 funcref))
        (import "env" "__table_base" (global $tb i32))
        (import "env" "__quakec_load_entity" (func $self (param i32) (result i32)))
        (import "env" "__quakec_store_entity_field_fn" (func $set (param i32 i32 i32)))
        (elem (global.get $tb) func $a $a)
        (func $a)"#;
    // A computed function pointer.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32)
            (call $set (call $self (i32.const 112)) (i32.const 44)
              (i32.mul (local.get 0) (i32.const 2)))))"#
    ));
    assert!(err.contains("type conflict"), "{err}");
    assert!(err.contains("as void()"), "{err}");
    // The same value used as two different functions.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32)
            (call $set (call $self (i32.const 112)) (i32.const 44) (local.get 0))
            (call $set (call $self (i32.const 112)) (i32.const 44)
              (i32.add (local.get 0) (i32.const 1)))))"#
    ));
    assert!(
        err.contains("void() values are used at different offsets"),
        "{err}"
    );
    // An offset that changes in a loop.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32) (local i32)
            (loop
              (local.set 1 (i32.add (local.get 1) (i32.const 1)))
              (br_if 0 (local.get 0)))
            (call $set (call $self (i32.const 112)) (i32.const 44) (local.get 1))))"#
    ));
    assert!(err.contains("inconsistent offsets"), "{err}");
    // Not in the table.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f")
            (call $set (call $self (i32.const 112)) (i32.const 44) (i32.const 5))))"#
    ));
    assert!(
        err.contains("function pointer 5 is not in the function table"),
        "{err}"
    );
}

#[test]
fn global_address_errors() {
    let base = r#"(import "env" "__quakec_load_float" (func $load (param i32) (result f32)))"#;
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32) (result f32)
            (call $load (local.get 0))))"#
    ));
    assert!(
        err.contains("address of QuakeC global is not a constant"),
        "{err}"
    );
    // One of several constants, chosen at run time: each must be a global.
    let err = translate_err(&format!(
        r#"(module {base} (func (export "f") (param i32) (result f32)
            (call $load (select (i32.const 124) (i32.const 4) (local.get 0)))))"#
    ));
    assert!(err.contains("memory address 4"), "{err}");
}

#[test]
fn entity_struct() {
    insta::assert_snapshot!(translate_wat("entity_struct.wat"));
    check_compiles(&wat::parse_file(fixture("entity_struct.wat")).unwrap(), "");
}

#[test]
fn field_moves() {
    insta::assert_snapshot!(translate_wat("field_moves.wat"));
    check_compiles(&wat::parse_file(fixture("field_moves.wat")).unwrap(), "");
}

#[test]
fn entity_struct_errors() {
    let cases = [
        // Beyond the system fields.
        (
            "(f32.load offset=4000 (local.get $e))",
            "is not a system field",
        ),
        // Inside a field.
        ("(f32.load offset=194 (local.get $e))", "isn't a field"),
        // A float field read as an `i32` (its bits) and used as a number.
        (
            "(f32.convert_i32_s (i32.load offset=192 (local.get $e)))",
            "as float bits",
        ),
        // `&e.health` passed to a function.
        (
            "(drop (f32.load offset=192 (local.get $e))) (call $g (i32.add (local.get $e) (i32.const 192))) (f32.const 0)",
            "a pointer to an entity's field is used as a value",
        ),
        // An entity or a pointer to one of its fields.
        (
            "(f32.load (select (local.get $e) (i32.add (local.get $e) (i32.const 192)) (local.get $c)))",
            "inconsistent offsets",
        ),
    ];
    for (body, msg) in cases {
        let wat = format!(
            r#"(module (import "env" "memory" (memory 1))
                (import "env" "__quakec_load_entity_field_float" (func $lf (param i32 i32) (result f32)))
                (func $g (param i32) (drop (call $lf (local.get 0) (i32.const 0))))
                (func (export "f") (param $e i32) (param $c i32) (result f32) {body}))"#
        );
        let err = translate_err(&wat);
        assert!(err.contains(msg), "{body}: expected {msg:?} in {err}");
    }
}

#[test]
fn i64_is_an_error() {
    for wat in [
        r#"(module (func (export "f") (param i64)))"#,
        r#"(module (func (export "f") (result i32) (i32.wrap_i64 (i64.const 1))))"#,
    ] {
        let err = translate_err(wat);
        assert!(
            err.contains("64-bit integers aren't supported"),
            "{wat}: {err}"
        );
    }
}

#[test]
fn stack() {
    insta::assert_snapshot!(translate_wat("stack.wat"));
    check_compiles(&wat::parse_file(fixture("stack.wat")).unwrap(), "");
}

/// A function with a 16-byte stack frame at `$fp`, running `body`.
fn stack_err(body: &str) -> String {
    translate_err(&format!(
        r#"(module (import "env" "memory" (memory 1))
            (import "env" "__stack_pointer" (global $sp (mut i32)))
            (func $g (param i32))
            (func (export "f") (param $x f32) (param $i i32) (result f32)
              (local $fp i32)
              (global.set $sp (local.tee $fp (i32.sub (global.get $sp) (i32.const 16))))
              {body}
              (global.set $sp (i32.add (local.get $fp) (i32.const 16)))
              (f32.const 0)))"#
    ))
}

#[test]
fn stack_errors() {
    let cases = [
        (
            "(call $g (local.get $fp))",
            "the address of a local is passed to a function",
        ),
        (
            "(f32.store (i32.add (local.get $fp) (i32.shl (local.get $i) (i32.const 2))) (local.get $x))",
            "non-constant offset",
        ),
        (
            "(i32.store (local.get $fp) (i32.const 1)) (drop (i32.load16_u offset=2 (local.get $fp)))",
            "overlap",
        ),
        (
            "(f32.store (local.get $fp) (local.get $x)) (drop (i32.load (local.get $fp)))",
            "accessed as both f32 and i32",
        ),
        (
            "(i32.store8 (local.get $fp) (local.get $i)) (drop (i32.load8_s (local.get $fp)))",
            "sign-extending load",
        ),
        (
            "(i32.store offset=4 (local.get $fp) (local.get $fp))",
            "stored in memory",
        ),
    ];
    for (body, msg) in cases {
        let err = stack_err(body);
        assert!(err.contains("uses the shadow stack"), "{body}: {err}");
        assert!(err.contains(msg), "{body}: expected {msg:?} in {err}");
    }
}

#[test]
fn simd_lanes() {
    insta::assert_snapshot!(translate_wat("simd_lanes.wat"));
    check_compiles(&wat::parse_file(fixture("simd_lanes.wat")).unwrap(), "");
}
