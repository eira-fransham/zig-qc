//! QuakeC system globals, named as in vanilla `defs.qc`.
//!
//! Each global lives at `slot * 4` in linear memory, which wasm-to-qc maps
//! back onto the QuakeC global with the same name.

const std = @import("std");

// HACK: Workaround for some jankiness with LLVM.
comptime {
    asm (
        \\.globl __funcref_call_table
        \\.weak __funcref_call_table
        \\.tabletype __funcref_call_table, funcref, 1
        \\__funcref_call_table:
    );
}

/// The vanilla NetQuake system entity fields (`entvars_t`), named as in
/// `defs.qc`. Each value is the field's QuakeC field offset, which is what
/// the field intrinsics below pass to the converter. Vector fields take up
/// three slots; access their components through the `Vector` instead.
pub const Field = enum(u32) {
    modelindex = 0,
    absmin = 1,
    absmax = 4,
    ltime = 7,
    movetype = 8,
    solid = 9,
    origin = 10,
    oldorigin = 13,
    velocity = 16,
    angles = 19,
    avelocity = 22,
    punchangle = 25,
    classname = 28,
    model = 29,
    frame = 30,
    skin = 31,
    effects = 32,
    mins = 33,
    maxs = 36,
    size = 39,
    touch = 42,
    use = 43,
    think = 44,
    blocked = 45,
    nextthink = 46,
    groundentity = 47,
    health = 48,
    frags = 49,
    weapon = 50,
    weaponmodel = 51,
    weaponframe = 52,
    currentammo = 53,
    ammo_shells = 54,
    ammo_nails = 55,
    ammo_rockets = 56,
    ammo_cells = 57,
    items = 58,
    takedamage = 59,
    chain = 60,
    deadflag = 61,
    view_ofs = 62,
    button0 = 65,
    button1 = 66,
    button2 = 67,
    impulse = 68,
    fixangle = 69,
    v_angle = 70,
    idealpitch = 73,
    netname = 74,
    enemy = 75,
    flags = 76,
    colormap = 77,
    team = 78,
    max_health = 79,
    teleport_time = 80,
    armortype = 81,
    armorvalue = 82,
    waterlevel = 83,
    watertype = 84,
    ideal_yaw = 85,
    yaw_speed = 86,
    aiment = 87,
    goalentity = 88,
    spawnflags = 89,
    target = 90,
    targetname = 91,
    dmg_take = 92,
    dmg_save = 93,
    dmg_inflictor = 94,
    owner = 95,
    movedir = 96,
    message = 99,
    sounds = 100,
    noise = 101,
    noise1 = 102,
    noise2 = 103,
    noise3 = 104,

    pub fn Type(comptime this: @This()) type {
        return switch (this) {
            .modelindex, .ltime, .movetype, .solid, .frame, .skin, .effects, .nextthink, .health, .frags, .weapon, .weaponframe, .currentammo, .ammo_shells, .ammo_nails, .ammo_rockets, .ammo_cells, .items, .takedamage, .deadflag, .button0, .button1, .button2, .impulse, .fixangle, .idealpitch, .flags, .colormap, .team, .max_health, .teleport_time, .armortype, .armorvalue, .waterlevel, .watertype, .ideal_yaw, .yaw_speed, .spawnflags, .dmg_take, .dmg_save, .sounds => f32,
            .absmin, .absmax, .origin, .oldorigin, .velocity, .angles, .avelocity, .punchangle, .mins, .maxs, .size, .view_ofs, .v_angle, .movedir => Vector,
            .classname, .model, .weaponmodel, .netname, .target, .targetname, .message, .noise, .noise1, .noise2, .noise3 => String,
            .groundentity, .chain, .enemy, .aiment, .goalentity, .dmg_inflictor, .owner => Entity,
            .touch, .use, .think, .blocked => Function,
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
extern fn __quakec_load_entity(ptr: *const anyopaque) EntityRef;
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

        pub inline fn get(this: Self) Ty {
            return switch (@typeInfo(T)) {
                .int => @intFromFloat(__quakec_load_float(this.ptr)),
                .float => @as(T, __quakec_load_float(this.ptr)),
                .bool => __quakec_load_float(this.ptr) != 0,
                .@"fn" => @ptrCast(__quakec_load_fn(this.ptr)),
                else => if (T == Entity)
                    .{ .ref = __quakec_load_entity(this.ptr) }
                else if (T == String)
                    .{ .ref = __quakec_load_string(this.ptr) }
                else if (T == Vector)
                    __quakec_load_vector(this.ptr)
                else
                    @compileError("unsupported global type " ++ @typeName(T)),
            };
        }

        pub inline fn set(this: Self, value: Ty) void {
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

pub const self = global(Entity, 28);
pub const other = global(Entity, 29);
pub const world = global(Entity, 30);
pub const time = global(f32, 31);
pub const frametime = global(f32, 32);
pub const force_retouch = global(bool, 33);
pub const mapname = global(String, 34);
pub const deathmatch = global(bool, 35);
pub const coop = global(bool, 36);
pub const teamplay = global(bool, 37);
pub const serverflags = global(u32, 38);
pub const total_secrets = global(u32, 39);
pub const total_monsters = global(u32, 40);
pub const found_secrets = global(u32, 41);
pub const killed_monsters = global(u32, 42);
pub const parm1 = global(u32, 43);
pub const parm2 = global(u32, 44);
pub const parm3 = global(u32, 45);
pub const parm4 = global(u32, 46);
pub const parm5 = global(u32, 47);
pub const parm6 = global(u32, 48);
pub const parm7 = global(u32, 49);
pub const parm8 = global(u32, 50);
pub const parm9 = global(u32, 51);
pub const parm10 = global(u32, 52);
pub const parm11 = global(u32, 53);
pub const parm12 = global(u32, 54);
pub const parm13 = global(u32, 55);
pub const parm14 = global(u32, 56);
pub const parm15 = global(u32, 57);
pub const parm16 = global(u32, 58);
pub const v_forward = global(Vector, 59);
pub const v_forward_x = global(f32, 59);
pub const v_forward_y = global(f32, 60);
pub const v_forward_z = global(f32, 61);
pub const v_up = global(Vector, 62);
pub const v_up_x = global(f32, 62);
pub const v_up_y = global(f32, 63);
pub const v_up_z = global(f32, 64);
pub const v_right = global(Vector, 65);
pub const v_right_x = global(f32, 65);
pub const v_right_y = global(f32, 66);
pub const v_right_z = global(f32, 67);
pub const trace_allsolid = global(bool, 68);
pub const trace_startsolid = global(bool, 69);
pub const trace_fraction = global(f32, 70);
pub const trace_endpos = global(Vector, 71);
pub const trace_endpos_x = global(f32, 71);
pub const trace_endpos_y = global(f32, 72);
pub const trace_endpos_z = global(f32, 73);
pub const trace_plane_normal = global(Vector, 74);
pub const trace_plane_normal_x = global(f32, 74);
pub const trace_plane_normal_y = global(f32, 75);
pub const trace_plane_normal_z = global(f32, 76);
pub const trace_plane_dist = global(f32, 77);
pub const trace_ent = global(Entity, 78);
pub const trace_inopen = global(bool, 79);
pub const trace_inwater = global(bool, 80);
pub const msg_entity = global(Entity, 81);

// Functions
pub const main = global(fn () void, 82);
pub const StartFrame = global(fn () void, 83);
pub const PlayerPreThink = global(fn () void, 84);
pub const PlayerPostThink = global(fn () void, 85);
pub const ClientKill = global(fn () void, 86);
pub const ClientConnect = global(fn () void, 87);
pub const PutClientInServer = global(fn () void, 88);
pub const ClientDisconnect = global(fn () void, 89);
pub const SetNewParms = global(fn () void, 90);
pub const SetChangeParms = global(fn () void, 91);

// Builtins (vanilla NetQuake `defs.qc`), imported from the `builtin` module.
// The comment on each gives its vanilla builtin number.

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
pub extern "builtin" fn bprint(s: String) void; // #23
pub extern "builtin" fn sprint(client: Entity, s: String) void; // #24
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
pub extern "builtin" fn particle(o: Vector, d: Vector, color: f32, count: f32) void; // #48
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
