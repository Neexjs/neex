// build.zig - Neex Native WASM Build Configuration
// Targets wasm32-freestanding for universal cross-platform support
// Compatible with Zig 0.15.x

const std = @import("std");

pub fn build(b: *std.Build) void {
    // =========================================================================
    // WASM Build (Default - Universal)
    // =========================================================================

    const wasm_target = b.resolveTargetQuery(.{
        .cpu_arch = .wasm32,
        .os_tag = .freestanding,
    });

    const optimize = b.standardOptimizeOption(.{
        .preferred_optimize_mode = .ReleaseSmall,
    });

    // Create WASM library (Zig 0.15+ API)
    const wasm = b.addExecutable(.{
        .name = "neex",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = wasm_target,
            .optimize = optimize,
        }),
    });

    // WASM-specific settings
    wasm.entry = .disabled; // No entry point for library
    wasm.rdynamic = true; // Export all public functions

    // Install the WASM file
    b.installArtifact(wasm);

    // =========================================================================
    // Tests (Native - for development)
    // =========================================================================

    const native_target = b.standardTargetOptions(.{});

    const tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = native_target,
            .optimize = .Debug,
        }),
    });

    const run_tests = b.addRunArtifact(tests);
    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_tests.step);
}
