//! progs.src: every file of the QuakeWorld game code, in the original order.
//! Their exported functions make up the progs.

const files = .{
    @import("defs.zig"),
    @import("subs.zig"),
    @import("combat.zig"),
    @import("items.zig"),
    @import("weapons.zig"),
    @import("world.zig"),
    @import("client.zig"),
    @import("spectate.zig"),
    @import("player.zig"),
    @import("doors.zig"),
    @import("buttons.zig"),
    @import("triggers.zig"),
    @import("plats.zig"),
    @import("misc.zig"),

    @import("server.zig"),

    // Not in `progs.src`; directives only.
    @import("models.zig"),
    @import("sprites.zig"),
};

comptime {
    for (files) |f| _ = f;
}

test {
    inline for (files) |f| _ = f;
}
