//! QuakeC system globals, fields and builtins, named as in QuakeWorld's
//! `qw-qc/defs.qc`, plus the game definitions from that file.
//!
//! Each global lives at `slot * 4` in linear memory, which wasm-to-qc maps
//! back onto the QuakeC global with the same name.

const std = @import("std");

/// Tells the converter to use the QuakeWorld system defs.
pub export const __quakec_target: [10:0]u8 = "quakeworld".*;

// HACK: Workaround for some jankiness with LLVM.
comptime {
    asm (
        \\.globl __funcref_call_table
        \\.weak __funcref_call_table
        \\.tabletype __funcref_call_table, funcref, 1
        \\__funcref_call_table:
    );
}

/// The QuakeWorld system entity fields (`entvars_t`), named as in
/// `qw-qc/defs.qc`. Each value is the field's QuakeC field offset, which is
/// what the field intrinsics below pass to the converter. Vector fields take
/// up three slots; access their components through the `Vector` instead.
pub const Field = enum(u32) {
    modelindex = 0,
    absmin = 1,
    absmax = 4,
    ltime = 7,
    lastruntime = 8,
    movetype = 9,
    solid = 10,
    origin = 11,
    oldorigin = 14,
    velocity = 17,
    angles = 20,
    avelocity = 23,
    classname = 26,
    model = 27,
    frame = 28,
    skin = 29,
    effects = 30,
    mins = 31,
    maxs = 34,
    size = 37,
    touch = 40,
    use = 41,
    think = 42,
    blocked = 43,
    nextthink = 44,
    groundentity = 45,
    health = 46,
    frags = 47,
    weapon = 48,
    weaponmodel = 49,
    weaponframe = 50,
    currentammo = 51,
    ammo_shells = 52,
    ammo_nails = 53,
    ammo_rockets = 54,
    ammo_cells = 55,
    items = 56,
    takedamage = 57,
    chain = 58,
    deadflag = 59,
    view_ofs = 60,
    button0 = 63,
    button1 = 64,
    button2 = 65,
    impulse = 66,
    fixangle = 67,
    v_angle = 68,
    netname = 71,
    enemy = 72,
    flags = 73,
    colormap = 74,
    team = 75,
    max_health = 76,
    teleport_time = 77,
    armortype = 78,
    armorvalue = 79,
    waterlevel = 80,
    watertype = 81,
    ideal_yaw = 82,
    yaw_speed = 83,
    aiment = 84,
    goalentity = 85,
    spawnflags = 86,
    target = 87,
    targetname = 88,
    dmg_take = 89,
    dmg_save = 90,
    dmg_inflictor = 91,
    owner = 92,
    movedir = 93,
    message = 96,
    sounds = 97,
    noise = 98,
    noise1 = 99,
    noise2 = 100,
    noise3 = 101,

    pub fn Type(comptime this: @This()) type {
        return switch (this) {
            .modelindex, .ltime, .lastruntime, .movetype, .solid, .frame, .skin, .effects, .nextthink, .health, .frags, .weapon, .weaponframe, .currentammo, .ammo_shells, .ammo_nails, .ammo_rockets, .ammo_cells, .items, .takedamage, .deadflag, .button0, .button1, .button2, .impulse, .fixangle, .flags, .colormap, .team, .max_health, .teleport_time, .armortype, .armorvalue, .waterlevel, .watertype, .ideal_yaw, .yaw_speed, .spawnflags, .dmg_take, .dmg_save, .sounds => f32,
            .absmin, .absmax, .origin, .oldorigin, .velocity, .angles, .avelocity, .mins, .maxs, .size, .view_ofs, .v_angle, .movedir => Vector,
            .classname, .model, .weaponmodel, .netname, .target, .targetname, .message, .noise, .noise1, .noise2, .noise3 => String,
            .touch, .use, .think, .blocked => Function,
            .groundentity, .chain, .enemy, .aiment, .goalentity, .dmg_inflictor, .owner => Entity,
        };
    }
};

pub const Entity = extern struct {
    const Self = @This();

    ref: EntityRef,

    // `inline` so that `field` reaches the intrinsic as a constant; the
    // converter needs it to be a compile-time constant. `field` is a `Field`
    // (e.g. `.health`) or a `UserField` (see `userField`).
    pub inline fn get(this: Self, comptime field: anytype) FieldType(field) {
        const offset = fieldOffset(field);
        return switch (FieldType(field)) {
            f32 => __quakec_load_entity_field_float(this.ref, offset),
            Vector => __quakec_load_entity_field_vector(this.ref, offset),
            String => .{ .ref = __quakec_load_entity_field_string(this.ref, offset) },
            Entity => .{ .ref = __quakec_load_entity_field_entity(this.ref, offset) },
            Function => __quakec_load_entity_field_fn(this.ref, offset),
            else => @compileError("unsupported field type " ++ @typeName(FieldType(field))),
        };
    }

    pub inline fn set(this: Self, comptime field: anytype, value: FieldType(field)) void {
        const offset = fieldOffset(field);
        switch (FieldType(field)) {
            f32 => __quakec_store_entity_field_float(this.ref, offset, value),
            Vector => __quakec_store_entity_field_vector(this.ref, offset, value),
            String => __quakec_store_entity_field_string(this.ref, offset, value.ref),
            Entity => __quakec_store_entity_field_entity(this.ref, offset, value.ref),
            Function => __quakec_store_entity_field_fn(this.ref, offset, value),
            else => @compileError("unsupported field type " ++ @typeName(FieldType(field))),
        }
    }

    /// `a == b` (`EQ_E`).
    pub inline fn eql(a: Self, b: Self) bool {
        return __quakec_eq_entity(a.ref, b.ref);
    }

    /// `a != b` (`NE_E`).
    pub inline fn neq(a: Self, b: Self) bool {
        return __quakec_ne_entity(a.ref, b.ref);
    }

    /// `!e` (`NOT_ENT`): whether this is `world`/null.
    pub inline fn isNull(this: Self) bool {
        return __quakec_not_entity(this.ref);
    }
};
pub const String = extern struct {
    const Self = @This();

    ref: StringRef,

    /// `a == b` (`EQ_S`), comparing contents.
    pub inline fn eql(a: Self, b: Self) bool {
        return __quakec_eq_string(a.ref, b.ref);
    }

    /// `a != b` (`NE_S`), comparing contents.
    pub inline fn neq(a: Self, b: Self) bool {
        return __quakec_ne_string(a.ref, b.ref);
    }

    /// `!s` (`NOT_S`): whether this is null or empty.
    pub inline fn isNull(this: Self) bool {
        return __quakec_not_string(this.ref);
    }

    /// A string literal, e.g. `String.lit("progs/player.mdl")`. The bytes
    /// are placed in static data and the converter emits a QuakeC `"..."`.
    pub inline fn lit(comptime s: []const u8) String {
        if (!NoNul(s).ok) @compileError("QuakeC strings can't contain NUL bytes");
        return .{ .ref = __quakec_string_literal(s.ptr, s.len) };
    }
};

/// Checked in a container so that the check gets its own branch quota, rather than
/// adding to that of the function using the literal.
fn NoNul(comptime s: []const u8) type {
    return struct {
        const ok = blk: {
            @setEvalBranchQuota(s.len * 4 + 1000);
            break :blk std.mem.indexOfScalar(u8, s, 0) == null;
        };
    };
}

const EntityRef = *addrspace(.externref) const struct {};
const StringRef = *addrspace(.externref) const struct {};

comptime {
    if (Entity == String or EntityRef == StringRef) unreachable;
}

/// A QuakeC `vector`. With simd128 enabled this is passed as a `v128`.
pub const Vector = @Vector(3, f32);

/// A QuakeC field reference of type `.T` (e.g. `EntityField(String)` is `.string`).
pub fn EntityField(comptime T: type) type {
    return *addrspace(.externref) const struct {
        pub const Type = T;
    };
}

/// A QuakeC function value (`void()`), as stored in globals and fields.
pub const Function = *addrspace(.funcref) const fn () callconv(.c) void;

/// A user-defined entity field `.T name`, which the converter declares. See
/// `userField`.
pub fn UserField(comptime T: type) type {
    return struct {
        pub const Type = T;
        name: [*:0]const u8,
    };
}

/// Declare a user entity field, e.g.
/// `pub const attack_finished = defs.userField(f32, "attack_finished");`,
/// then use it like a system field: `e.get(attack_finished)`.
pub fn userField(comptime T: type, comptime name: [:0]const u8) UserField(T) {
    return .{ .name = name.ptr };
}

fn isSysField(comptime F: type) bool {
    return F == Field or F == @TypeOf(.enum_literal);
}

/// The type of a system or user field.
pub fn FieldType(comptime field: anytype) type {
    return if (comptime isSysField(@TypeOf(field))) @as(Field, field).Type() else @TypeOf(field).Type;
}

/// System fields are passed to the intrinsics as their offset, user fields
/// as the address of their name in static data.
inline fn fieldOffset(comptime field: anytype) u32 {
    return if (comptime isSysField(@TypeOf(field))) @backingInt(@as(Field, field)) else @intFromPtr(field.name);
}

/// A reference to a `.string` field, as passed to `find`.
pub inline fn fieldRef(comptime field: anytype) EntityField(String) {
    if (FieldType(field) != String) @compileError("only `.string` field references are supported");
    return __quakec_field_ref_string(fieldOffset(field));
}

/// A Zig function as a QuakeC function value, e.g.
/// `e.set(.think, defs.func(SUB_Remove))`. The function must use the C
/// calling convention. Functions taking parameters can be stored too, and
/// called with `callAs`.
pub inline fn func(comptime f: anytype) Function {
    return __quakec_func_ref(@ptrCast(&f));
}

/// View a function value as a function of type `F` (e.g.
/// `fn (Entity, f32) callconv(.c) void`) so that it can be called.
pub inline fn callAs(comptime F: type, f: Function) *addrspace(.funcref) const F {
    return @ptrCast(f);
}

/// `a == b` (`EQ_FNC`).
pub inline fn fnEql(a: Function, b: Function) bool {
    return __quakec_eq_fn(a, b);
}

/// `a != b` (`NE_FNC`).
pub inline fn fnNeq(a: Function, b: Function) bool {
    return __quakec_ne_fn(a, b);
}

/// `!f` (`NOT_FNC`): whether this is the null function.
pub inline fn fnIsNull(f: Function) bool {
    return __quakec_not_fn(f);
}

/// `a == b` on vectors (`EQ_V`). Written as an intrinsic so that LLVM can't
/// turn it into SIMD comparisons QuakeC doesn't have.
pub inline fn vecEql(a: Vector, b: Vector) bool {
    return __quakec_eq_vector(a, b);
}

/// `a != b` on vectors (`NE_V`).
pub inline fn vecNeq(a: Vector, b: Vector) bool {
    return __quakec_ne_vector(a, b);
}

/// `!v` (`NOT_V`): whether this is `'0 0 0'`.
pub inline fn vecIsZero(v: Vector) bool {
    return __quakec_not_vector(v);
}

/// `a * b` on vectors (`MUL_V`): the dot product.
pub inline fn vecDot(a: Vector, b: Vector) f32 {
    return __quakec_dot_vector(a, b);
}

extern fn __quakec_eq_vector(a: Vector, b: Vector) bool;
extern fn __quakec_ne_vector(a: Vector, b: Vector) bool;
extern fn __quakec_not_vector(v: Vector) bool;
extern fn __quakec_dot_vector(a: Vector, b: Vector) f32;

extern fn __quakec_field_ref_string(field: u32) EntityField(String);
extern fn __quakec_func_ref(f: *const anyopaque) Function;
extern fn __quakec_eq_fn(a: Function, b: Function) bool;
extern fn __quakec_ne_fn(a: Function, b: Function) bool;
extern fn __quakec_not_fn(f: Function) bool;

// Entity field access. `field` is the field's offset (see `Field`), and must
// be a compile-time constant. There is one intrinsic per type so the
// converter can emit the correctly-typed load/store instruction.
extern fn __quakec_load_entity_field_float(ent: EntityRef, field: u32) f32;
extern fn __quakec_load_entity_field_vector(ent: EntityRef, field: u32) Vector;
extern fn __quakec_load_entity_field_string(ent: EntityRef, field: u32) StringRef;
extern fn __quakec_load_entity_field_entity(ent: EntityRef, field: u32) EntityRef;
extern fn __quakec_load_entity_field_fn(ent: EntityRef, field: u32) Function;

extern fn __quakec_store_entity_field_float(ent: EntityRef, field: u32, value: f32) void;
extern fn __quakec_store_entity_field_vector(ent: EntityRef, field: u32, value: Vector) void;
extern fn __quakec_store_entity_field_string(ent: EntityRef, field: u32, value: StringRef) void;
extern fn __quakec_store_entity_field_entity(ent: EntityRef, field: u32, value: EntityRef) void;
extern fn __quakec_store_entity_field_fn(ent: EntityRef, field: u32, value: Function) void;

// Comparisons. Wasm can't compare `externref`s itself, and QuakeC needs the
// type to pick the right opcode.
extern fn __quakec_eq_entity(a: EntityRef, b: EntityRef) bool;
extern fn __quakec_ne_entity(a: EntityRef, b: EntityRef) bool;
extern fn __quakec_not_entity(e: EntityRef) bool;
extern fn __quakec_eq_string(a: StringRef, b: StringRef) bool;
extern fn __quakec_ne_string(a: StringRef, b: StringRef) bool;
extern fn __quakec_not_string(s: StringRef) bool;

// A string literal. `ptr` must be a constant address in static data and `len`
// a constant; the converter reads the bytes and emits a QuakeC literal.
extern fn __quakec_string_literal(ptr: [*]const u8, len: usize) StringRef;

// Globals. `ptr` is `slot * 4` for system globals, or the address of the
// name of a user global (see `userGlobal`).
extern fn __quakec_load_float(ptr: *const anyopaque) f32;
extern fn __quakec_load_vector(ptr: *const anyopaque) Vector;
extern fn __quakec_load_entity(ptr: *const anyopaque) callconv(.{ .wasm_mvp = .{} }) EntityRef;
extern fn __quakec_load_string(ptr: *const anyopaque) StringRef;
extern fn __quakec_load_fn(ptr: *const anyopaque) Function;

extern fn __quakec_store_float(ptr: *const anyopaque, value: f32) void;
extern fn __quakec_store_vector(ptr: *const anyopaque, value: Vector) void;
extern fn __quakec_store_entity(ptr: *const anyopaque, ent: EntityRef) void;
extern fn __quakec_store_string(ptr: *const anyopaque, ent: StringRef) void;
extern fn __quakec_store_fn(ptr: *const anyopaque, value: Function) void;

/// A QuakeC global of type `T`: `f32` (or an integer or `bool`, stored as a
/// float), `Vector`, `Entity`, `String`, or a function type.
pub fn Global(comptime T: type) type {
    const Ty = switch (@typeInfo(T)) {
        .@"fn" => *addrspace(.funcref) const T,
        else => T,
    };

    return struct {
        const Self = @This();

        ptr: *const anyopaque,

        pub inline fn get(comptime this: Self) Ty {
            return switch (@typeInfo(T)) {
                .int => @intFromFloat(__quakec_load_float(this.ptr)),
                .float => @as(T, __quakec_load_float(this.ptr)),
                .bool => __quakec_load_float(this.ptr) != 0,
                .@"fn" => @ptrCast(__quakec_load_fn(this.ptr)),
                else => if (T == Entity)
                    // For some reason, trying to generate `__quakec_load_entity` as
                    // an extern fn causes LLVM to crash.
                    .{ .ref = @panic("`__quakec_load_entity` broken") }
                else if (T == String)
                    .{ .ref = __quakec_load_string(this.ptr) }
                else if (T == Vector)
                    __quakec_load_vector(this.ptr)
                else
                    @compileError("unsupported global type " ++ @typeName(T)),
            };
        }

        pub inline fn set(comptime this: Self, value: Ty) void {
            switch (@typeInfo(T)) {
                .int => __quakec_store_float(this.ptr, @floatFromInt(value)),
                .float => __quakec_store_float(this.ptr, value),
                .bool => __quakec_store_float(this.ptr, if (value) 1 else 0),
                .@"fn" => __quakec_store_fn(this.ptr, @ptrCast(value)),
                else => if (T == Entity)
                    __quakec_store_entity(this.ptr, value.ref)
                else if (T == String)
                    __quakec_store_string(this.ptr, value.ref)
                else if (T == Vector)
                    __quakec_store_vector(this.ptr, value)
                else
                    @compileError("unsupported global type " ++ @typeName(T)),
            }
        }
    };
}

/// A system global at `slot`.
pub fn global(comptime T: type, comptime slot: comptime_int) Global(T) {
    return .{ .ptr = @ptrFromInt(slot * 4) };
}

/// Declare a user global, e.g.
/// `pub const activator = defs.userGlobal(Entity, "activator");`. The
/// converter declares it in the QuakeC output. Zig `var`s can't be used.
pub fn userGlobal(comptime T: type, comptime name: [:0]const u8) Global(T) {
    return .{ .ptr = name.ptr };
}

// ---- system globals (QuakeWorld `globalvars_t`) ----

pub const self = global(Entity, 28);
pub const other = global(Entity, 29);
pub const world = global(Entity, 30);
pub const time = global(f32, 31);
pub const frametime = global(f32, 32);
pub const newmis = global(Entity, 33);
pub const force_retouch = global(f32, 34);
pub const mapname = global(String, 35);
pub const serverflags = global(f32, 36);
pub const total_secrets = global(f32, 37);
pub const total_monsters = global(f32, 38);
pub const found_secrets = global(f32, 39);
pub const killed_monsters = global(f32, 40);
pub const parm1 = global(f32, 41);
pub const parm2 = global(f32, 42);
pub const parm3 = global(f32, 43);
pub const parm4 = global(f32, 44);
pub const parm5 = global(f32, 45);
pub const parm6 = global(f32, 46);
pub const parm7 = global(f32, 47);
pub const parm8 = global(f32, 48);
pub const parm9 = global(f32, 49);
pub const parm10 = global(f32, 50);
pub const parm11 = global(f32, 51);
pub const parm12 = global(f32, 52);
pub const parm13 = global(f32, 53);
pub const parm14 = global(f32, 54);
pub const parm15 = global(f32, 55);
pub const parm16 = global(f32, 56);
pub const v_forward = global(Vector, 57);
pub const v_forward_x = global(f32, 57);
pub const v_forward_y = global(f32, 58);
pub const v_forward_z = global(f32, 59);
pub const v_up = global(Vector, 60);
pub const v_up_x = global(f32, 60);
pub const v_up_y = global(f32, 61);
pub const v_up_z = global(f32, 62);
pub const v_right = global(Vector, 63);
pub const v_right_x = global(f32, 63);
pub const v_right_y = global(f32, 64);
pub const v_right_z = global(f32, 65);
pub const trace_allsolid = global(f32, 66);
pub const trace_startsolid = global(f32, 67);
pub const trace_fraction = global(f32, 68);
pub const trace_endpos = global(Vector, 69);
pub const trace_endpos_x = global(f32, 69);
pub const trace_endpos_y = global(f32, 70);
pub const trace_endpos_z = global(f32, 71);
pub const trace_plane_normal = global(Vector, 72);
pub const trace_plane_normal_x = global(f32, 72);
pub const trace_plane_normal_y = global(f32, 73);
pub const trace_plane_normal_z = global(f32, 74);
pub const trace_plane_dist = global(f32, 75);
pub const trace_ent = global(Entity, 76);
pub const trace_inopen = global(f32, 77);
pub const trace_inwater = global(f32, 78);
pub const msg_entity = global(Entity, 79);
pub const main = global(fn () callconv(.c) void, 80);
pub const StartFrame = global(fn () callconv(.c) void, 81);
pub const PlayerPreThink = global(fn () callconv(.c) void, 82);
pub const PlayerPostThink = global(fn () callconv(.c) void, 83);
pub const ClientKill = global(fn () callconv(.c) void, 84);
pub const ClientConnect = global(fn () callconv(.c) void, 85);
pub const PutClientInServer = global(fn () callconv(.c) void, 86);
pub const ClientDisconnect = global(fn () callconv(.c) void, 87);
pub const SetNewParms = global(fn () callconv(.c) void, 88);
pub const SetChangeParms = global(fn () callconv(.c) void, 89);

// ---- builtins (QuakeWorld) ----

pub extern "builtin" fn makevectors(ang: Vector) void; // #1
pub extern "builtin" fn setorigin(e: Entity, o: Vector) void; // #2
pub extern "builtin" fn setmodel(e: Entity, m: String) void; // #3
pub extern "builtin" fn setsize(e: Entity, min: Vector, max: Vector) void; // #4
pub extern "builtin" fn @"break"() void; // #6
pub extern "builtin" fn random() f32; // #7
pub extern "builtin" fn sound(e: Entity, chan: f32, samp: String, vol: f32, atten: f32) void; // #8
pub extern "builtin" fn normalize(v: Vector) Vector; // #9
pub extern "builtin" fn @"error"(e: String) void; // #10
pub extern "builtin" fn objerror(e: String) void; // #11
pub extern "builtin" fn vlen(v: Vector) f32; // #12
pub extern "builtin" fn vectoyaw(v: Vector) f32; // #13
pub extern "builtin" fn spawn() Entity; // #14
pub extern "builtin" fn remove(e: Entity) void; // #15
pub extern "builtin" fn traceline(v1: Vector, v2: Vector, nomonsters: f32, forent: Entity) void; // #16
pub extern "builtin" fn checkclient() Entity; // #17
pub extern "builtin" fn find(start: Entity, fld: EntityField(String), match: String) Entity; // #18
pub extern "builtin" fn precache_sound(s: String) String; // #19
pub extern "builtin" fn precache_model(s: String) String; // #20
pub extern "builtin" fn stuffcmd(client: Entity, s: String) void; // #21
pub extern "builtin" fn findradius(org: Vector, rad: f32) Entity; // #22
pub extern "builtin" fn bprint(level: f32, s: String) void; // #23
pub extern "builtin" fn sprint(client: Entity, level: f32, s: String) void; // #24
pub extern "builtin" fn dprint(s: String) void; // #25
pub extern "builtin" fn ftos(f: f32) String; // #26
pub extern "builtin" fn vtos(v: Vector) String; // #27
pub extern "builtin" fn coredump() void; // #28
pub extern "builtin" fn traceon() void; // #29
pub extern "builtin" fn traceoff() void; // #30
pub extern "builtin" fn eprint(e: Entity) void; // #31
pub extern "builtin" fn walkmove(yaw: f32, dist: f32) f32; // #32
pub extern "builtin" fn droptofloor() f32; // #34
pub extern "builtin" fn lightstyle(style: f32, value: String) void; // #35
pub extern "builtin" fn rint(v: f32) f32; // #36
pub extern "builtin" fn floor(v: f32) f32; // #37
pub extern "builtin" fn ceil(v: f32) f32; // #38
pub extern "builtin" fn checkbottom(e: Entity) f32; // #40
pub extern "builtin" fn pointcontents(v: Vector) f32; // #41
pub extern "builtin" fn fabs(f: f32) f32; // #43
pub extern "builtin" fn aim(e: Entity, speed: f32) Vector; // #44
pub extern "builtin" fn cvar(s: String) f32; // #45
pub extern "builtin" fn localcmd(s: String) void; // #46
pub extern "builtin" fn nextent(e: Entity) Entity; // #47
pub extern "builtin" fn ChangeYaw() void; // #49
pub extern "builtin" fn vectoangles(v: Vector) Vector; // #51
pub extern "builtin" fn WriteByte(to: f32, f: f32) void; // #52
pub extern "builtin" fn WriteChar(to: f32, f: f32) void; // #53
pub extern "builtin" fn WriteShort(to: f32, f: f32) void; // #54
pub extern "builtin" fn WriteLong(to: f32, f: f32) void; // #55
pub extern "builtin" fn WriteCoord(to: f32, f: f32) void; // #56
pub extern "builtin" fn WriteAngle(to: f32, f: f32) void; // #57
pub extern "builtin" fn WriteString(to: f32, s: String) void; // #58
pub extern "builtin" fn WriteEntity(to: f32, s: Entity) void; // #59
pub extern "builtin" fn movetogoal(step: f32) void; // #67
pub extern "builtin" fn precache_file(s: String) String; // #68
pub extern "builtin" fn makestatic(e: Entity) void; // #69
pub extern "builtin" fn changelevel(s: String) void; // #70
pub extern "builtin" fn cvar_set(@"var": String, val: String) void; // #72
pub extern "builtin" fn centerprint(client: Entity, s: String) void; // #73
pub extern "builtin" fn ambientsound(pos: Vector, samp: String, vol: f32, atten: f32) void; // #74
pub extern "builtin" fn precache_model2(s: String) String; // #75
pub extern "builtin" fn precache_sound2(s: String) String; // #76
pub extern "builtin" fn precache_file2(s: String) String; // #77
pub extern "builtin" fn setspawnparms(e: Entity) void; // #78
pub extern "builtin" fn logfrag(killer: Entity, killee: Entity) void; // #79
pub extern "builtin" fn infokey(e: Entity, key: String) String; // #80
pub extern "builtin" fn stof(s: String) f32; // #81
pub extern "builtin" fn multicast(where: Vector, set: f32) void; // #82

// ============================================================================
//
// VARS NOT REFERENCED BY C CODE
//
// (The rest of `qw-qc/defs.qc`: game constants, globals and fields.)
//
// ============================================================================

pub const FALSE: f32 = 0;
pub const TRUE: f32 = 1;

// edict.flags
pub const FL_FLY: f32 = 1;
pub const FL_SWIM: f32 = 2;
pub const FL_CLIENT: f32 = 8; // set for all client edicts
pub const FL_INWATER: f32 = 16; // for enter / leave water splash
pub const FL_MONSTER: f32 = 32;
pub const FL_GODMODE: f32 = 64; // player cheat
pub const FL_NOTARGET: f32 = 128; // player cheat
pub const FL_ITEM: f32 = 256; // extra wide size for bonus items
pub const FL_ONGROUND: f32 = 512; // standing on something
pub const FL_PARTIALGROUND: f32 = 1024; // not all corners are valid
pub const FL_WATERJUMP: f32 = 2048; // player jumping out of water
pub const FL_JUMPRELEASED: f32 = 4096; // for jump debouncing

// edict.movetype values
pub const MOVETYPE_NONE: f32 = 0; // never moves
pub const MOVETYPE_WALK: f32 = 3; // players only
pub const MOVETYPE_STEP: f32 = 4; // discrete, not real time unless fall
pub const MOVETYPE_FLY: f32 = 5;
pub const MOVETYPE_TOSS: f32 = 6; // gravity
pub const MOVETYPE_PUSH: f32 = 7; // no clip to world, push and crush
pub const MOVETYPE_NOCLIP: f32 = 8;
pub const MOVETYPE_FLYMISSILE: f32 = 9; // fly with extra size against monsters
pub const MOVETYPE_BOUNCE: f32 = 10;
pub const MOVETYPE_BOUNCEMISSILE: f32 = 11; // bounce with extra size

// edict.solid values
pub const SOLID_NOT: f32 = 0; // no interaction with other objects
pub const SOLID_TRIGGER: f32 = 1; // touch on edge, but not blocking
pub const SOLID_BBOX: f32 = 2; // touch on edge, block
pub const SOLID_SLIDEBOX: f32 = 3; // touch on edge, but not an onground
pub const SOLID_BSP: f32 = 4; // bsp clip, touch on edge, block

// range values
pub const RANGE_MELEE: f32 = 0;
pub const RANGE_NEAR: f32 = 1;
pub const RANGE_MID: f32 = 2;
pub const RANGE_FAR: f32 = 3;

// deadflag values
pub const DEAD_NO: f32 = 0;
pub const DEAD_DYING: f32 = 1;
pub const DEAD_DEAD: f32 = 2;
pub const DEAD_RESPAWNABLE: f32 = 3;

// takedamage values
pub const DAMAGE_NO: f32 = 0;
pub const DAMAGE_YES: f32 = 1;
pub const DAMAGE_AIM: f32 = 2;

// items
pub const IT_AXE: f32 = 4096;
pub const IT_SHOTGUN: f32 = 1;
pub const IT_SUPER_SHOTGUN: f32 = 2;
pub const IT_NAILGUN: f32 = 4;
pub const IT_SUPER_NAILGUN: f32 = 8;
pub const IT_GRENADE_LAUNCHER: f32 = 16;
pub const IT_ROCKET_LAUNCHER: f32 = 32;
pub const IT_LIGHTNING: f32 = 64;
pub const IT_EXTRA_WEAPON: f32 = 128;

pub const IT_SHELLS: f32 = 256;
pub const IT_NAILS: f32 = 512;
pub const IT_ROCKETS: f32 = 1024;
pub const IT_CELLS: f32 = 2048;

pub const IT_ARMOR1: f32 = 8192;
pub const IT_ARMOR2: f32 = 16384;
pub const IT_ARMOR3: f32 = 32768;
pub const IT_SUPERHEALTH: f32 = 65536;

pub const IT_KEY1: f32 = 131072;
pub const IT_KEY2: f32 = 262144;

pub const IT_INVISIBILITY: f32 = 524288;
pub const IT_INVULNERABILITY: f32 = 1048576;
pub const IT_SUIT: f32 = 2097152;
pub const IT_QUAD: f32 = 4194304;

// point content values
pub const CONTENT_EMPTY: f32 = -1;
pub const CONTENT_SOLID: f32 = -2;
pub const CONTENT_WATER: f32 = -3;
pub const CONTENT_SLIME: f32 = -4;
pub const CONTENT_LAVA: f32 = -5;
pub const CONTENT_SKY: f32 = -6;

pub const STATE_TOP: f32 = 0;
pub const STATE_BOTTOM: f32 = 1;
pub const STATE_UP: f32 = 2;
pub const STATE_DOWN: f32 = 3;

pub const VEC_ORIGIN: Vector = .{ 0, 0, 0 };
pub const VEC_HULL_MIN: Vector = .{ -16, -16, -24 };
pub const VEC_HULL_MAX: Vector = .{ 16, 16, 32 };

pub const VEC_HULL2_MIN: Vector = .{ -32, -32, -24 };
pub const VEC_HULL2_MAX: Vector = .{ 32, 32, 64 };

// protocol bytes
pub const SVC_TEMPENTITY: f32 = 23;
pub const SVC_KILLEDMONSTER: f32 = 27;
pub const SVC_FOUNDSECRET: f32 = 28;
pub const SVC_INTERMISSION: f32 = 30;
pub const SVC_FINALE: f32 = 31;
pub const SVC_CDTRACK: f32 = 32;
pub const SVC_SELLSCREEN: f32 = 33;
pub const SVC_SMALLKICK: f32 = 34;
pub const SVC_BIGKICK: f32 = 35;
pub const SVC_MUZZLEFLASH: f32 = 39;

pub const TE_SPIKE: f32 = 0;
pub const TE_SUPERSPIKE: f32 = 1;
pub const TE_GUNSHOT: f32 = 2;
pub const TE_EXPLOSION: f32 = 3;
pub const TE_TAREXPLOSION: f32 = 4;
pub const TE_LIGHTNING1: f32 = 5;
pub const TE_LIGHTNING2: f32 = 6;
pub const TE_WIZSPIKE: f32 = 7;
pub const TE_KNIGHTSPIKE: f32 = 8;
pub const TE_LIGHTNING3: f32 = 9;
pub const TE_LAVASPLASH: f32 = 10;
pub const TE_TELEPORT: f32 = 11;
pub const TE_BLOOD: f32 = 12;
pub const TE_LIGHTNINGBLOOD: f32 = 13;

// sound channels
// channel 0 never willingly overrides
// other channels (1-7) allways override a playing sound on that channel
pub const CHAN_AUTO: f32 = 0;
pub const CHAN_WEAPON: f32 = 1;
pub const CHAN_VOICE: f32 = 2;
pub const CHAN_ITEM: f32 = 3;
pub const CHAN_BODY: f32 = 4;
pub const CHAN_NO_PHS_ADD: f32 = 8; // ie: CHAN_BODY+CHAN_NO_PHS_ADD

pub const ATTN_NONE: f32 = 0;
pub const ATTN_NORM: f32 = 1;
pub const ATTN_IDLE: f32 = 2;
pub const ATTN_STATIC: f32 = 3;

// update types
pub const UPDATE_GENERAL: f32 = 0;
pub const UPDATE_STATIC: f32 = 1;
pub const UPDATE_BINARY: f32 = 2;
pub const UPDATE_TEMP: f32 = 3;

// entity effects
pub const EF_BRIGHTLIGHT: f32 = 4;
pub const EF_DIMLIGHT: f32 = 8;
pub const EF_FLAG1: f32 = 16;
pub const EF_FLAG2: f32 = 32;
// GLQuakeWorld Stuff
pub const EF_BLUE: f32 = 64; // Blue Globe effect for Quad
pub const EF_RED: f32 = 128; // Red Globe effect for Pentagram

// messages
pub const MSG_BROADCAST: f32 = 0; // unreliable to all
pub const MSG_ONE: f32 = 1; // reliable to one (msg_entity)
pub const MSG_ALL: f32 = 2; // reliable to all
pub const MSG_INIT: f32 = 3; // write to the init string
pub const MSG_MULTICAST: f32 = 4; // for multicast() call

// message levels
pub const PRINT_LOW: f32 = 0; // pickup messages
pub const PRINT_MEDIUM: f32 = 1; // death messages
pub const PRINT_HIGH: f32 = 2; // critical messages
pub const PRINT_CHAT: f32 = 3; // also goes to chat console

// multicast sets
pub const MULTICAST_ALL: f32 = 0; // every client
pub const MULTICAST_PHS: f32 = 1; // within hearing
pub const MULTICAST_PVS: f32 = 2; // within sight
pub const MULTICAST_ALL_R: f32 = 3; // every client, reliable
pub const MULTICAST_PHS_R: f32 = 4; // within hearing, reliable
pub const MULTICAST_PVS_R: f32 = 5; // within sight, reliable

//================================================

//
// globals
//
pub const movedist = userGlobal(f32, "movedist");

pub const string_null = userGlobal(String, "string_null"); // null string, nothing should be held here
pub const empty_float = userGlobal(f32, "empty_float");

pub const activator = userGlobal(Entity, "activator"); // the entity that activated a trigger or brush

pub const damage_attacker = userGlobal(Entity, "damage_attacker"); // set by T_Damage
pub const damage_inflictor = userGlobal(Entity, "damage_inflictor");
pub const framecount = userGlobal(f32, "framecount");

//
// cvars checked each frame
//
pub const teamplay = userGlobal(f32, "teamplay");
pub const timelimit = userGlobal(f32, "timelimit");
pub const fraglimit = userGlobal(f32, "fraglimit");
pub const deathmatch = userGlobal(f32, "deathmatch");
// `float rj = 1;` in the original. Zig globals can't have initial values in
// QuakeC, so `worldspawn` sets it (progs globals are reset on every map load,
// just before `worldspawn` runs).
pub const rj = userGlobal(f32, "rj");

//================================================

//
// world fields (FIXME: make globals)
//
pub const wad = userField(String, "wad");
pub const map = userField(String, "map");
pub const worldtype = userField(f32, "worldtype"); // 0=medieval 1=metal 2=base

//================================================

pub const killtarget = userField(String, "killtarget");

//
// quakeed fields
//
pub const light_lev = userField(f32, "light_lev"); // not used by game, but parsed by light util
pub const style = userField(f32, "style");

//
// monster ai
//
pub const th_stand = userField(Function, "th_stand");
pub const th_walk = userField(Function, "th_walk");
pub const th_run = userField(Function, "th_run");
pub const th_missile = userField(Function, "th_missile");
pub const th_melee = userField(Function, "th_melee");
/// `.void(entity attacker, float damage) th_pain`; call with `callAs(ThPain, ...)`.
pub const th_pain = userField(Function, "th_pain");
pub const ThPain = fn (Entity, f32) callconv(.c) void;
pub const th_die = userField(Function, "th_die");

pub const oldenemy = userField(Entity, "oldenemy"); // mad at this player before taking damage

pub const speed = userField(f32, "speed");

pub const lefty = userField(f32, "lefty");

pub const search_time = userField(f32, "search_time");
pub const attack_state = userField(f32, "attack_state");

pub const AS_STRAIGHT: f32 = 1;
pub const AS_SLIDING: f32 = 2;
pub const AS_MELEE: f32 = 3;
pub const AS_MISSILE: f32 = 4;

//
// player only fields
//
pub const voided = userField(f32, "voided");
pub const walkframe = userField(f32, "walkframe");

// Zoid Additions
pub const maxspeed = userField(f32, "maxspeed"); // Used to set Maxspeed on a player
pub const gravity = userField(f32, "gravity"); // Gravity Multiplier (0 to 1.0)

pub const attack_finished = userField(f32, "attack_finished");
pub const pain_finished = userField(f32, "pain_finished");

pub const invincible_finished = userField(f32, "invincible_finished");
pub const invisible_finished = userField(f32, "invisible_finished");
pub const super_damage_finished = userField(f32, "super_damage_finished");
pub const radsuit_finished = userField(f32, "radsuit_finished");

pub const invincible_time = userField(f32, "invincible_time");
pub const invincible_sound = userField(f32, "invincible_sound");
pub const invisible_time = userField(f32, "invisible_time");
pub const invisible_sound = userField(f32, "invisible_sound");
pub const super_time = userField(f32, "super_time");
pub const super_sound = userField(f32, "super_sound");
pub const rad_time = userField(f32, "rad_time");
pub const fly_sound = userField(f32, "fly_sound");

pub const axhitme = userField(f32, "axhitme");

pub const show_hostile = userField(f32, "show_hostile"); // set to time+0.2 whenever a client fires a
// weapon or takes damage.  Used to alert
// monsters that otherwise would let the player go
pub const jump_flag = userField(f32, "jump_flag"); // player jump flag
pub const swim_flag = userField(f32, "swim_flag"); // player swimming sound flag
pub const air_finished = userField(f32, "air_finished"); // when time > air_finished, start drowning
pub const bubble_count = userField(f32, "bubble_count"); // keeps track of the number of bubbles
pub const deathtype = userField(String, "deathtype"); // keeps track of how the player died

//
// object stuff
//
pub const mdl = userField(String, "mdl");
pub const mangle = userField(Vector, "mangle"); // angle at start

// `.vector oldorigin` is redeclared here in the original ("only used by
// secret door"); it is the system field `.oldorigin`.

pub const t_length = userField(f32, "t_length");
pub const t_width = userField(f32, "t_width");

//
// doors, etc
//
pub const dest = userField(Vector, "dest");
pub const dest1 = userField(Vector, "dest1");
pub const dest2 = userField(Vector, "dest2");
pub const wait = userField(f32, "wait"); // time from firing to restarting
pub const delay = userField(f32, "delay"); // time from activation to firing
pub const trigger_field = userField(Entity, "trigger_field"); // door's trigger entity
pub const noise4 = userField(String, "noise4");

//
// monsters
//
pub const pausetime = userField(f32, "pausetime");
pub const movetarget = userField(Entity, "movetarget");

//
// doors
//
pub const aflag = userField(f32, "aflag");
pub const dmg = userField(f32, "dmg"); // damage done by door when hit

//
// misc
//
pub const cnt = userField(f32, "cnt"); // misc flag

//
// subs
//
pub const think1 = userField(Function, "think1");
pub const finaldest = userField(Vector, "finaldest");
pub const finalangle = userField(Vector, "finalangle");

//
// triggers
//
pub const count = userField(f32, "count"); // for counting triggers

//
// plats / doors / buttons
//
pub const lip = userField(f32, "lip");
pub const state = userField(f32, "state");
pub const pos1 = userField(Vector, "pos1");
pub const pos2 = userField(Vector, "pos2"); // top and bottom positions
pub const height = userField(f32, "height");

//
// sounds
//
pub const waitmin = userField(f32, "waitmin");
pub const waitmax = userField(f32, "waitmax");
pub const distance = userField(f32, "distance");
pub const volume = userField(f32, "volume");

// (The function prototypes at the end of `defs.qc` aren't needed in Zig.)

// ============================================================================
//
// Helpers for QuakeC idioms, used by the port.
//
// ============================================================================

/// A string literal.
pub const S = String.lit;

/// `a & b` on QuakeC floats (`OP_BITAND`).
pub inline fn band(a: f32, b: f32) f32 {
    return @floatFromInt(@as(i32, @intFromFloat(a)) & @as(i32, @intFromFloat(b)));
}

/// `a | b` on QuakeC floats (`OP_BITOR`).
pub inline fn bor(a: f32, b: f32) f32 {
    return @floatFromInt(@as(i32, @intFromFloat(a)) | @as(i32, @intFromFloat(b)));
}

/// Whether any of the bits of `b` are set in `a`: `if (a & b)`.
pub inline fn has(a: f32, b: f32) bool {
    return band(a, b) != 0;
}

/// A vector `'x y z'`.
pub inline fn vec(x: f32, y: f32, z: f32) Vector {
    return .{ x, y, z };
}

/// `v * f` for a vector `v` and a float `f`.
pub inline fn scale(v: Vector, f: f32) Vector {
    return v * @as(Vector, @splat(f));
}

/// `a * b` for two vectors: the dot product.
pub inline fn dot(a: Vector, b: Vector) f32 {
    return vecDot(a, b);
}

/// `a == b` for vectors.
pub inline fn veq(a: Vector, b: Vector) bool {
    return vecEql(a, b);
}

/// `if (v)` for a vector.
pub inline fn vtrue(v: Vector) bool {
    return !vecIsZero(v);
}

/// `self`, as an entity.
pub inline fn me() Entity {
    return self.get();
}

/// The `[frame, nextthink]` prefix of a QuakeC state function:
/// `void() f = [$frame, next] { ... };`.
pub inline fn setState(frame: f32, next: Function) void {
    const e = self.get();
    e.set(.frame, frame);
    e.set(.nextthink, time.get() + 0.1);
    e.set(.think, next);
}

test "float bit operations" {
    try std.testing.expectEqual(@as(f32, 512), band(512 + 8, FL_ONGROUND));
    try std.testing.expectEqual(@as(f32, 520), bor(512, 8));
    try std.testing.expect(has(IT_AXE + IT_SHOTGUN, IT_SHOTGUN));
    try std.testing.expect(!has(IT_AXE, IT_SHOTGUN));
}
