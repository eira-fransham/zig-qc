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

pub export fn button_wait() void {
    const self = q.me();
    self.set(q.state, q.STATE_TOP);
    self.set(.nextthink, self.get(.ltime) + self.get(q.wait));
    self.set(.think, q.func(button_return));
    q.activator.set(self.get(.enemy));
    subs.SUB_UseTargets();
    self.set(.frame, 1); // use alternate textures
}

pub export fn button_done() void {
    q.me().set(q.state, q.STATE_BOTTOM);
}

pub export fn button_return() void {
    const self = q.me();
    self.set(q.state, q.STATE_DOWN);
    subs.SUB_CalcMove(self.get(q.pos1), self.get(q.speed), q.func(button_done));
    self.set(.frame, 0); // use normal textures
    if (self.get(.health) != 0)
        self.set(.takedamage, q.DAMAGE_YES); // can be shot again
}

pub export fn button_blocked() void {
    // do nothing, just don't ome all the way back out
}

pub export fn button_fire() void {
    const self = q.me();
    if (self.get(q.state) == q.STATE_UP or self.get(q.state) == q.STATE_TOP)
        return;

    q.sound(self, q.CHAN_VOICE, self.get(.noise), 1, q.ATTN_NORM);

    self.set(q.state, q.STATE_UP);
    subs.SUB_CalcMove(self.get(q.pos2), self.get(q.speed), q.func(button_wait));
}

pub export fn button_use() void {
    q.me().set(.enemy, q.activator.get());
    button_fire();
}

pub export fn button_touch() void {
    const other = q.other.get();
    if (other.get(.classname).neq(S("player")))
        return;
    q.me().set(.enemy, other);
    button_fire();
}

pub export fn button_killed() void {
    const self = q.me();
    self.set(.enemy, q.damage_attacker.get());
    self.set(.health, self.get(.max_health));
    self.set(.takedamage, q.DAMAGE_NO); // wil be reset upon return
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

    if (self.get(.sounds) == 0) {
        _ = q.precache_sound(S("buttons/airbut1.wav"));
        self.set(.noise, S("buttons/airbut1.wav"));
    }
    if (self.get(.sounds) == 1) {
        _ = q.precache_sound(S("buttons/switch21.wav"));
        self.set(.noise, S("buttons/switch21.wav"));
    }
    if (self.get(.sounds) == 2) {
        _ = q.precache_sound(S("buttons/switch02.wav"));
        self.set(.noise, S("buttons/switch02.wav"));
    }
    if (self.get(.sounds) == 3) {
        _ = q.precache_sound(S("buttons/switch04.wav"));
        self.set(.noise, S("buttons/switch04.wav"));
    }

    subs.SetMovedir();

    self.set(.movetype, q.MOVETYPE_PUSH);
    self.set(.solid, q.SOLID_BSP);
    q.setmodel(self, self.get(.model));

    self.set(.blocked, q.func(button_blocked));
    self.set(.use, q.func(button_use));

    if (self.get(.health) != 0) {
        self.set(.max_health, self.get(.health));
        self.set(q.th_die, q.func(button_killed));
        self.set(.takedamage, q.DAMAGE_YES);
    } else {
        self.set(.touch, q.func(button_touch));
    }

    if (self.get(q.speed) == 0)
        self.set(q.speed, 40);
    if (self.get(q.wait) == 0)
        self.set(q.wait, 1);
    if (self.get(q.lip) == 0)
        self.set(q.lip, 4);

    self.set(q.state, q.STATE_BOTTOM);

    self.set(q.pos1, self.get(.origin));
    const movedir = self.get(.movedir);
    self.set(q.pos2, self.get(q.pos1) + q.scale(movedir, q.fabs(q.dot(movedir, self.get(.size))) - self.get(q.lip)));
}
