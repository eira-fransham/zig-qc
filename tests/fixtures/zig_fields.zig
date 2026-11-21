//! Exercises entity fields, vectors, builtins and tail calls.
const defs = @import("defs.zig");
const Entity = defs.Entity;
const String = defs.String;
const Vector = defs.Vector;

/// Float, vector, string, entity and function fields; vector components.
pub export fn fields(e: Entity, name: String) void {
    e.set(.health, e.get(.health) + 10);
    e.set(.velocity, e.get(.origin) + e.get(.avelocity));
    e.set(.netname, name);
    e.set(.enemy, e.get(.owner));
    e.set(.think, e.get(.touch));
    const o = e.get(.origin);
    e.set(.nextthink, o[1]);
}

/// Vector builtins and building vectors from scalars.
pub export fn aim_at(e: Entity, x: f32) f32 {
    const dir = defs.normalize(.{ x, 2, 3 });
    defs.setorigin(e, dir);
    return defs.vlen(e.get(.velocity));
}

/// Ends in a tail call to a builtin.
pub export fn spawn_thing(m: String) Entity {
    const e = defs.spawn();
    defs.setmodel(e, m);
    return defs.nextent(e);
}
