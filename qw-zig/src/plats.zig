//! plats.qc
//!
//! Copyright (C) 1996-1997  Id Software, Inc.
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation; either version 2 of the License, or (at your option)
//! any later version. See gnu.txt.

const q = @import("defs.zig");
const g = @import("game.zig");
const S = q.S;

const combat = @import("combat.zig");
const subs = @import("subs.zig");

pub const PLAT_LOW_TRIGGER: f32 = 1;

pub fn plat_spawn_inside_trigger() callconv(.c) void {
    const self = q.me();

    //
    // middle trigger
    //
    const trigger = q.spawn();
    trigger.touch = q.func(plat_center_touch);
    trigger.movetype = q.MOVETYPE_NONE;
    trigger.solid = q.SOLID_TRIGGER;
    trigger.enemy = self;

    const mins = self.mins;
    const maxs = self.maxs;
    const size = self.size;
    var tmin = mins + q.vec(25, 25, 0);
    var tmax = maxs - q.vec(25, 25, -8);
    tmin[2] = tmax[2] - (self.get(g.pos1)[2] - self.get(g.pos2)[2] + 8);
    if (q.has(self.spawnflags, PLAT_LOW_TRIGGER))
        tmax[2] = tmin[2] + 8;

    if (size[0] <= 50) {
        tmin[0] = (mins[0] + maxs[0]) / 2;
        tmax[0] = tmin[0] + 1;
    }
    if (size[1] <= 50) {
        tmin[1] = (mins[1] + maxs[1]) / 2;
        tmax[1] = tmin[1] + 1;
    }

    q.setsize(trigger, tmin, tmax);
}

pub fn plat_hit_top() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_NO_PHS_ADD + q.CHAN_VOICE, self.noise1, 1, q.ATTN_NORM);
    self.set(g.state, q.STATE_TOP);
    self.think = q.func(plat_go_down);
    self.nextthink = self.ltime + 3;
}

pub fn plat_hit_bottom() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_NO_PHS_ADD + q.CHAN_VOICE, self.noise1, 1, q.ATTN_NORM);
    self.set(g.state, q.STATE_BOTTOM);
}

pub fn plat_go_down() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_VOICE, self.noise, 1, q.ATTN_NORM);
    self.set(g.state, q.STATE_DOWN);
    subs.SUB_CalcMove(self.get(g.pos2), self.get(g.speed), q.func(plat_hit_bottom));
}

pub fn plat_go_up() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_VOICE, self.noise, 1, q.ATTN_NORM);
    self.set(g.state, q.STATE_UP);
    subs.SUB_CalcMove(self.get(g.pos1), self.get(g.speed), q.func(plat_hit_top));
}

pub fn plat_center_touch() callconv(.c) void {
    const other = q.other.get();
    if (other.classname.neq(S("player")))
        return;

    if (other.health <= 0)
        return;

    // (`self` is not restored afterwards in the original.)
    const self = q.me().enemy;
    q.self.set(self);
    if (self.get(g.state) == q.STATE_BOTTOM) {
        plat_go_up();
    } else if (self.get(g.state) == q.STATE_TOP) {
        self.nextthink = self.ltime + 1; // delay going down
    }
}

pub fn plat_outside_touch() callconv(.c) void {
    const other = q.other.get();
    if (other.classname.neq(S("player")))
        return;

    if (other.health <= 0)
        return;

    //dprint ("plat_outside_touch\n");
    const self = q.me().enemy;
    q.self.set(self);
    if (self.get(g.state) == q.STATE_TOP)
        plat_go_down();
}

pub fn plat_trigger_use() callconv(.c) void {
    if (!q.fnIsNull(q.me().think))
        return; // allready activated
    plat_go_down();
}

pub fn plat_crush() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    //dprint ("plat_crush\n");

    other.set(g.deathtype, S("squish"));
    combat.T_Damage(other, self, self, 1);

    if (self.get(g.state) == q.STATE_UP) {
        plat_go_down();
    } else if (self.get(g.state) == q.STATE_DOWN) {
        plat_go_up();
    } else {
        q.objerror(S("plat_crush: bad self.state\n"));
    }
}

pub fn plat_use() callconv(.c) void {
    const self = q.me();
    self.use = q.func(subs.SUB_Null);
    if (self.get(g.state) != q.STATE_UP)
        q.objerror(S("plat_use: not in up state"));
    plat_go_down();
}

/// QUAKED func_plat (0 .5 .8) ? PLAT_LOW_TRIGGER
/// speed   default 150
///
/// Plats are always drawn in the extended position, so they will light correctly.
///
/// If the plat is the target of another trigger or button, it will start out
/// disabled in the extended position until it is trigger, when it will lower
/// and become a normal plat.
///
/// If the "height" key is set, that will determine the amount the plat moves,
/// instead of being implicitly determined by the model's height.
/// Set "sounds" to one of the following:
/// 1) base fast
/// 2) chain slow
pub export fn func_plat() void {
    const self = q.me();

    if (self.get(g.t_length) == 0)
        self.set(g.t_length, 80);
    if (self.get(g.t_width) == 0)
        self.set(g.t_width, 10);

    if (self.sounds == 0)
        self.sounds = 2;
    // FIX THIS TO LOAD A GENERIC PLAT SOUND

    if (self.sounds == 1) {
        _ = q.precache_sound(S("plats/plat1.wav"));
        _ = q.precache_sound(S("plats/plat2.wav"));
        self.noise = S("plats/plat1.wav");
        self.noise1 = S("plats/plat2.wav");
    }

    if (self.sounds == 2) {
        _ = q.precache_sound(S("plats/medplat1.wav"));
        _ = q.precache_sound(S("plats/medplat2.wav"));
        self.noise = S("plats/medplat1.wav");
        self.noise1 = S("plats/medplat2.wav");
    }

    self.set(g.mangle, self.angles);
    self.angles = q.vec(0, 0, 0);

    self.classname = S("plat");
    self.solid = q.SOLID_BSP;
    self.movetype = q.MOVETYPE_PUSH;
    q.setorigin(self, self.origin);
    q.setmodel(self, self.model);
    q.setsize(self, self.mins, self.maxs);

    self.blocked = q.func(plat_crush);
    if (self.get(g.speed) == 0)
        self.set(g.speed, 150);

    // pos1 is the top position, pos2 is the bottom
    self.set(g.pos1, self.origin);
    var pos2 = self.origin;
    if (self.get(g.height) != 0) {
        pos2[2] = self.origin[2] - self.get(g.height);
    } else {
        pos2[2] = self.origin[2] - self.size[2] + 8;
    }
    self.set(g.pos2, pos2);

    self.use = q.func(plat_trigger_use);

    plat_spawn_inside_trigger(); // the "start moving" trigger

    if (!self.targetname.isNull()) {
        self.set(g.state, q.STATE_UP);
        self.use = q.func(plat_use);
    } else {
        q.setorigin(self, self.get(g.pos2));
        self.set(g.state, q.STATE_BOTTOM);
    }
}

//============================================================================

pub fn train_blocked() callconv(.c) void {
    const self = q.me();
    if (q.time.get() < self.get(g.attack_finished))
        return;
    self.set(g.attack_finished, q.time.get() + 0.5);
    const other = q.other.get();
    other.set(g.deathtype, S("squish"));
    combat.T_Damage(other, self, self, self.get(g.dmg));
}

pub fn train_use() callconv(.c) void {
    if (q.fnNeq(q.me().think, q.func(func_train_find)))
        return; // already activated
    train_next();
}

pub fn train_wait() callconv(.c) void {
    const self = q.me();
    if (self.get(g.wait) != 0) {
        self.nextthink = self.ltime + self.get(g.wait);
        q.sound(self, q.CHAN_NO_PHS_ADD + q.CHAN_VOICE, self.noise, 1, q.ATTN_NORM);
    } else {
        self.nextthink = self.ltime + 0.1;
    }

    self.think = q.func(train_next);
}

pub fn train_next() callconv(.c) void {
    const self = q.me();

    const targ = q.find(q.world.get(), .targetname, self.target);
    self.target = targ.target;
    if (self.target.isNull())
        q.objerror(S("train_next: no next target"));
    if (targ.get(g.wait) != 0) {
        self.set(g.wait, targ.get(g.wait));
    } else {
        self.set(g.wait, 0);
    }
    q.sound(self, q.CHAN_VOICE, self.noise1, 1, q.ATTN_NORM);
    subs.SUB_CalcMove(@as(q.Vector, targ.origin) - self.mins, self.get(g.speed), q.func(train_wait));
}

pub fn func_train_find() callconv(.c) void {
    const self = q.me();

    const targ = q.find(q.world.get(), .targetname, self.target);
    self.target = targ.target;
    q.setorigin(self, @as(q.Vector, targ.origin) - self.mins);
    if (self.targetname.isNull()) { // not triggered, so start immediately
        self.nextthink = self.ltime + 0.1;
        self.think = q.func(train_next);
    }
}

/// QUAKED func_train (0 .5 .8) ?
/// Trains are moving platforms that players can ride.
/// The targets origin specifies the min point of the train at each corner.
/// The train spawns at the first target it is pointing at.
/// If the train is the target of a button or trigger, it will not begin
/// moving until activated.
/// speed   default 100
/// dmg     default 2
/// sounds
/// 1) ratchet metal
pub export fn func_train() void {
    const self = q.me();
    if (self.get(g.speed) == 0)
        self.set(g.speed, 100);
    if (self.target.isNull())
        q.objerror(S("func_train without a target"));
    if (self.get(g.dmg) == 0)
        self.set(g.dmg, 2);

    if (self.sounds == 0) {
        self.noise = S("misc/null.wav");
        _ = q.precache_sound(S("misc/null.wav"));
        self.noise1 = S("misc/null.wav");
        _ = q.precache_sound(S("misc/null.wav"));
    }

    if (self.sounds == 1) {
        self.noise = S("plats/train2.wav");
        _ = q.precache_sound(S("plats/train2.wav"));
        self.noise1 = S("plats/train1.wav");
        _ = q.precache_sound(S("plats/train1.wav"));
    }

    self.set(g.cnt, 1);
    self.solid = q.SOLID_BSP;
    self.movetype = q.MOVETYPE_PUSH;
    self.blocked = q.func(train_blocked);
    self.use = q.func(train_use);
    self.classname = S("train");

    q.setmodel(self, self.model);
    q.setsize(self, self.mins, self.maxs);
    q.setorigin(self, self.origin);

    // start trains on the second frame, to make sure their targets have had
    // a chance to spawn
    self.nextthink = self.ltime + 0.1;
    self.think = q.func(func_train_find);
}

/// QUAKED misc_teleporttrain (0 .5 .8) (-8 -8 -8) (8 8 8)
/// This is used for the final bos
pub export fn misc_teleporttrain() void {
    const self = q.me();
    if (self.get(g.speed) == 0)
        self.set(g.speed, 100);
    if (self.target.isNull())
        q.objerror(S("func_train without a target"));

    self.set(g.cnt, 1);
    self.solid = q.SOLID_NOT;
    self.movetype = q.MOVETYPE_PUSH;
    self.blocked = q.func(train_blocked);
    self.use = q.func(train_use);
    self.avelocity = q.vec(100, 200, 300);

    self.noise = S("misc/null.wav");
    _ = q.precache_sound(S("misc/null.wav"));
    self.noise1 = S("misc/null.wav");
    _ = q.precache_sound(S("misc/null.wav"));

    _ = q.precache_model2(S("progs/teleport.mdl"));
    q.setmodel(self, S("progs/teleport.mdl"));
    q.setsize(self, self.mins, self.maxs);
    q.setorigin(self, self.origin);

    // start trains on the second frame, to make sure their targets have had
    // a chance to spawn
    self.nextthink = self.ltime + 0.1;
    self.think = q.func(func_train_find);
}
