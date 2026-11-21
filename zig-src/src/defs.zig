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

pub const Entity = *addrspace(.externref) const struct {};
pub const String = *addrspace(.externref) const struct {};

comptime {
    if (Entity == String) unreachable;
}

extern fn __quakec_load_entity(ptr: *const volatile void) Entity;
extern fn __quakec_load_string(ptr: *const volatile void) String;
extern fn __quakec_load_fn(ptr: *const volatile void) *addrspace(.funcref) const fn () callconv(.c) void;

extern fn __quakec_store_entity(ptr: *align(4) volatile void, ent: Entity) void;
extern fn __quakec_store_string(ptr: *align(4) volatile void, ent: String) void;
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
                .pointer => {
                    if (T == Entity) {
                        return __quakec_load_entity(this.ptr);
                    } else if (T == String) {
                        return __quakec_load_string(this.ptr);
                    } else {
                        unreachable;
                    }
                },
                .int => @intFromFloat(this.ptr.*),
                .float => @as(T, this.ptr.*),
                .bool => if (this.ptr.* == 0) false else true,
                .@"fn" => @ptrCast(__quakec_load_fn(@ptrCast(this.ptr))),
                else => this.ptr.*,
            };
        }

        pub inline fn set(this: *const Self, value: Ty) void {
            return switch (@typeInfo(T)) {
                .pointer => {
                    if (T == Entity) {
                        __quakec_store_entity(this.ptr, value);
                    } else if (T == String) {
                        __quakec_store_string(this.ptr, value);
                    } else {
                        unreachable;
                    }
                },
                .int => this.ptr.* = @intFromFloat(value),
                .float => this.ptr.* = @as(QCType, value),
                .bool => this.ptr.* = if (value) 1 else 0,
                .@"fn" => __quakec_store_fn(this.ptr, @ptrCast(value)),
                else => this.ptr.* = value,
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
pub const main = global(fn (f32) f32, 82);
pub const StartFrame = global(fn () void, 83);
pub const PlayerPreThink = global(fn () void, 84);
pub const PlayerPostThink = global(fn () void, 85);
pub const ClientKill = global(fn () void, 86);
pub const ClientConnect = global(fn () void, 87);
pub const PutClientInServer = global(fn () void, 88);
pub const ClientDisconnect = global(fn () void, 89);
pub const SetNewParms = global(fn () void, 90);
pub const SetChangeParms = global(fn () void, 91);
