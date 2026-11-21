//! By convention, root.zig is the root source file when making a package.

pub const defs = @import("defs.zig");
pub const Entity = defs.Entity;

pub export fn doSomethingElse(_: Entity) f32 {
    return 0;
}
