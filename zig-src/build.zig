const std = @import("std");

pub fn build(b: *std.Build) void {
    // wasm32 with every feature enabled (notably reference types, SIMD and
    // tail calls), which is what wasm-to-qc expects.
    const target = b.resolveTargetQuery(.{
        .cpu_arch = .wasm32,
        .os_tag = .freestanding,
        .cpu_model = .{
            .explicit = &.{
                .name = "bleeding_edge",
                .llvm_name = "bleeding-edge",
                .features = .{
                    .ints = .{ 260083, 0, 0, 0, 0, 0 },
                },
            },
        },
    });

    // `addModule` makes this module available to consumers of this package.
    const mod = b.addModule("quakec_zig", moduleOptions(b, "src/root.zig", target, .small));
    addOptLibrary(b, "test", mod);

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
