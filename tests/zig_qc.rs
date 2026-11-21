use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn temp_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("zig-qc-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn zig_qc(dir: &Path, args: &[&str], path: Option<&OsString>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_zig-qc"));
    cmd.args(args).current_dir(dir);
    if let Some(path) = path {
        cmd.env("PATH", path);
    }
    cmd.output().unwrap()
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

#[test]
fn init() {
    let dir = temp_dir();
    let out = zig_qc(&dir, &["init", "my_mod"], None);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let build = std::fs::read_to_string(dir.join("build.zig")).unwrap();
    assert!(build.contains("\"my_mod-unopt\"") && build.contains("\"out-name\""), "{build}");
    assert!(!build.contains("@NAME@"));
    let zon = std::fs::read_to_string(dir.join("build.zig.zon")).unwrap();
    assert!(zon.contains(".name = .my_mod,"), "{zon}");
    // Zig's fingerprint check: the high half is the CRC-32 of the name.
    assert!(zon.contains(".fingerprint = 0x8ab1aa37"), "{zon}");
    let defs = std::fs::read_to_string(dir.join("src/defs.zig")).unwrap();
    assert_eq!(defs, std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/zig-src/src/defs.zig")).unwrap());
    assert!(dir.join("src/root.zig").is_file());

    // Never overwrites an existing project.
    let out = zig_qc(&dir, &["init", "my_mod"], None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("already exists"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn init_rejects_bad_names() {
    for name in ["my-mod", "fn", "1st", "", &"x".repeat(33)] {
        let dir = temp_dir();
        let out = zig_qc(&dir, &["init", name], None);
        assert!(!out.status.success(), "accepted `{name}`");
        assert!(String::from_utf8_lossy(&out.stderr).contains("invalid project name"), "`{name}`");
        assert!(!dir.join("build.zig").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn build_reports_missing_tools() {
    let dir = temp_dir();
    let out = zig_qc(&dir, &["build"], Some(&OsString::new()));
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    for hint in ["ziglang.org", "www.fteqcc.org", "binaryen"] {
        assert!(err.contains(hint), "missing `{hint}` in:\n{err}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// The whole pipeline, if `zig`, `wasm-opt` and fteqcc are available.
#[test]
fn init_and_build() {
    let fteqcc = std::env::var_os("FTEQCC")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../fteqcc-mac/fteqcc"));
    if !on_path("zig") || !on_path("wasm-opt") || !fteqcc.is_file() {
        eprintln!("skipping: zig, wasm-opt or fteqcc not found");
        return;
    }
    // fteqcc must be found as `fteqcc`; put its directory first on the PATH,
    // via a symlink in case it has a different file name.
    let dir = temp_dir();
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(std::fs::canonicalize(&fteqcc).unwrap(), bin.join("fteqcc")).unwrap();
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let path = std::env::join_paths(paths).unwrap();

    let project = dir.join("project");
    std::fs::create_dir_all(&project).unwrap();
    assert!(zig_qc(&project, &["init", "demo"], None).status.success());
    let out = zig_qc(&project, &["build"], Some(&path));
    assert!(
        out.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let lib = project.join("zig-out/lib");
    assert!(lib.join("progs.dat").is_file());
    let qc = std::fs::read_to_string(lib.join("zig-qc-build.qc")).unwrap();
    assert!(qc.contains(r#"dprint("hello from demo\n");"#), "{qc}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn init_name_from_directory() {
    let dir = temp_dir();
    let project = dir.join("1st-cool.mod");
    std::fs::create_dir_all(&project).unwrap();
    let out = zig_qc(&project, &["init"], None);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("using project name `_1st_cool_mod`"));
    let zon = std::fs::read_to_string(project.join("build.zig.zon")).unwrap();
    assert!(zon.contains(".name = ._1st_cool_mod,"), "{zon}");
    assert!(zon.contains(".fingerprint = 0x66aa6823"), "{zon}");

    // A directory name that can't be made into an identifier.
    let project = dir.join("fn");
    std::fs::create_dir_all(&project).unwrap();
    let out = zig_qc(&project, &["init"], None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("pass one explicitly"));
    assert!(!project.join("build.zig").exists());
    std::fs::remove_dir_all(&dir).ok();
}
