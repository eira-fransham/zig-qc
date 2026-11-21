//! By convention, root.zig is the root source file when making a package.

// HACK: Workaround for some jankiness with LLVM.
comptime {
    asm (
        \\.globl __funcref_call_table
        \\.weak __funcref_call_table
        \\.tabletype __funcref_call_table, funcref, 1
        \\__funcref_call_table:
    );
}

const Entity = *addrspace(.externref) const struct {};
const String = *addrspace(.externref) const struct {};

comptime {
    if (Entity == String) unreachable;
}

pub extern fn __quakec_load_entity(ptr: *const volatile void) Entity;
pub extern fn __quakec_load_string(ptr: *const volatile void) String;
pub extern fn __quakec_load_fn(ptr: *const volatile void) *addrspace(.funcref) const fn () callconv(.c) void;

pub extern fn __quakec_store_entity(ptr: *align(4) volatile void, ent: Entity) void;
pub extern fn __quakec_store_string(ptr: *align(4) volatile void, ent: String) void;
pub extern fn __quakec_store_fn(ptr: *align(4) volatile void, value: *addrspace(.funcref) const fn () callconv(.c) void) void;

fn Global(comptime T: type) type {
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

        pub inline fn get(self: *const Self) Ty {
            return switch (@typeInfo(T)) {
                .pointer => {
                    if (T == Entity) {
                        return __quakec_load_entity(self.ptr);
                    } else if (T == String) {
                        return __quakec_load_string(self.ptr);
                    } else {
                        unreachable;
                    }
                },
                .int => @intFromFloat(self.ptr.*),
                .float => @as(T, self.ptr.*),
                .bool => if (self.ptr.* == 0) false else true,
                .@"fn" => @ptrCast(__quakec_load_fn(@ptrCast(self.ptr))),
                else => self.ptr.*,
            };
        }

        pub inline fn set(self: *const Self, value: Ty) void {
            return switch (@typeInfo(T)) {
                .pointer => {
                    if (T == Entity) {
                        __quakec_store_entity(self.ptr, value);
                    } else if (T == String) {
                        __quakec_store_string(self.ptr, value);
                    } else {
                        unreachable;
                    }
                },
                .int => self.ptr.* = @intFromFloat(value),
                .float => self.ptr.* = @as(QCType, value),
                .bool => self.ptr.* = if (value) 1 else 0,
                .@"fn" => __quakec_store_fn(self.ptr, @ptrCast(value)),
                else => self.ptr.* = value,
            };
        }
    };
}

fn global(comptime T: type, comptime offset: comptime_int) Global(T) {
    return .{ .ptr = @ptrFromInt(offset * 4) };
}

pub const Self_ = global(Entity, 28);
pub const Other = global(Entity, 29);
pub const World = global(Entity, 30);
pub const Time = global(f32, 31);
pub const FrameTime = global(f32, 32);
pub const ForceRetouch = global(bool, 33);
pub const MapName = global(String, 34);
pub const Deathmatch = global(bool, 35);
pub const Coop = global(bool, 36);
pub const TeamPlay = global(bool, 37);
pub const ServerFlags = global(u32, 38);
pub const TotalSecrets = global(u32, 39);
pub const TotalMonsters = global(u32, 40);
pub const FoundSecrets = global(u32, 41);
pub const KilledMonsters = global(u32, 42);
pub const SpawnParam0 = global(u32, 43);
pub const SpawnParam1 = global(u32, 44);
pub const SpawnParam2 = global(u32, 45);
pub const SpawnParam3 = global(u32, 46);
pub const SpawnParam4 = global(u32, 47);
pub const SpawnParam5 = global(u32, 48);
pub const SpawnParam6 = global(u32, 49);
pub const SpawnParam7 = global(u32, 50);
pub const SpawnParam8 = global(u32, 51);
pub const SpawnParam9 = global(u32, 52);
pub const SpawnParam10 = global(u32, 53);
pub const SpawnParam11 = global(u32, 54);
pub const SpawnParam12 = global(u32, 55);
pub const SpawnParam13 = global(u32, 56);
pub const SpawnParam14 = global(u32, 57);
pub const SpawnParam15 = global(u32, 58);
pub const VForward = global(f32[3], 59);
pub const VForwardX = global(f32, 59);
pub const VForwardY = global(f32, 60);
pub const VForwardZ = global(f32, 61);
pub const VUp = global(f32[3], 62);
pub const VUpX = global(f32, 62);
pub const VUpY = global(f32, 63);
pub const VUpZ = global(f32, 64);
pub const VRight = global(f32[3], 65);
pub const VRightX = global(f32, 65);
pub const VRightY = global(f32, 66);
pub const VRightZ = global(f32, 67);
pub const TraceAllSolid = global(bool, 68);
pub const TraceStartSolid = global(bool, 69);
pub const TraceFraction = global(f32, 70);
pub const TraceEndPos = global(f32[3], 71);
pub const TraceEndPosX = global(f32, 71);
pub const TraceEndPosY = global(f32, 72);
pub const TraceEndPosZ = global(f32, 73);
pub const TracePlaneNormal = global(f32[3], 74);
pub const TracePlaneNormalX = global(f32, 74);
pub const TracePlaneNormalY = global(f32, 75);
pub const TracePlaneNormalZ = global(f32, 76);
pub const TracePlaneDist = global(f32, 77);
pub const TraceEntity = global(Entity, 78);
pub const TraceInOpen = global(bool, 79);
pub const TraceInWater = global(bool, 80);
pub const MsgEntity = global(Entity, 81);
// Functions
pub const Main = global(fn (f32) f32, 82);
pub const StartFrame = global(fn () void, 83);
pub const PlayerPreThink = global(fn () void, 84);
pub const PlayerPostThink = global(fn () void, 85);
pub const ClientKill = global(fn () void, 86);
pub const ClientConnect = global(fn () void, 87);
pub const PutClientInServer = global(fn () void, 88);
pub const ClientDisconnect = global(fn () void, 89);
pub const SetNewArgs = global(fn () void, 90);
pub const SetChangeArgs = global(fn () void, 91);

pub const world: *const volatile f32 = @ptrFromInt(0x4);

pub extern fn doSomething(foo: Entity, bar: Entity, baz: f32) f32;

pub export fn doSomethingElse(foo: Entity) f32 {
    const main = Main.get();
    const main_out = main(123);
    const val = doSomething(foo, World.get(), main_out);
    VUpX.set(val);
    return val;
}
