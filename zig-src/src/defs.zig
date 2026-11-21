//! QuakeC system globals, named as in vanilla `defs.qc`.
//!
//! Each global lives at `slot * 4` in linear memory, which wasm-to-qc maps
//! back onto the QuakeC global with the same name.

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

    // `inline` so that `field` reaches the intrinsic as an `i32.const`; the
    // converter needs it to be a compile-time constant.
    pub inline fn get(this: Self, comptime field: Field) field.Type() {
        const offset = @backingInt(field);
        return switch (field.Type()) {
            f32 => __quakec_load_entity_field_float(this.ref, offset),
            Vector => __quakec_load_entity_field_vector(this.ref, offset),
            String => .{ .ref = __quakec_load_entity_field_string(this.ref, offset) },
            Entity => .{ .ref = __quakec_load_entity_field_entity(this.ref, offset) },
            Function => __quakec_load_entity_field_fn(this.ref, offset),
            else => @compileError("unsupported field type " ++ @typeName(field.Type())),
        };
    }

    pub inline fn set(this: Self, comptime field: Field, value: field.Type()) void {
        const offset = @backingInt(field);
        switch (field.Type()) {
            f32 => __quakec_store_entity_field_float(this.ref, offset, value),
            Vector => __quakec_store_entity_field_vector(this.ref, offset, value),
            String => __quakec_store_entity_field_string(this.ref, offset, value.ref),
            Entity => __quakec_store_entity_field_entity(this.ref, offset, value.ref),
            Function => __quakec_store_entity_field_fn(this.ref, offset, value),
            else => @compileError("unsupported field type " ++ @typeName(field.Type())),
        }
    }
};
pub const String = extern struct {
    ref: StringRef,
};

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

extern fn __quakec_load_entity(ptr: *const volatile void) EntityRef;
extern fn __quakec_load_string(ptr: *const volatile void) StringRef;
extern fn __quakec_load_fn(ptr: *const volatile void) *addrspace(.funcref) const fn () callconv(.c) void;

extern fn __quakec_store_entity(ptr: *align(4) volatile void, ent: EntityRef) void;
extern fn __quakec_store_string(ptr: *align(4) volatile void, ent: StringRef) void;
extern fn __quakec_store_fn(ptr: *align(4) volatile void, value: *addrspace(.funcref) const fn () callconv(.c) void) void;

pub fn Global(comptime T: type) type {
    const QCType = switch (@typeInfo(T)) {
        .pointer => void,
        .int, .bool => f32,
        else => T,
    };
    const Ty = switch (@typeInfo(T)) {
        .@"fn" => *addrspace(.funcref) const T,
        else => T,
    };

    return struct {
        const Self = @This();

        ptr: *align(4) volatile QCType,

        pub inline fn get(this: *const Self) Ty {
            return switch (@typeInfo(T)) {
                .int => @intFromFloat(this.ptr.*),
                .float => @as(T, this.ptr.*),
                .bool => if (this.ptr.* == 0) false else true,
                .@"fn" => @ptrCast(__quakec_load_fn(@ptrCast(this.ptr))),
                else => {
                    if (T == Entity) {
                        return .{ .ref = __quakec_load_entity(this.ptr) };
                    } else if (T == String) {
                        return .{ .ref = __quakec_load_string(this.ptr) };
                    } else {
                        this.ptr.*;
                    }
                },
            };
        }

        pub inline fn set(this: *const Self, value: Ty) void {
            return switch (@typeInfo(T)) {
                .int => this.ptr.* = @intFromFloat(value),
                .float => this.ptr.* = @as(QCType, value),
                .bool => this.ptr.* = if (value) 1 else 0,
                .@"fn" => __quakec_store_fn(this.ptr, @ptrCast(value)),
                else => {
                    if (T == Entity) {
                        __quakec_store_entity(this.ptr, value.ref);
                    } else if (T == String) {
                        __quakec_store_string(this.ptr, value.ref);
                    } else {
                        this.ptr.* = value;
                    }
                },
            };
        }
    };
}

pub fn global(comptime T: type, comptime offset: comptime_int) Global(T) {
    return .{ .ptr = @ptrFromInt(offset * 4) };
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
pub const v_forward = global([3]f32, 59);
pub const v_forward_x = global(f32, 59);
pub const v_forward_y = global(f32, 60);
pub const v_forward_z = global(f32, 61);
pub const v_up = global([3]f32, 62);
pub const v_up_x = global(f32, 62);
pub const v_up_y = global(f32, 63);
pub const v_up_z = global(f32, 64);
pub const v_right = global([3]f32, 65);
pub const v_right_x = global(f32, 65);
pub const v_right_y = global(f32, 66);
pub const v_right_z = global(f32, 67);
pub const trace_allsolid = global(bool, 68);
pub const trace_startsolid = global(bool, 69);
pub const trace_fraction = global(f32, 70);
pub const trace_endpos = global([3]f32, 71);
pub const trace_endpos_x = global(f32, 71);
pub const trace_endpos_y = global(f32, 72);
pub const trace_endpos_z = global(f32, 73);
pub const trace_plane_normal = global([3]f32, 74);
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
