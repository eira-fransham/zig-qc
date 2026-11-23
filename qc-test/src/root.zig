//! Functions run by `tests/run_qc.rs` with `run-qc`, which doesn't support
//! builtins: nothing here may call one.

const defs = @import("defs.zig");
const String = defs.String;
const Vector = defs.Vector;

/// A user global, set with `run-qc -g`.
const counter = defs.userGlobal(f32, "counter");
const name = defs.userGlobal(String, "name");

pub export fn add(a: f32, b: f32) f32 {
    return a + b;
}

/// Loops and integer arithmetic.
pub export fn factorial(n: f32) f32 {
    var acc: f32 = 1;
    var i: i32 = 2;
    const end: i32 = @intFromFloat(n);
    while (i <= end) : (i += 1) acc *= @floatFromInt(i);
    return acc;
}

/// Recursion.
pub export fn fib(n: f32) f32 {
    return fibRec(@intFromFloat(n));
}

noinline fn fibRec(n: i32) f32 {
    if (n < 2) return @floatFromInt(n);
    return fibRec(n - 1) + fibRec(n - 2);
}

/// A `switch`, which LLVM may turn into a lookup table in static data.
pub export fn days_in_month(m: f32) f32 {
    const month: u32 = @intFromFloat(m);
    return switch (month) {
        1, 3, 5, 7, 8, 10, 12 => 31,
        4, 6, 9, 11 => 30,
        2 => 28,
        else => 0,
    };
}

/// Float → int truncation, and comparisons.
pub export fn clamp_trunc(x: f32, lo: f32, hi: f32) f32 {
    const t: i32 = @intFromFloat(x);
    const f: f32 = @floatFromInt(t);
    return if (f < lo) lo else if (f > hi) hi else f;
}

fn twice(x: f32) callconv(.c) f32 {
    return x * 2;
}

fn negate(x: f32) callconv(.c) f32 {
    return -x;
}

/// Calls through a function pointer chosen at run time.
pub export fn apply(op: f32, x: f32) f32 {
    const f: *const fn (f32) callconv(.c) f32 = if (op == 0) &twice else &negate;
    return call(f, x);
}

noinline fn call(f: *const fn (f32) callconv(.c) f32, x: f32) f32 {
    return f(x);
}

/// Vector arithmetic: the length squared of `(x, y, z) * 2 + (1, 1, 1)`.
pub export fn vector_math(x: f32, y: f32, z: f32) f32 {
    const v: Vector = .{ x, y, z };
    const w = v * @as(Vector, @splat(2)) + @as(Vector, @splat(1));
    return defs.dot(w, w);
}

/// Reads and writes a user global.
pub export fn bump(by: f32) f32 {
    counter.set(counter.get() + by);
    return counter.get();
}

/// Reads a string global, and compares it with string literals.
pub export fn name_code() f32 {
    const n = name.get();
    if (n.eql(String.lit("player"))) return 1;
    if (n.eql(String.lit("monster"))) return 2;
    if (n.isNull()) return 0;
    return 3;
}

/// A stress test: Project Euler 19. The number of months starting on a
/// Sunday from the start of `first_year` to the end of `last_year` (both at
/// least 1900). Everything is counted from 1 January 1900 (a Monday), with
/// running counters in place of division, which LLVM would turn into 64-bit
/// multiplications.
pub export fn sundays_on_first(first_year: f32, last_year: f32) f32 {
    const first: u32 = @intFromFloat(first_year);
    const last: u32 = @intFromFloat(last_year);
    var count: u32 = 0;
    // Day of the week, 0 = Sunday.
    var dow: u32 = 1;
    // `year` modulo 4, 100 and 400.
    var mod4: u32 = 0;
    var mod100: u32 = 0;
    var mod400: u32 = 300;
    var year: u32 = 1900;
    while (year <= last) : (year += 1) {
        const leap = mod4 == 0 and (mod100 != 0 or mod400 == 0);
        var month: u32 = 1;
        while (month <= 12) : (month += 1) {
            if (year >= first and dow == 0) count += 1;
            const days: u32 = switch (month) {
                4, 6, 9, 11 => 30,
                2 => if (leap) 29 else 28,
                else => 31,
            };
            dow += days;
            while (dow >= 7) dow -= 7;
        }
        mod4 += 1;
        if (mod4 == 4) mod4 = 0;
        mod100 += 1;
        if (mod100 == 100) mod100 = 0;
        mod400 += 1;
        if (mod400 == 400) mod400 = 0;
    }
    return @floatFromInt(count);
}

// ---- entities, strings and vectors as arguments and results ----

const Entity = defs.Entity;

const armor = defs.userField(f32, "armor");
const last_dir = defs.userGlobal(Vector, "last_dir");

/// Damage absorbed by up to half by armor (a user field, so through `get`
/// and `set`). Kills the entity (renaming it) at 0 health.
pub export fn take_damage(e: Entity, dmg: f32) Entity {
    var d = dmg;
    const a = e.get(armor);
    if (a > 0) {
        const absorbed = @min(a, d * 0.5);
        e.set(armor, a - absorbed);
        d -= absorbed;
    }
    e.health -= d;
    if (e.health <= 0) e.classname = String.lit("corpse");
    return e;
}

/// A vector field, read whole.
pub export fn speed_sq(e: Entity) f32 {
    const v: Vector = e.velocity;
    return defs.dot(v, v);
}

/// A string field compared with a string argument.
pub export fn is_named(e: Entity, n: String) f32 {
    return if (e.classname.eql(n)) 1 else 0;
}

/// Null entities and strings: 1 if `e` is null, plus 2 if `s` is.
pub export fn null_flags(e: Entity, s: String) f32 {
    var flags: f32 = 0;
    if (e.isNull()) flags += 1;
    if (s.isNull()) flags += 2;
    return flags;
}

/// Follows `.owner` links: the total health along the chain, in a loop over
/// entities.
pub export fn chain_health(e: Entity) f32 {
    var total: f32 = 0;
    var cur = e;
    while (!cur.isNull()) : (cur = cur.owner) total += cur.health;
    return total;
}

/// One function used at two QuakeC types: it is emitted once per type.
noinline fn choose(c: bool, a: u32, b: u32) u32 {
    return if (c) a else b;
}

/// The health of `a` or `b`.
pub export fn pick_health(c: f32, a: Entity, b: Entity) f32 {
    const e: Entity = @ptrFromInt(choose(c != 0, @intFromPtr(a), @intFromPtr(b)));
    return e.health;
}

/// Whether `a` or `b` is "axe".
pub export fn pick_is_axe(c: f32, a: String, b: String) f32 {
    const s: String = @enumFromInt(choose(c != 0, @intFromEnum(a), @intFromEnum(b)));
    return if (s.eql(String.lit("axe"))) 1 else 0;
}

/// The target example: a field of `self`, updated in place.
pub export fn self_hurt() f32 {
    const s = defs.self.get();
    s.health -= 1;
    return s.health;
}

/// A vector field, written whole.
pub export fn nudge(e: Entity, d: Vector) Entity {
    e.origin = @as(Vector, e.origin) + d;
    return e;
}

/// A component of a vector field.
pub export fn lift(e: Entity) Entity {
    e.origin[2] += 8;
    return e;
}

/// System fields through `get` and `set`, which still work.
pub export fn old_style(e: Entity) Entity {
    e.set(.frags, e.get(.health) * 2);
    return e;
}

/// A string literal chosen by a `switch`.
pub export fn weapon_name(w: f32) String {
    const i: u32 = @intFromFloat(w);
    return switch (i) {
        1 => String.lit("axe"),
        2 => String.lit("shotgun"),
        3 => String.lit("nailgun"),
        4 => String.lit("rocket launcher"),
        else => String.lit("none"),
    };
}

/// Vector arguments and result, and a vector global.
pub export fn midpoint(a: Vector, b: Vector) Vector {
    last_dir.set(b - a);
    return (a + b) * @as(Vector, @splat(0.5));
}

/// The `self` global: heals it, up to its `max_health`.
pub export fn heal_self(by: f32) f32 {
    const s = defs.self.get();
    s.health = @min(s.health + by, s.max_health);
    return s.health;
}

fn add_frags(e: Entity, n: f32) callconv(.c) void {
    e.frags += n;
}

/// A function stored in an entity field, then called through it with
/// arguments (`wasm-opt` inlines `callThink`, but the call is still through
/// the loaded field).
pub export fn call_through_field(e: Entity) Entity {
    e.think = defs.func(add_frags);
    callThink(e);
    return e;
}

noinline fn callThink(e: Entity) void {
    const f = defs.callAs(fn (Entity, f32) callconv(.c) void, e.think);
    f(e, 1);
    f(e, 2);
}

// ---- the shadow stack: volatile locals live in memory ----

/// Two volatile locals.
pub export fn volatile_sum(a: f32, b: f32) f32 {
    var x: f32 = a;
    var y: f32 = b;
    const px: *volatile f32 = &x;
    const py: *volatile f32 = &y;
    px.* += 1;
    return px.* + py.*;
}

/// A volatile counter, read and written around a loop.
pub export fn volatile_counter(n: f32) f32 {
    var i: f32 = 0;
    const p: *volatile f32 = &i;
    while (p.* < n) p.* += 1;
    return p.*;
}

/// A store through a pointer to one of two locals.
pub export fn volatile_pick(c: f32, a: f32, b: f32) f32 {
    var x: f32 = a;
    var y: f32 = b;
    const px: *volatile f32 = &x;
    const py: *volatile f32 = &y;
    const p: *volatile f32 = if (c != 0) px else py;
    p.* = 10;
    return px.* + 2 * py.*;
}

/// A volatile entity.
pub export fn volatile_entity(e: Entity) f32 {
    var l: Entity = e;
    const p: *volatile Entity = &l;
    return p.*.get(.health);
}

/// A volatile vector.
pub export fn volatile_vector(v: Vector) Vector {
    var l: Vector = v;
    const p: *volatile Vector = &l;
    p.* = p.* + @as(Vector, @splat(1));
    return p.*;
}

/// A volatile `bool`.
pub export fn volatile_flag(c: f32) f32 {
    var f: bool = c > 0;
    const p: *volatile bool = &f;
    return if (p.*) 1 else 0;
}

/// A volatile local and a call (of the recursive `fibRec`, which can't be
/// inlined): the frame is allocated by moving `__stack_pointer`, and
/// restored.
pub export fn stack_frame_call(x: f32) f32 {
    var l: f32 = x;
    const p: *volatile f32 = &l;
    p.* = fibRec(@intFromFloat(p.*));
    return p.* + 1;
}
