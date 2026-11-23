//! spectate.qc
//!
//! Copyright (C) 1996-1997  Id Software, Inc.
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation; either version 2 of the License, or (at your option)
//! any later version. See gnu.txt.
//!
//! Added Aug11'97 by Zoid <zoid@idsoftware.com>
//!
//! These functions are called from the server if they exist.
//! Note that Spectators only have one think since they movement code doesn't
//! track them much.  Impulse commands work as usual, but don't call
//! the regular ImpulseCommand handler in weapons.qc since Spectators don't
//! have any weapons and things can explode.
//!
//!   --- Zoid.

const q = @import("defs.zig");
const S = q.S;

/// SpectatorConnect
///
/// called when a spectator connects to a server
pub export fn SpectatorConnect() void {
    const self = q.me();
    q.bprint(q.PRINT_MEDIUM, S("Spectator "));
    q.bprint(q.PRINT_MEDIUM, self.netname);
    q.bprint(q.PRINT_MEDIUM, S(" entered the game\n"));

    self.goalentity = q.world.get(); // used for impulse 1 below
}

/// SpectatorDisconnect
///
/// called when a spectator disconnects from a server
pub export fn SpectatorDisconnect() void {
    const self = q.me();
    q.bprint(q.PRINT_MEDIUM, S("Spectator "));
    q.bprint(q.PRINT_MEDIUM, self.netname);
    q.bprint(q.PRINT_MEDIUM, S(" left the game\n"));
}

/// SpectatorImpulseCommand
///
/// Called by SpectatorThink if the spectator entered an impulse
pub fn SpectatorImpulseCommand() callconv(.c) void {
    const self = q.me();
    if (self.impulse == 1) {
        // teleport the spectator to the next spawn point
        // note that if the spectator is tracking, this doesn't do
        // much
        self.goalentity = q.find(self.goalentity, .classname, S("info_player_deathmatch"));
        if (self.goalentity.eql(q.world.get()))
            self.goalentity = q.find(self.goalentity, .classname, S("info_player_deathmatch"));
        if (self.goalentity.neq(q.world.get())) {
            q.setorigin(self, self.goalentity.origin);
            self.angles = self.goalentity.angles;
            self.fixangle = q.TRUE; // turn this way immediately
        }
    }

    self.impulse = 0;
}

/// SpectatorThink
///
/// Called every frame after physics are run
pub export fn SpectatorThink() void {
    // self.origin, etc contains spectator position, so you could
    // do some neat stuff here

    if (q.me().impulse != 0)
        SpectatorImpulseCommand();
}
