//! Runs converted code with `run-qc`, checking the results: `qc-test`, built
//! with `zig-qc`, and WAT fixtures. Arguments and results are JSON (see
//! `run-qc --help`). Skipped if `zig`, `wasm-opt` (for
//! `qc-test`), `run-qc` or fteqcc isn't available.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde_json::{Value, json};

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

fn fteqcc() -> PathBuf {
    std::env::var_os("FTEQCC")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../fteqcc-mac/fteqcc"))
}

/// Convert `tests/fixtures/<name>` and compile it with fteqcc (its default
/// optimisations), returning its `progs.dat`, or `None` if the tools aren't
/// available.
fn fixture_progs(name: &str) -> Option<PathBuf> {
    let fteqcc = fteqcc();
    if !on_path("run-qc") || !fteqcc.is_file() {
        eprintln!("skipping: run-qc or fteqcc not found");
        return None;
    }
    let wasm = wat::parse_file(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap();
    let qc = wasm_to_qc::translate(&wasm, &wasm_to_qc::Options::default())
        .unwrap()
        .source;
    let dir = std::env::temp_dir().join(format!("run-qc-fixture-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("out.qc"), qc).unwrap();
    std::fs::write(dir.join("progs.src"), "progs.dat\nout.qc\n").unwrap();
    let out = Command::new(&fteqcc).current_dir(&dir).output().unwrap();
    assert!(
        out.status.success() && dir.join("progs.dat").is_file(),
        "fteqcc failed on {name}:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    Some(dir.join("progs.dat"))
}

/// Build `qc-test` (once per test run), returning its `progs.dat`, or `None`
/// if the tools aren't available.
fn progs() -> Option<&'static Path> {
    static PROGS: OnceLock<Option<PathBuf>> = OnceLock::new();
    PROGS
        .get_or_init(|| {
            let fteqcc = fteqcc();
            if !on_path("zig") || !on_path("wasm-opt") || !on_path("run-qc") || !fteqcc.is_file() {
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

/// Run `func(args)` in `progs` with `globals` set (a JSON object, or
/// `None`), and return what `run-qc` prints, as JSON. Each argument is JSON
/// (a number, `[x, y, z]`, `null`, or an object for a new entity), or else a
/// string.
fn run(
    progs: &Path,
    func: &str,
    args: &[&str],
    globals: Option<&str>,
    print_globals: bool,
) -> Value {
    let mut cmd = Command::new("run-qc");
    if let Some(g) = globals {
        cmd.args(["-g", g]);
    }
    if print_globals {
        cmd.arg("--print-globals");
    }
    cmd.arg(progs).arg(func);
    // `--` so that negative numbers aren't taken as options.
    cmd.arg("--");
    cmd.args(args);
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "run-qc {func} {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = String::from_utf8(out.stdout).unwrap();
    serde_json::from_str(&out)
        .unwrap_or_else(|e| panic!("`{func}` printed invalid JSON ({e}): {out:?}"))
}

/// What `func(args)` in `qc-test` returns, or `None` if it can't be run.
fn call(func: &str, args: &[&str]) -> Option<Value> {
    Some(run(progs()?, func, args, None, false))
}

/// What `func(args)` in `qc-test` returns with `globals` set, and the
/// globals afterwards.
fn call_globals(func: &str, args: &[&str], globals: Option<&str>) -> Option<(Value, Value)> {
    let mut out = run(progs()?, func, args, globals, true);
    let ret = out["return"].take();
    Some((ret, out["globals"].take()))
}

/// Check that `func(args)` in `qc-test` returns `expected`.
fn check(func: &str, args: &[f32], globals: Option<&str>, expected: f32) {
    if let Some(progs) = progs() {
        check_in(progs, func, args, globals, expected);
    }
}

/// Check that `func(args)` in `progs` returns `expected`.
fn check_in(progs: &Path, func: &str, args: &[f32], globals: Option<&str>, expected: f32) {
    let args: Vec<String> = args.iter().map(f32::to_string).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let got = run(progs, func, &args, globals, false);
    assert_eq!(
        got,
        json!(expected),
        "{func}({args:?}) with globals {globals:?}"
    );
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
    for (m, days) in [
        (1, 31),
        (2, 28),
        (4, 30),
        (7, 31),
        (11, 30),
        (12, 31),
        (0, 0),
        (13, 0),
    ] {
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

/// Project Euler 19 (checked against Python's `datetime`): a large function
/// with nested loops, a `switch` LLVM turns into a bit test, a remainder,
/// and counters LLVM vectorizes into `i32x4`.
#[test]
fn stress_sundays_on_first() {
    for (first, last, count) in [
        (1901, 2000, 171),
        (1900, 1900, 2),
        (1901, 1901, 2),
        (2000, 2000, 1),
        // Near Quake's limit of 100000 statements per call (2100 is over).
        (1900, 2050, 259),
    ] {
        check(
            "sundays_on_first",
            &[first as f32, last as f32],
            None,
            count as f32,
        );
    }
}

/// `tests/fixtures/integers.wat`: integer division and remainder, bit tests,
/// and `i32x4` counters and masks.
#[test]
fn fixture_integers() {
    let Some(progs) = fixture_progs("integers.wat") else {
        return;
    };
    let check = |func, args: &[f32], expected| check_in(&progs, func, args, None, expected);
    check("div_s", &[7.0, 2.0], 3.0);
    check("div_s", &[-7.0, 2.0], -3.0);
    check("div_s", &[7.0, -2.0], -3.0);
    check("rem_u", &[23.0], 2.0);
    check("rem_u", &[21.0], 0.0);
    for m in 0..13 {
        let thirty = [4, 6, 9, 11].contains(&m);
        check(
            "is_30_day_month",
            &[m as f32],
            if thirty { 1.0 } else { 0.0 },
        );
    }
    // Bits 0 and 2 of 5.
    for (i, bit) in [(0, 1.0), (1, 0.0), (2, 1.0), (3, 0.0)] {
        check("bit_of", &[i as f32], bit);
    }
    // `n % 3 + n % 5`.
    for n in [1, 3, 7, 15, 16] {
        check("wrap", &[n as f32], (n % 3 + n % 5) as f32);
    }
    // How many of the arguments are below 1.
    check("count_lt", &[0.0, 2.0, 3.0], 1.0);
    check("count_lt", &[0.0, 2.0, -3.0], 2.0);
    check("count_lt", &[5.0, 2.0, 3.0], 0.0);
    check("count_lt", &[-1.0, 0.5, 0.0], 3.0);
}

// ---- entities, strings and vectors ----

/// The value of `field` of the entity `e` (as `run-qc` prints it).
fn field<'a>(e: &'a Value, field: &str) -> &'a Value {
    assert!(e.is_object(), "expected an entity, got {e}");
    &e[field]
}

#[test]
fn entity_fields() {
    // Armor absorbs up to half the damage.
    let Some(e) = call("take_damage", &[r#"{"health": 100, "armor": 10}"#, "30"]) else {
        return;
    };
    assert_eq!(field(&e, "health"), &json!(80.0));
    assert_eq!(field(&e, "armor"), &json!(0.0));
    let e = call("take_damage", &[r#"{"health": 100, "armor": 20}"#, "30"]).unwrap();
    assert_eq!(field(&e, "health"), &json!(85.0));
    assert_eq!(field(&e, "armor"), &json!(5.0));
    assert_eq!(field(&e, "classname"), &Value::Null);
    // Killed: renamed.
    let e = call(
        "take_damage",
        &[r#"{"health": 10, "classname": "player"}"#, "30"],
    )
    .unwrap();
    assert_eq!(field(&e, "health"), &json!(-20.0));
    assert_eq!(field(&e, "classname"), &json!("corpse"));
}

#[test]
fn vector_fields() {
    let Some(v) = call("speed_sq", &[r#"{"velocity": [1, 2, 3]}"#]) else {
        return;
    };
    assert_eq!(v, json!(14.0));
    assert_eq!(call("speed_sq", &["{}"]).unwrap(), json!(0.0));
}

#[test]
fn string_arguments() {
    let named = |e, n| call("is_named", &[e, n]);
    let Some(v) = named(r#"{"classname": "player"}"#, "player") else {
        return;
    };
    assert_eq!(v, json!(1.0));
    assert_eq!(
        named(r#"{"classname": "player"}"#, "monster").unwrap(),
        json!(0.0)
    );
    assert_eq!(named("{}", "player").unwrap(), json!(0.0));
}

#[test]
fn null_arguments() {
    for (e, s, flags) in [
        ("null", "null", 3.0),
        ("{}", "hi", 0.0),
        ("null", "hi", 1.0),
        ("{}", "null", 2.0),
    ] {
        let Some(v) = call("null_flags", &[e, s]) else {
            return;
        };
        assert_eq!(v, json!(flags), "null_flags({e}, {s})");
    }
}

/// A loop following `.owner` links between entities.
#[test]
fn entity_chains() {
    let Some(v) = call(
        "chain_health",
        &[r#"{"health": 1, "owner": {"health": 2, "owner": {"health": 4}}}"#],
    ) else {
        return;
    };
    assert_eq!(v, json!(7.0));
    assert_eq!(call("chain_health", &["null"]).unwrap(), json!(0.0));
}

/// One wasm function used at two QuakeC types (entity and string), so
/// emitted twice.
#[test]
fn polymorphic_function() {
    let (a, b) = (r#"{"health": 1}"#, r#"{"health": 2}"#);
    let Some(v) = call("pick_health", &["1", a, b]) else {
        return;
    };
    assert_eq!(v, json!(1.0));
    assert_eq!(call("pick_health", &["0", a, b]).unwrap(), json!(2.0));
    assert_eq!(
        call("pick_is_axe", &["1", "axe", "shotgun"]).unwrap(),
        json!(1.0)
    );
    assert_eq!(
        call("pick_is_axe", &["0", "axe", "shotgun"]).unwrap(),
        json!(0.0)
    );
}

/// `pick_entity` and `pick_string` are one wasm function (LLVM merges them)
/// with two exports.
#[test]
fn merged_exports() {
    let Some(e) = call(
        "pick_entity",
        &["1", r#"{"health": 1}"#, r#"{"health": 2}"#],
    ) else {
        return;
    };
    assert_eq!(field(&e, "health"), &json!(1.0));
    assert_eq!(
        call("pick_entity", &["0", r#"{"health": 1}"#, "null"]).unwrap(),
        Value::Null
    );
    assert_eq!(call("pick_string", &["0", "a", "b"]).unwrap(), json!("b"));
    assert_eq!(call("pick_string", &["1", "a", "b"]).unwrap(), json!("a"));
}

/// A `switch` LLVM turns into a range check (an unsigned comparison of
/// `w - 1`) and a table of string pointers in static data.
#[test]
fn string_table() {
    for (w, name) in [
        ("-3", "none"),
        ("0", "none"),
        ("1", "axe"),
        ("2", "shotgun"),
        ("3", "nailgun"),
        ("4", "rocket launcher"),
        ("5", "none"),
    ] {
        let Some(v) = call("weapon_name", &[w]) else {
            return;
        };
        assert_eq!(v, json!(name), "weapon_name({w})");
    }
}

#[test]
fn vector_arguments_and_globals() {
    let Some((ret, globals)) = call_globals("midpoint", &["[1, 0, -2]", "[3, 4, 2]"], None) else {
        return;
    };
    assert_eq!(ret, json!([2.0, 2.0, 0.0]));
    assert_eq!(globals["last_dir"], json!([2.0, 4.0, 4.0]));
}

/// The `self` global, set to an entity.
#[test]
fn self_global() {
    let s = r#"{"self": {"health": 50, "max_health": 100}}"#;
    let Some((ret, globals)) = call_globals("heal_self", &["30"], Some(s)) else {
        return;
    };
    assert_eq!(ret, json!(80.0));
    assert_eq!(field(&globals["self"], "health"), &json!(80.0));
    // Capped at `max_health`.
    let (ret, globals) = call_globals("heal_self", &["70"], Some(s)).unwrap();
    assert_eq!(ret, json!(100.0));
    assert_eq!(field(&globals["self"], "health"), &json!(100.0));
}

/// A function stored in an entity's `.think`, then called through it.
#[test]
fn function_fields() {
    let Some(e) = call("call_through_field", &[r#"{"frags": 1}"#]) else {
        return;
    };
    assert_eq!(field(&e, "frags"), &json!(4.0));
    assert!(field(&e, "think").is_string(), "think: {e}");
}

/// `tests/fixtures/llvm_idioms.wat`: a relocated pointer table, unsigned
/// comparisons, float sign tests and a function with two exports.
#[test]
fn fixture_llvm_idioms() {
    let Some(progs) = fixture_progs("llvm_idioms.wat") else {
        return;
    };
    let call = |func, args: &[&str]| run(&progs, func, args, None, false);
    for (w, name) in [
        ("-1", "none"),
        ("0", "none"),
        ("1", "axe"),
        ("2", "shotgun"),
        ("3", "none"),
    ] {
        assert_eq!(call("weapon_name", &[w]), json!(name), "weapon_name({w})");
    }
    // Negative numbers are at least 2^31 unsigned.
    for (a, b, lt) in [
        ("2", "3", 1.0),
        ("3", "2", 0.0),
        ("-1", "1", 0.0),
        ("1", "-1", 1.0),
        ("-3", "-2", 1.0),
        ("-2", "-3", 0.0),
    ] {
        assert_eq!(call("lt_u", &[a, b]), json!(lt), "lt_u({a}, {b})");
    }
    for (x, r) in [("10", 4.0), ("6", 0.0), ("3", 0.0), ("-1", -7.0)] {
        assert_eq!(call("sat_sub", &[x]), json!(r), "sat_sub({x})");
    }
    assert_eq!(call("count_to", &["5"]), json!(5.0));
    assert_eq!(call("is_negative", &["-2"]), json!(1.0));
    assert_eq!(call("is_negative", &["2"]), json!(0.0));
    for (a, b, d) in [
        ("1", "-1", 1.0),
        ("-1", "1", 1.0),
        ("1", "2", 0.0),
        ("-1", "-2", 0.0),
    ] {
        assert_eq!(
            call("signs_differ", &[a, b]),
            json!(d),
            "signs_differ({a}, {b})"
        );
    }
    for f in ["pick_a", "pick_b"] {
        let e = call(f, &["1", r#"{"health": 1}"#, r#"{"health": 2}"#]);
        assert_eq!(field(&e, "health"), &json!(1.0), "{f}");
        let e = call(f, &["0", r#"{"health": 1}"#, r#"{"health": 2}"#]);
        assert_eq!(field(&e, "health"), &json!(2.0), "{f}");
    }
}
