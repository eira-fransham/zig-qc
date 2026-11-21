//! By convention, root.zig is the root source file when making a package.

pub const defs = @import("defs.zig");
pub const Entity = defs.Entity;

pub extern fn doSomething(foo: Entity, bar: Entity, baz: f32) f32;

pub export fn doSomethingElse(foo: Entity) f32 {
    const main = defs.main.get();
    main();
    const val = doSomething(foo, defs.world.get(), defs.time.get());
    defs.v_up_x.set(val);
    return val;
}
