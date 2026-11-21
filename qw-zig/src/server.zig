//! server.qc
//!
//! server functions (movetarget code)
//!
//! Copyright (C) 1996-1997  Id Software, Inc.
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation; either version 2 of the License, or (at your option)
//! any later version. See gnu.txt.

const q = @import("defs.zig");
const S = q.S;

// QuakeWorld has no monsters; their spawn functions remove the entity.

/// `remove(self)`.
inline fn removeSelf() void {
    q.remove(q.me());
}

pub export fn monster_ogre() void {
    removeSelf();
}
pub export fn monster_demon1() void {
    removeSelf();
}
pub export fn monster_shambler() void {
    removeSelf();
}
pub export fn monster_knight() void {
    removeSelf();
}
pub export fn monster_army() void {
    removeSelf();
}
pub export fn monster_wizard() void {
    removeSelf();
}
pub export fn monster_dog() void {
    removeSelf();
}
pub export fn monster_zombie() void {
    removeSelf();
}
pub export fn monster_boss() void {
    removeSelf();
}
pub export fn monster_tarbaby() void {
    removeSelf();
}
pub export fn monster_hell_knight() void {
    removeSelf();
}
pub export fn monster_fish() void {
    removeSelf();
}
pub export fn monster_shalrath() void {
    removeSelf();
}
pub export fn monster_enforcer() void {
    removeSelf();
}
pub export fn monster_oldone() void {
    removeSelf();
}
pub export fn event_lightning() void {
    removeSelf();
}

// ==============================================================================
//
// MOVETARGET CODE
//
// The angle of the movetarget effects standing and bowing direction, but has
// no effect on movement, which allways heads to the next target.
//
// targetname
// must be present.  The name of this movetarget.
//
// target
// the next spot to move to.  If not present, stop here for good.
//
// pausetime
// The number of seconds to spend standing or bowing for path_stand or path_bow
//
// ==============================================================================

/// t_movetarget
///
/// Something has bumped into a movetarget.  If it is a monster
/// moving towards it, change the next destination and continue.
pub export fn t_movetarget() void {
    if (q.other.get().get(q.movetarget).neq(q.me()))
        return;

    if (!q.other.get().get(.enemy).isNull())
        return; // fighting, not following a path

    const temp = q.me();
    q.self.set(q.other.get());
    q.other.set(temp);
    const self = q.me();
    const other = q.other.get();

    if (self.get(.classname).eql(S("monster_ogre")))
        q.sound(self, q.CHAN_VOICE, S("ogre/ogdrag.wav"), 1, q.ATTN_IDLE); // play chainsaw drag sound

    //dprint ("t_movetarget\n");
    self.set(q.movetarget, q.find(q.world.get(), q.fieldRef(.targetname), other.get(.target)));
    self.set(.goalentity, self.get(q.movetarget));
    self.set(.ideal_yaw, q.vectoyaw(self.get(.goalentity).get(.origin) - self.get(.origin)));
    if (self.get(q.movetarget).isNull()) {
        self.set(q.pausetime, q.time.get() + 999999);
        self.get(q.th_stand)();
        return;
    }
}

pub export fn movetarget_f() void {
    const self = q.me();
    if (self.get(.targetname).isNull())
        q.objerror(S("monster_movetarget: no targetname"));

    self.set(.solid, q.SOLID_TRIGGER);
    self.set(.touch, q.func(t_movetarget));
    q.setsize(self, q.vec(-8, -8, -8), q.vec(8, 8, 8));
}

/// QUAKED path_corner (0.5 0.3 0) (-8 -8 -8) (8 8 8)
/// Monsters will continue walking towards the next target corner.
pub export fn path_corner() void {
    movetarget_f();
}

//============================================================================
