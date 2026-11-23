//! buttons.qc
//!
//! button and multiple button
//!
//! Copyright (C) 1996-1997  Id Software, Inc.
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation; either version 2 of the License, or (at your option)
//! any later version. See gnu.txt.

const q = @import("defs.zig");
const S = q.S;

const subs = @import("subs.zig");

pub fn button_wait() callconv(.c) void {
    const self = q.me();
    self.set(q.state, q.STATE_TOP);
    self.nextthink = self.ltime + self.get(q.wait);
    self.think = q.func(button_return);
    q.activator.set(self.enemy);
    subs.SUB_UseTargets();
    self.frame = 1; // use alternate textures
}

pub fn button_done() callconv(.c) void {
    q.me().set(q.state, q.STATE_BOTTOM);
}

pub fn button_return() callconv(.c) void {
    const self = q.me();
    self.set(q.state, q.STATE_DOWN);
    subs.SUB_CalcMove(self.get(q.pos1), self.get(q.speed), q.func(button_done));
    self.frame = 0; // use normal textures
    if (self.health != 0)
        self.takedamage = q.DAMAGE_YES; // can be shot again
}

pub fn button_blocked() callconv(.c) void {
    // do nothing, just don't ome all the way back out
}

pub fn button_fire() callconv(.c) void {
    const self = q.me();
    if (self.get(q.state) == q.STATE_UP or self.get(q.state) == q.STATE_TOP)
        return;

    q.sound(self, q.CHAN_VOICE, self.noise, 1, q.ATTN_NORM);

    self.set(q.state, q.STATE_UP);
    subs.SUB_CalcMove(self.get(q.pos2), self.get(q.speed), q.func(button_wait));
}

pub fn button_use() callconv(.c) void {
    q.me().enemy = q.activator.get();
    button_fire();
}

pub fn button_touch() callconv(.c) void {
    const other = q.other.get();
    if (other.classname.neq(S("player")))
        return;
    q.me().enemy = other;
    button_fire();
}

pub fn button_killed() callconv(.c) void {
    const self = q.me();
    self.enemy = q.damage_attacker.get();
    self.health = self.max_health;
    self.takedamage = q.DAMAGE_NO; // wil be reset upon return
    button_fire();
}

/// QUAKED func_button (0 .5 .8) ?
/// When a button is touched, it moves some distance in the direction of it's
/// angle, triggers all of it's targets, waits some time, then returns to it's
/// original position where it can be triggered again.
///
/// "angle"     determines the opening direction
/// "target"    all entities with a matching targetname will be used
/// "speed"     override the default 40 speed
/// "wait"      override the default 1 second wait (-1 = never return)
/// "lip"       override the default 4 pixel lip remaining at end of move
/// "health"    if set, the button must be killed instead of touched
/// "sounds"
/// 0) steam metal
/// 1) wooden clunk
/// 2) metallic click
/// 3) in-out
pub export fn func_button() void {
    const self = q.me();

    if (self.sounds == 0) {
        _ = q.precache_sound(S("buttons/airbut1.wav"));
        self.noise = S("buttons/airbut1.wav");
    }
    if (self.sounds == 1) {
        _ = q.precache_sound(S("buttons/switch21.wav"));
        self.noise = S("buttons/switch21.wav");
    }
    if (self.sounds == 2) {
        _ = q.precache_sound(S("buttons/switch02.wav"));
        self.noise = S("buttons/switch02.wav");
    }
    if (self.sounds == 3) {
        _ = q.precache_sound(S("buttons/switch04.wav"));
        self.noise = S("buttons/switch04.wav");
    }

    subs.SetMovedir();

    self.movetype = q.MOVETYPE_PUSH;
    self.solid = q.SOLID_BSP;
    q.setmodel(self, self.model);

    self.blocked = q.func(button_blocked);
    self.use = q.func(button_use);

    if (self.health != 0) {
        self.max_health = self.health;
        self.set(q.th_die, q.func(button_killed));
        self.takedamage = q.DAMAGE_YES;
    } else {
        self.touch = q.func(button_touch);
    }

    if (self.get(q.speed) == 0)
        self.set(q.speed, 40);
    if (self.get(q.wait) == 0)
        self.set(q.wait, 1);
    if (self.get(q.lip) == 0)
        self.set(q.lip, 4);

    self.set(q.state, q.STATE_BOTTOM);

    self.set(q.pos1, self.origin);
    const movedir = self.movedir;
    self.set(q.pos2, self.get(q.pos1) + q.scale(movedir, q.fabs(q.dot(movedir, self.size)) - self.get(q.lip)));
}
