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
    q.bprint(q.PRINT_MEDIUM, self.get(.netname));
    q.bprint(q.PRINT_MEDIUM, S(" entered the game\n"));

    self.set(.goalentity, q.world.get()); // used for impulse 1 below
}

/// SpectatorDisconnect
///
/// called when a spectator disconnects from a server
pub export fn SpectatorDisconnect() void {
    const self = q.me();
    q.bprint(q.PRINT_MEDIUM, S("Spectator "));
    q.bprint(q.PRINT_MEDIUM, self.get(.netname));
    q.bprint(q.PRINT_MEDIUM, S(" left the game\n"));
}

/// SpectatorImpulseCommand
///
/// Called by SpectatorThink if the spectator entered an impulse
pub export fn SpectatorImpulseCommand() void {
    const self = q.me();
    if (self.get(.impulse) == 1) {
        // teleport the spectator to the next spawn point
        // note that if the spectator is tracking, this doesn't do
        // much
        self.set(.goalentity, q.find(self.get(.goalentity), q.fieldRef(.classname), S("info_player_deathmatch")));
        if (self.get(.goalentity).eql(q.world.get()))
            self.set(.goalentity, q.find(self.get(.goalentity), q.fieldRef(.classname), S("info_player_deathmatch")));
        if (self.get(.goalentity).neq(q.world.get())) {
            q.setorigin(self, self.get(.goalentity).get(.origin));
            self.set(.angles, self.get(.goalentity).get(.angles));
            self.set(.fixangle, q.TRUE); // turn this way immediately
        }
    }

    self.set(.impulse, 0);
}

/// SpectatorThink
///
/// Called every frame after physics are run
pub export fn SpectatorThink() void {
    // self.origin, etc contains spectator position, so you could
    // do some neat stuff here

    if (q.me().get(.impulse) != 0)
        SpectatorImpulseCommand();
}
