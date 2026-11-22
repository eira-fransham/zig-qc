const std = @import("std");

pub fn build(b: *std.Build) void {
    // wasm32 with every feature enabled (notably reference types, SIMD and
    // tail calls), which is what zig-qc expects, except relaxed SIMD:
    // QuakeC has no equivalent, and wasm-opt is run without it.
    const target = b.resolveTargetQuery(.{
        .cpu_arch = .wasm32,
        .os_tag = .freestanding,
        .cpu_model = .{
            .explicit = &std.Target.wasm.cpu.bleeding_edge,
        },
        .cpu_features_sub = std.Target.wasm.featureSet(&.{.relaxed_simd}),
    });

    const out_name = b.option([]const u8, "out-name", "Name of the installed .wasm file") orelse "test.wasm";

    const out_name_no_ext = std.mem.trimEnd(u8, out_name, ".wasm");

    // `addModule` makes this module available to consumers of this package.
    const mod = b.addModule("test", moduleOptions(b, "src/root.zig", target, .small));
    addOptLibrary(b, out_name_no_ext, mod);

    const stress_test = b.createModule(moduleOptions(b, "src/stress-test.zig", target, .small));
    addOptLibrary(b, "stress-test", stress_test);

    const mod_tests = b.addTest(.{
        .root_module = mod,
    });
    const run_mod_tests = b.addRunArtifact(mod_tests);
    const test_step = b.step("test", "Run tests");
    test_step.dependOn(&run_mod_tests.step);
}

fn moduleOptions(
    b: *std.Build,
    root_source_file: []const u8,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
) std.Build.Module.CreateOptions {
    return .{
        .root_source_file = b.path(root_source_file),
        .target = target,
        .optimize = optimize,
        .pic = true,
        .strip = true,
    };
}

/// Build `root_module` as a wasm library, optimise it with `wasm-opt`, and
/// install the result as `lib/<name>.wasm`.
fn addOptLibrary(b: *std.Build, name: []const u8, root_module: *std.Build.Module) void {
    const lib = b.addLibrary(.{
        .name = b.fmt("{s}-unopt", .{name}),
        .linkage = .dynamic,
        .root_module = root_module,
    });

    const wasm_opt = b.addSystemCommand(&.{ "wasm-opt", "-Oz" });
    wasm_opt.addArtifactArg(lib);
    wasm_opt.addArgs(&.{
        "--enable-reference-types",
        "--enable-bulk-memory-opt",
        "--enable-simd",
        "--enable-tail-call",
        "--enable-nontrapping-float-to-int",
        "--enable-extended-const",
    });
    wasm_opt.addArg("-o");
    const optimized = wasm_opt.addOutputFileArg2(name, .{});

    const install = b.addInstallLibFile(optimized, b.fmt("{s}.wasm", .{name}));
    b.getInstallStep().dependOn(&install.step);
}
