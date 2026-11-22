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
