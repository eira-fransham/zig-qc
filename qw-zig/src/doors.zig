//! doors.qc
//!
//! Copyright (C) 1996-1997  Id Software, Inc.
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation; either version 2 of the License, or (at your option)
//! any later version. See gnu.txt.
//!
//! Doors are similar to buttons, but can spawn a fat trigger field around
//! them to open without a touch, and they link together to form simultanious
//! double/quad doors.
//!
//! Door.owner is the master door.  If there is only one door, it points to
//! itself. If multiple doors, all will point to a single one.
//!
//! Door.enemy chains from the master door through all doors linked in the
//! chain.

const q = @import("defs.zig");
const g = @import("game.zig");
const S = q.S;
const Entity = q.Entity;
const Vector = q.Vector;

const combat = @import("combat.zig");
const subs = @import("subs.zig");

pub const DOOR_START_OPEN: f32 = 1;
pub const DOOR_DONT_LINK: f32 = 4;
pub const DOOR_GOLD_KEY: f32 = 8;
pub const DOOR_SILVER_KEY: f32 = 16;
pub const DOOR_TOGGLE: f32 = 32;

// =============================================================================
//
// THINK FUNCTIONS
//
// =============================================================================

pub fn door_blocked() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    other.set(g.deathtype, S("squish"));
    combat.T_Damage(other, self, self.goalentity, self.get(g.dmg));

    // if a door has a negative wait, it would never come back if blocked,
    // so let it just squash the object to death real fast
    if (self.get(g.wait) >= 0) {
        if (self.get(g.state) == q.STATE_DOWN) {
            door_go_up();
        } else {
            door_go_down();
        }
    }
}

pub fn door_hit_top() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_NO_PHS_ADD + q.CHAN_VOICE, self.noise1, 1, q.ATTN_NORM);
    self.set(g.state, q.STATE_TOP);
    if (q.has(self.spawnflags, DOOR_TOGGLE))
        return; // don't come down automatically
    self.think = q.func(door_go_down);
    self.nextthink = self.ltime + self.get(g.wait);
}

pub fn door_hit_bottom() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_NO_PHS_ADD + q.CHAN_VOICE, self.noise1, 1, q.ATTN_NORM);
    self.set(g.state, q.STATE_BOTTOM);
}

pub fn door_go_down() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_VOICE, self.noise2, 1, q.ATTN_NORM);
    if (self.max_health != 0) {
        self.takedamage = q.DAMAGE_YES;
        self.health = self.max_health;
    }

    self.set(g.state, q.STATE_DOWN);
    subs.SUB_CalcMove(self.get(g.pos1), self.get(g.speed), q.func(door_hit_bottom));
}

pub fn door_go_up() callconv(.c) void {
    const self = q.me();
    if (self.get(g.state) == q.STATE_UP)
        return; // allready going up

    if (self.get(g.state) == q.STATE_TOP) { // reset top wait time
        self.nextthink = self.ltime + self.get(g.wait);
        return;
    }

    q.sound(self, q.CHAN_VOICE, self.noise2, 1, q.ATTN_NORM);
    self.set(g.state, q.STATE_UP);
    subs.SUB_CalcMove(self.get(g.pos2), self.get(g.speed), q.func(door_hit_top));

    subs.SUB_UseTargets();
}

// =============================================================================
//
// ACTIVATION FUNCTIONS
//
// =============================================================================

pub fn door_fire() callconv(.c) void {
    var self = q.me();

    if (self.owner.neq(self))
        q.objerror(S("door_fire: self.owner != self"));

    // play use key sound

    if (self.items != 0)
        q.sound(self, q.CHAN_VOICE, self.get(g.noise4), 1, q.ATTN_NORM);

    self.message = g.string_null.get(); // no more message
    const oself = self;

    if (q.has(self.spawnflags, DOOR_TOGGLE)) {
        if (self.get(g.state) == q.STATE_UP or self.get(g.state) == q.STATE_TOP) {
            const starte = self;
            while (true) {
                door_go_down();
                self = self.enemy;
                q.self.set(self);
                if (!(self.neq(starte) and self.neq(q.world.get()))) break;
            }
            q.self.set(oself);
            return;
        }
    }

    // trigger all paired doors
    const starte = self;

    while (true) {
        self.goalentity = g.activator.get(); // Who fired us
        door_go_up();
        self = self.enemy;
        q.self.set(self);
        if (!(self.neq(starte) and self.neq(q.world.get()))) break;
    }
    q.self.set(oself);
}

pub fn door_use() callconv(.c) void {
    const self = q.me();

    self.message = S(""); // door message are for touch only
    self.owner.message = S("");
    self.enemy.message = S("");

    const oself = self;
    q.self.set(self.owner);
    door_fire();
    q.self.set(oself);
}

pub fn door_trigger_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    if (other.health <= 0)
        return;

    if (q.time.get() < self.get(g.attack_finished))
        return;
    self.set(g.attack_finished, q.time.get() + 1);

    g.activator.set(other);

    // (`self` is not restored afterwards in the original.)
    q.self.set(self.owner);
    door_use();
}

pub fn door_killed() callconv(.c) void {
    const oself = q.me();
    const self = oself.owner;
    q.self.set(self);
    self.health = self.max_health;
    self.takedamage = q.DAMAGE_NO; // wil be reset upon return
    door_use();
    q.self.set(oself);
}

/// door_touch
///
/// Prints messages and opens key doors
pub fn door_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    const owner = self.owner;

    if (other.classname.neq(S("player")))
        return;
    if (owner.get(g.attack_finished) > q.time.get())
        return;

    owner.set(g.attack_finished, q.time.get() + 2);

    if (owner.message.neq(S(""))) {
        q.centerprint(other, owner.message);
        q.sound(other, q.CHAN_VOICE, S("misc/talk.wav"), 1, q.ATTN_NORM);
    }

    // key door stuff
    if (self.items == 0)
        return;

    // FIXME: blink key on player's status bar
    if (q.band(self.items, other.items) != self.items) {
        const worldtype = q.world.get().get(g.worldtype);
        if (owner.items == q.IT_KEY1) {
            if (worldtype == 2) {
                q.centerprint(other, S("You need the silver keycard"));
                q.sound(self, q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
            } else if (worldtype == 1) {
                q.centerprint(other, S("You need the silver runekey"));
                q.sound(self, q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
            } else if (worldtype == 0) {
                q.centerprint(other, S("You need the silver key"));
                q.sound(self, q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
            }
        } else {
            if (worldtype == 2) {
                q.centerprint(other, S("You need the gold keycard"));
                q.sound(self, q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
            } else if (worldtype == 1) {
                q.centerprint(other, S("You need the gold runekey"));
                q.sound(self, q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
            } else if (worldtype == 0) {
                q.centerprint(other, S("You need the gold key"));
                q.sound(self, q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
            }
        }
        return;
    }

    other.items = other.items - self.items;
    self.touch = q.func(subs.SUB_Null);
    if (!self.enemy.isNull())
        self.enemy.touch = q.func(subs.SUB_Null); // get paired door
    door_use();
}

// =============================================================================
//
// SPAWNING FUNCTIONS
//
// =============================================================================

pub fn spawn_field(fmins: Vector, fmaxs: Vector) callconv(.c) Entity {
    const trigger = q.spawn();
    trigger.movetype = q.MOVETYPE_NONE;
    trigger.solid = q.SOLID_TRIGGER;
    trigger.owner = q.me();
    trigger.touch = q.func(door_trigger_touch);

    const t1 = fmins;
    const t2 = fmaxs;
    q.setsize(trigger, t1 - q.vec(60, 60, 8), t2 + q.vec(60, 60, 8));
    return trigger;
}

pub fn EntitiesTouching(e1: Entity, e2: Entity) callconv(.c) f32 {
    const mins1 = e1.mins;
    const maxs1 = e1.maxs;
    const mins2 = e2.mins;
    const maxs2 = e2.maxs;
    if (mins1[0] > maxs2[0])
        return q.FALSE;
    if (mins1[1] > maxs2[1])
        return q.FALSE;
    if (mins1[2] > maxs2[2])
        return q.FALSE;
    if (maxs1[0] < mins2[0])
        return q.FALSE;
    if (maxs1[1] < mins2[1])
        return q.FALSE;
    if (maxs1[2] < mins2[2])
        return q.FALSE;
    return q.TRUE;
}

/// LinkDoors
pub fn LinkDoors() callconv(.c) void {
    var self = q.me();

    if (!self.enemy.isNull())
        return; // already linked by another door
    if (q.has(self.spawnflags, 4)) {
        self.enemy = self;
        self.owner = self;
        return; // don't want to link this door
    }

    var cmins = self.mins;
    var cmaxs = self.maxs;

    const starte = self;
    var t = self;

    while (true) {
        self.owner = starte; // master door

        if (self.health != 0)
            starte.health = self.health;
        if (!self.targetname.isNull())
            starte.targetname = self.targetname;
        if (self.message.neq(S("")))
            starte.message = self.message;

        t = q.find(t, .classname, self.classname);
        if (t.isNull()) {
            self.enemy = starte; // make the chain a loop

            // shootable, fired, or key doors just needed the owner/enemy links,
            // they don't spawn a field

            self = self.owner;
            q.self.set(self);

            if (self.health != 0)
                return;
            if (!self.targetname.isNull())
                return;
            if (self.items != 0)
                return;

            self.owner.set(g.trigger_field, spawn_field(cmins, cmaxs));

            return;
        }

        if (EntitiesTouching(self, t) != 0) {
            if (!t.enemy.isNull())
                q.objerror(S("cross connected doors"));

            self.enemy = t;
            self = t;
            q.self.set(self);

            const tmins = t.mins;
            const tmaxs = t.maxs;
            if (tmins[0] < cmins[0])
                cmins[0] = tmins[0];
            if (tmins[1] < cmins[1])
                cmins[1] = tmins[1];
            if (tmins[2] < cmins[2])
                cmins[2] = tmins[2];
            if (tmaxs[0] > cmaxs[0])
                cmaxs[0] = tmaxs[0];
            if (tmaxs[1] > cmaxs[1])
                cmaxs[1] = tmaxs[1];
            if (tmaxs[2] > cmaxs[2])
                cmaxs[2] = tmaxs[2];
        }
    }
}

/// QUAKED func_door (0 .5 .8) ? START_OPEN x DOOR_DONT_LINK GOLD_KEY SILVER_KEY TOGGLE
/// if two doors touch, they are assumed to be connected and operate as a unit.
///
/// TOGGLE causes the door to wait in both the start and end states for a
/// trigger event.
///
/// START_OPEN causes the door to move to its destination when spawned, and
/// operate in reverse.  It is used to temporarily or permanently close off an
/// area when triggered (not usefull for touch or takedamage doors).
///
/// Key doors are allways wait -1.
///
/// "message"       is printed when the door is touched if it is a trigger door and it hasn't been fired yet
/// "angle"         determines the opening direction
/// "targetname" if set, no touch field will be spawned and a remote button or trigger field activates the door.
/// "health"        if set, door must be shot open
/// "speed"         movement speed (100 default)
/// "wait"          wait before returning (3 default, -1 = never return)
/// "lip"           lip remaining at end of move (8 default)
/// "dmg"           damage to inflict when blocked (2 default)
/// "sounds"
/// 0)      no sound
/// 1)      stone
/// 2)      base
/// 3)      stone chain
/// 4)      screechy metal
pub export fn func_door() void {
    const self = q.me();
    const worldtype = q.world.get().get(g.worldtype);

    if (worldtype == 0) {
        _ = q.precache_sound(S("doors/medtry.wav"));
        _ = q.precache_sound(S("doors/meduse.wav"));
        self.noise3 = S("doors/medtry.wav");
        self.set(g.noise4, S("doors/meduse.wav"));
    } else if (worldtype == 1) {
        _ = q.precache_sound(S("doors/runetry.wav"));
        _ = q.precache_sound(S("doors/runeuse.wav"));
        self.noise3 = S("doors/runetry.wav");
        self.set(g.noise4, S("doors/runeuse.wav"));
    } else if (worldtype == 2) {
        _ = q.precache_sound(S("doors/basetry.wav"));
        _ = q.precache_sound(S("doors/baseuse.wav"));
        self.noise3 = S("doors/basetry.wav");
        self.set(g.noise4, S("doors/baseuse.wav"));
    } else {
        q.dprint(S("no worldtype set!\n"));
    }
    if (self.sounds == 0) {
        _ = q.precache_sound(S("misc/null.wav"));
        _ = q.precache_sound(S("misc/null.wav"));
        self.noise1 = S("misc/null.wav");
        self.noise2 = S("misc/null.wav");
    }
    if (self.sounds == 1) {
        _ = q.precache_sound(S("doors/drclos4.wav"));
        _ = q.precache_sound(S("doors/doormv1.wav"));
        self.noise1 = S("doors/drclos4.wav");
        self.noise2 = S("doors/doormv1.wav");
    }
    if (self.sounds == 2) {
        _ = q.precache_sound(S("doors/hydro1.wav"));
        _ = q.precache_sound(S("doors/hydro2.wav"));
        self.noise2 = S("doors/hydro1.wav");
        self.noise1 = S("doors/hydro2.wav");
    }
    if (self.sounds == 3) {
        _ = q.precache_sound(S("doors/stndr1.wav"));
        _ = q.precache_sound(S("doors/stndr2.wav"));
        self.noise2 = S("doors/stndr1.wav");
        self.noise1 = S("doors/stndr2.wav");
    }
    if (self.sounds == 4) {
        _ = q.precache_sound(S("doors/ddoor1.wav"));
        _ = q.precache_sound(S("doors/ddoor2.wav"));
        self.noise1 = S("doors/ddoor2.wav");
        self.noise2 = S("doors/ddoor1.wav");
    }

    subs.SetMovedir();

    self.max_health = self.health;
    self.solid = q.SOLID_BSP;
    self.movetype = q.MOVETYPE_PUSH;
    q.setorigin(self, self.origin);
    q.setmodel(self, self.model);
    self.classname = S("door");

    self.blocked = q.func(door_blocked);
    self.use = q.func(door_use);

    if (q.has(self.spawnflags, DOOR_SILVER_KEY))
        self.items = q.IT_KEY1;
    if (q.has(self.spawnflags, DOOR_GOLD_KEY))
        self.items = q.IT_KEY2;

    if (self.get(g.speed) == 0)
        self.set(g.speed, 100);
    if (self.get(g.wait) == 0)
        self.set(g.wait, 3);
    if (self.get(g.lip) == 0)
        self.set(g.lip, 8);
    if (self.get(g.dmg) == 0)
        self.set(g.dmg, 2);

    self.set(g.pos1, self.origin);
    const movedir = self.movedir;
    self.set(g.pos2, self.get(g.pos1) + q.scale(movedir, q.fabs(q.dot(movedir, self.size)) - self.get(g.lip)));

    // DOOR_START_OPEN is to allow an entity to be lighted in the closed position
    // but spawn in the open position
    if (q.has(self.spawnflags, DOOR_START_OPEN)) {
        q.setorigin(self, self.get(g.pos2));
        self.set(g.pos2, self.get(g.pos1));
        self.set(g.pos1, self.origin);
    }

    self.set(g.state, q.STATE_BOTTOM);

    if (self.health != 0) {
        self.takedamage = q.DAMAGE_YES;
        self.set(g.th_die, q.func(door_killed));
    }

    if (self.items != 0)
        self.set(g.wait, -1);

    self.touch = q.func(door_touch);

    // LinkDoors can't be done until all of the doors have been spawned, so
    // the sizes can be detected properly.
    self.think = q.func(LinkDoors);
    self.nextthink = self.ltime + 0.1;
}

// =============================================================================
//
// SECRET DOORS
//
// =============================================================================

pub const SECRET_OPEN_ONCE: f32 = 1; // stays open
pub const SECRET_1ST_LEFT: f32 = 2; // 1st move is left of arrow
pub const SECRET_1ST_DOWN: f32 = 4; // 1st move is down from arrow
pub const SECRET_NO_SHOOT: f32 = 8; // only opened by trigger
pub const SECRET_YES_SHOOT: f32 = 16; // shootable even if targeted

pub fn fd_secret_use() callconv(.c) void {
    const self = q.me();

    self.health = 10000;

    // exit if still moving around...
    if (q.vecNeq(self.origin, self.oldorigin))
        return;

    self.message = g.string_null.get(); // no more message

    subs.SUB_UseTargets(); // fire all targets / killtargets

    if (!q.has(self.spawnflags, SECRET_NO_SHOOT)) {
        self.set(g.th_pain, q.func(subs.SUB_Null));
        self.takedamage = q.DAMAGE_NO;
    }
    self.velocity = q.vec(0, 0, 0);

    // Make a sound, wait a little...

    q.sound(self, q.CHAN_VOICE, self.noise1, 1, q.ATTN_NORM);
    self.nextthink = self.ltime + 0.1;

    const temp = 1 - q.band(self.spawnflags, SECRET_1ST_LEFT); // 1 or -1
    q.makevectors(self.get(g.mangle));

    if (self.get(g.t_width) == 0) {
        if (q.has(self.spawnflags, SECRET_1ST_DOWN)) {
            self.set(g.t_width, q.fabs(q.dot(q.v_up.get(), self.size)));
        } else {
            self.set(g.t_width, q.fabs(q.dot(q.v_right.get(), self.size)));
        }
    }

    if (self.get(g.t_length) == 0)
        self.set(g.t_length, q.fabs(q.dot(q.v_forward.get(), self.size)));

    if (q.has(self.spawnflags, SECRET_1ST_DOWN)) {
        self.set(g.dest1, self.origin - q.scale(q.v_up.get(), self.get(g.t_width)));
    } else {
        self.set(g.dest1, self.origin + q.scale(q.v_right.get(), self.get(g.t_width) * temp));
    }

    self.set(g.dest2, self.get(g.dest1) + q.scale(q.v_forward.get(), self.get(g.t_length)));
    subs.SUB_CalcMove(self.get(g.dest1), self.get(g.speed), q.func(fd_secret_move1));
    q.sound(self, q.CHAN_VOICE, self.noise2, 1, q.ATTN_NORM);
}

/// Wait after first movement...
pub fn fd_secret_move1() callconv(.c) void {
    const self = q.me();
    self.nextthink = self.ltime + 1.0;
    self.think = q.func(fd_secret_move2);
    q.sound(self, q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
}

/// Start moving sideways w/sound...
pub fn fd_secret_move2() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_VOICE, self.noise2, 1, q.ATTN_NORM);
    subs.SUB_CalcMove(self.get(g.dest2), self.get(g.speed), q.func(fd_secret_move3));
}

/// Wait here until time to go back...
pub fn fd_secret_move3() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
    if (!q.has(self.spawnflags, SECRET_OPEN_ONCE)) {
        self.nextthink = self.ltime + self.get(g.wait);
        self.think = q.func(fd_secret_move4);
    }
}

/// Move backward...
pub fn fd_secret_move4() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_VOICE, self.noise2, 1, q.ATTN_NORM);
    subs.SUB_CalcMove(self.get(g.dest1), self.get(g.speed), q.func(fd_secret_move5));
}

/// Wait 1 second...
pub fn fd_secret_move5() callconv(.c) void {
    const self = q.me();
    self.nextthink = self.ltime + 1.0;
    self.think = q.func(fd_secret_move6);
    q.sound(self, q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
}

pub fn fd_secret_move6() callconv(.c) void {
    const self = q.me();
    q.sound(self, q.CHAN_VOICE, self.noise2, 1, q.ATTN_NORM);
    subs.SUB_CalcMove(self.oldorigin, self.get(g.speed), q.func(fd_secret_done));
}

pub fn fd_secret_done() callconv(.c) void {
    const self = q.me();
    if (self.targetname.isNull() or q.has(self.spawnflags, SECRET_YES_SHOOT)) {
        self.health = 10000;
        self.takedamage = q.DAMAGE_YES;
        self.set(g.th_pain, q.func(fd_secret_use));
        self.set(g.th_die, q.func(fd_secret_use));
    }
    q.sound(self, q.CHAN_NO_PHS_ADD + q.CHAN_VOICE, self.noise3, 1, q.ATTN_NORM);
}

pub fn secret_blocked() callconv(.c) void {
    const self = q.me();
    if (q.time.get() < self.get(g.attack_finished))
        return;
    self.set(g.attack_finished, q.time.get() + 0.5);
    const other = q.other.get();
    other.set(g.deathtype, S("squish"));
    combat.T_Damage(other, self, self, self.get(g.dmg));
}

/// secret_touch
///
/// Prints messages
pub fn secret_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    if (other.classname.neq(S("player")))
        return;
    if (self.get(g.attack_finished) > q.time.get())
        return;

    self.set(g.attack_finished, q.time.get() + 2);

    if (!self.message.isNull()) {
        q.centerprint(other, self.message);
        q.sound(other, q.CHAN_BODY, S("misc/talk.wav"), 1, q.ATTN_NORM);
    }
}

/// QUAKED func_door_secret (0 .5 .8) ? open_once 1st_left 1st_down no_shoot always_shoot
/// Basic secret door. Slides back, then to the side. Angle determines direction.
/// wait  = # of seconds before coming back
/// 1st_left = 1st move is left of arrow
/// 1st_down = 1st move is down from arrow
/// always_shoot = even if targeted, keep shootable
/// t_width = override WIDTH to move back (or height if going down)
/// t_length = override LENGTH to move sideways
/// "dmg"           damage to inflict when blocked (2 default)
///
/// If a secret door has a targetname, it will only be opened by it's botton
/// or trigger, not by damage.
/// "sounds"
/// 1) medieval
/// 2) metal
/// 3) base
pub export fn func_door_secret() void {
    const self = q.me();
    if (self.sounds == 0)
        self.sounds = 3;
    if (self.sounds == 1) {
        _ = q.precache_sound(S("doors/latch2.wav"));
        _ = q.precache_sound(S("doors/winch2.wav"));
        _ = q.precache_sound(S("doors/drclos4.wav"));
        self.noise1 = S("doors/latch2.wav");
        self.noise2 = S("doors/winch2.wav");
        self.noise3 = S("doors/drclos4.wav");
    }
    if (self.sounds == 2) {
        _ = q.precache_sound(S("doors/airdoor1.wav"));
        _ = q.precache_sound(S("doors/airdoor2.wav"));
        self.noise2 = S("doors/airdoor1.wav");
        self.noise1 = S("doors/airdoor2.wav");
        self.noise3 = S("doors/airdoor2.wav");
    }
    if (self.sounds == 3) {
        _ = q.precache_sound(S("doors/basesec1.wav"));
        _ = q.precache_sound(S("doors/basesec2.wav"));
        self.noise2 = S("doors/basesec1.wav");
        self.noise1 = S("doors/basesec2.wav");
        self.noise3 = S("doors/basesec2.wav");
    }

    if (self.get(g.dmg) == 0)
        self.set(g.dmg, 2);

    // Magic formula...
    self.set(g.mangle, self.angles);
    self.angles = q.vec(0, 0, 0);
    self.solid = q.SOLID_BSP;
    self.movetype = q.MOVETYPE_PUSH;
    self.classname = S("door");
    q.setmodel(self, self.model);
    q.setorigin(self, self.origin);

    self.touch = q.func(secret_touch);
    self.blocked = q.func(secret_blocked);
    self.set(g.speed, 50);
    self.use = q.func(fd_secret_use);
    if (self.targetname.isNull() or q.has(self.spawnflags, SECRET_YES_SHOOT)) {
        self.health = 10000;
        self.takedamage = q.DAMAGE_YES;
        self.set(g.th_pain, q.func(fd_secret_use));
    }
    self.oldorigin = self.origin;
    if (self.get(g.wait) == 0)
        self.set(g.wait, 5); // 5 seconds before closing
}
