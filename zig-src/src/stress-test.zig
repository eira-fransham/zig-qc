//! Exercises everything wasm-to-qc supports so far. Each export covers one
//! area; the converter's test suite translates this and compiles the result
//! with fteqcc.

const defs = @import("defs.zig");
const Entity = defs.Entity;
const String = defs.String;
const Vector = defs.Vector;

// ---- globals ----

/// Entity, string, float, bool and integer globals.
pub export fn scalar_globals() f32 {
    defs.other.set(defs.self.get());
    defs.msg_entity.set(defs.world.get());
    defs.mapname.set(defs.mapname.get());
    defs.force_retouch.set(!defs.deathmatch.get());
    defs.serverflags.set(defs.total_secrets.get() + 1);
    return defs.time.get() + defs.frametime.get();
}

/// Vector globals and their components.
pub export fn vector_globals() f32 {
    defs.v_up.set(defs.v_forward.get());
    defs.v_right_x.set(defs.v_forward_y.get());
    return defs.v_up_z.get();
}

/// Function globals: copying one, and calling one indirectly.
pub export fn function_globals() void {
    defs.StartFrame.set(defs.main.get());
    const main = defs.main.get();
    main();
}

// ---- entity fields ----

/// Every field type, and vector components.
pub export fn entity_fields(e: Entity, name: String) void {
    e.set(.health, e.get(.health) + 10);
    e.set(.velocity, e.get(.origin) + e.get(.avelocity));
    e.set(.netname, name);
    e.set(.enemy, e.get(.owner));
    e.set(.think, e.get(.touch));
    const o = e.get(.origin);
    e.set(.nextthink, o[1] + defs.time.get());
}

/// Building vectors from scalars.
pub export fn build_vector(e: Entity, x: f32, y: f32) Vector {
    const v: Vector = .{ x, y, 0 };
    e.set(.angles, v);
    e.set(.mins, @splat(x));
    return v + e.get(.maxs);
}

// ---- comparisons ----

/// Entity and string equality and null tests.
pub export fn comparisons(a: Entity, b: Entity, s: String) f32 {
    var score: f32 = 0;
    if (a.eql(b)) score += 1;
    if (a.neq(defs.world.get())) score += 2;
    if (b.isNull()) score += 4;
    if (s.eql(defs.mapname.get())) score += 8;
    if (s.neq(a.get(.classname))) score += 16;
    if (!s.isNull()) score += 32;
    return score;
}

// ---- string literals ----

/// String literals passed to builtins, stored in fields and compared.
pub export fn string_literals(e: Entity) void {
    _ = defs.precache_model(String.lit("progs/player.mdl"));
    defs.dprint(String.lit("hello, \"world\"\n"));
    e.set(.classname, String.lit("monster_ogre"));
    if (e.get(.netname).eql(String.lit("player"))) defs.bprint(String.lit("tab\there \\ \x80\xff\n"));
}

/// A literal chosen at runtime (LLVM may select between the two addresses).
pub export fn pick_string(x: bool) String {
    return if (x) String.lit("yes") else String.lit("no");
}

// ---- builtins ----

/// Builtins taking and returning entities, strings, vectors and floats.
pub export fn builtins(model: String) Entity {
    const e = defs.spawn();
    defs.setmodel(e, defs.precache_model(model));
    defs.makevectors(e.get(.angles));
    defs.traceline(e.get(.origin), e.get(.origin) + defs.v_forward.get(), 0, e);
    defs.setorigin(e, defs.normalize(defs.v_forward.get()));
    defs.dprint(defs.ftos(defs.vlen(e.get(.velocity)) + defs.random()));
    if (defs.trace_fraction.get() < 1) {
        e.set(.enemy, defs.trace_ent.get());
    }
    return defs.nextent(e);
}

// ---- control flow ----

/// A loop with a conditional exit.
pub export fn sum_to(n: f32) f32 {
    var acc: f32 = 0;
    var i: f32 = 0;
    while (i < n) : (i += 1) {
        acc += i;
    }
    return acc;
}

/// If/else chains and early returns.
pub export fn classify(x: f32) f32 {
    if (x < 0) return -1;
    if (x == 0) {
        return 0;
    } else if (x < 10) {
        return 1;
    }
    return 2;
}

/// A `switch` whose arms are only constants. LLVM turns this into a lookup
/// table in a static data segment, read with a dynamic address.
pub export fn pick(x: u32) f32 {
    return switch (x) {
        0 => 10,
        1 => 20,
        2 => 40,
        3 => 80,
        else => 0,
    };
}

/// A `switch` with side effects, which stays a `br_table`.
pub export fn pick_field(e: Entity, x: u32) void {
    switch (x) {
        0 => e.set(.health, 10),
        1 => e.set(.frags, 20),
        2 => e.set(.armorvalue, 40),
        3 => e.set(.currentammo, 80),
        else => e.set(.nextthink, defs.time.get()),
    }
}

/// A non-exported, non-inlined helper, called normally and as a tail call.
/// Internal functions taking `Entity`/`String` must use the C calling
/// convention: Zig's default passes structs by reference, and an externref
/// can't be stored to the stack.
noinline fn helper(e: Entity, amount: f32) callconv(.c) f32 {
    e.set(.health, e.get(.health) - amount);
    return e.get(.health);
}

/// The second call becomes a `return_call`. Not `@call(.always_tail, ...)`:
/// Zig 0.17.1 rejects it (`noinline` is part of the function type), and
/// without `noinline` it miscompiles the arguments.
pub export fn damage_twice(e: Entity, amount: f32) f32 {
    _ = helper(e, amount);
    return helper(e, amount * 2);
}

// ---- user globals and fields, function values ----

const activator = defs.userGlobal(Entity, "activator");
const counter = defs.userGlobal(f32, "counter");
const last_dir = defs.userGlobal(Vector, "last_dir");
const on_done = defs.userGlobal(fn () callconv(.c) void, "on_done");
const attack_finished = defs.userField(f32, "attack_finished");
const goal = defs.userField(Vector, "goal");
const th_pain = defs.userField(defs.Function, "th_pain");

fn bump() callconv(.c) void {
    counter.set(counter.get() + 1);
}

fn pain(attacker: Entity, damage: f32) callconv(.c) void {
    attacker.set(.health, attacker.get(.health) - damage);
}

/// User globals and fields, references to Zig functions, `find` with a field
/// reference, and function comparisons.
pub export fn user_defs(e: Entity) void {
    activator.set(e);
    last_dir.set(e.get(.velocity));
    e.set(attack_finished, defs.time.get() + 1);
    e.set(goal, last_dir.get());
    e.set(.think, defs.func(bump));
    e.set(th_pain, defs.func(pain));
    on_done.set(defs.func(bump));
    if (defs.fnNeq(e.get(.think), defs.func(helperThink))) e.set(.nextthink, defs.time.get());
    if (!defs.fnIsNull(e.get(th_pain))) defs.callAs(fn (Entity, f32) callconv(.c) void, e.get(th_pain))(activator.get(), 5);
    const spot = defs.find(defs.world.get(), defs.fieldRef(.classname), String.lit("info_player_start"));
    if (!spot.isNull()) e.set(.origin, spot.get(.origin));
}

fn helperThink() callconv(.c) void {}
