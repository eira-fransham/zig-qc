//! triggers.qc
//!
//! Copyright (C) 1996-1997  Id Software, Inc.
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation; either version 2 of the License, or (at your option)
//! any later version. See gnu.txt.

const q = @import("defs.zig");
const S = q.S;
const Entity = q.Entity;
const Vector = q.Vector;

const combat = @import("combat.zig");
const subs = @import("subs.zig");

pub const stemp = q.userGlobal(Entity, "stemp");
pub const otemp = q.userGlobal(Entity, "otemp");
pub const s = q.userGlobal(Entity, "s");
pub const old = q.userGlobal(Entity, "old");

pub fn trigger_reactivate() callconv(.c) void {
    q.me().solid = q.SOLID_TRIGGER;
}

//=============================================================================

pub const SPAWNFLAG_NOMESSAGE: f32 = 1;
pub const SPAWNFLAG_NOTOUCH: f32 = 1;

/// the wait time has passed, so set back up for another activation
pub fn multi_wait() callconv(.c) void {
    const self = q.me();
    if (self.max_health != 0) {
        self.health = self.max_health;
        self.takedamage = q.DAMAGE_YES;
        self.solid = q.SOLID_BBOX;
    }
}

/// the trigger was just touched/killed/used
/// self.enemy should be set to the activator so it can be held through a delay
/// so wait for the delay time before firing
pub fn multi_trigger() callconv(.c) void {
    const self = q.me();
    if (self.nextthink > q.time.get()) {
        return; // allready been triggered
    }

    if (self.classname.eql(S("trigger_secret"))) {
        if (self.enemy.classname.neq(S("player")))
            return;
        q.found_secrets.set(q.found_secrets.get() + 1);
        q.WriteByte(q.MSG_ALL, q.SVC_FOUNDSECRET);
    }

    if (!self.noise.isNull())
        q.sound(self, q.CHAN_VOICE, self.noise, 1, q.ATTN_NORM);

    // don't trigger again until reset
    self.takedamage = q.DAMAGE_NO;

    q.activator.set(self.enemy);

    subs.SUB_UseTargets();

    if (self.get(q.wait) > 0) {
        self.think = q.func(multi_wait);
        self.nextthink = q.time.get() + self.get(q.wait);
    } else { // we can't just remove (self) here, because this is a touch function
        // called wheil C code is looping through area links...
        self.touch = q.func(subs.SUB_Null);
        self.nextthink = q.time.get() + 0.1;
        self.think = q.func(subs.SUB_Remove);
    }
}

pub fn multi_killed() callconv(.c) void {
    q.me().enemy = q.damage_attacker.get();
    multi_trigger();
}

pub fn multi_use() callconv(.c) void {
    q.me().enemy = q.activator.get();
    multi_trigger();
}

pub fn multi_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    if (other.classname.neq(S("player")))
        return;

    // if the trigger has an angles field, check player's facing direction
    if (!q.veq(self.movedir, q.vec(0, 0, 0))) {
        q.makevectors(other.angles);
        if (q.dot(q.v_forward.get(), self.movedir) < 0)
            return; // not facing the right way
    }

    self.enemy = other;
    multi_trigger();
}

/// QUAKED trigger_multiple (.5 .5 .5) ? notouch
/// Variable sized repeatable trigger.  Must be targeted at one or more
/// entities.  If "health" is set, the trigger must be killed to activate each
/// time.
/// If "delay" is set, the trigger waits some time after activating before firing.
/// "wait" : Seconds between triggerings. (.2 default)
/// If notouch is set, the trigger is only fired by other entities, not by touching.
/// NOTOUCH has been obsoleted by trigger_relay!
/// sounds
/// 1)  secret
/// 2)  beep beep
/// 3)  large switch
/// 4)
/// set "message" to text string
pub export fn trigger_multiple() void {
    const self = q.me();
    if (self.sounds == 1) {
        _ = q.precache_sound(S("misc/secret.wav"));
        self.noise = S("misc/secret.wav");
    } else if (self.sounds == 2) {
        _ = q.precache_sound(S("misc/talk.wav"));
        self.noise = S("misc/talk.wav");
    } else if (self.sounds == 3) {
        _ = q.precache_sound(S("misc/trigger1.wav"));
        self.noise = S("misc/trigger1.wav");
    }

    if (self.get(q.wait) == 0)
        self.set(q.wait, 0.2);
    self.use = q.func(multi_use);

    subs.InitTrigger();

    if (self.health != 0) {
        if (q.has(self.spawnflags, SPAWNFLAG_NOTOUCH))
            q.objerror(S("health and notouch don't make sense\n"));
        self.max_health = self.health;
        self.set(q.th_die, q.func(multi_killed));
        self.takedamage = q.DAMAGE_YES;
        self.solid = q.SOLID_BBOX;
        q.setorigin(self, self.origin); // make sure it links into the world
    } else {
        if (!q.has(self.spawnflags, SPAWNFLAG_NOTOUCH)) {
            self.touch = q.func(multi_touch);
        }
    }
}

/// QUAKED trigger_once (.5 .5 .5) ? notouch
/// Variable sized trigger. Triggers once, then removes itself.  You must set
/// the key "target" to the name of another object in the level that has a
/// matching "targetname".  If "health" is set, the trigger must be killed to
/// activate.
/// If notouch is set, the trigger is only fired by other entities, not by touching.
/// if "killtarget" is set, any objects that have a matching "target" will be
/// removed when the trigger is fired.
/// if "angle" is set, the trigger will only fire when someone is facing the
/// direction of the angle.  Use "360" for an angle of 0.
/// sounds
/// 1)  secret
/// 2)  beep beep
/// 3)  large switch
/// 4)
/// set "message" to text string
pub export fn trigger_once() void {
    q.me().set(q.wait, -1);
    trigger_multiple();
}

//=============================================================================

/// QUAKED trigger_relay (.5 .5 .5) (-8 -8 -8) (8 8 8)
/// This fixed size trigger cannot be touched, it can only be fired by other
/// events.  It can contain killtargets, targets, delays, and messages.
pub export fn trigger_relay() void {
    q.me().use = q.func(subs.SUB_UseTargets);
}

//=============================================================================

/// QUAKED trigger_secret (.5 .5 .5) ?
/// secret counter trigger
/// sounds
/// 1)  secret
/// 2)  beep beep
/// 3)
/// 4)
/// set "message" to text string
pub export fn trigger_secret() void {
    const self = q.me();
    q.total_secrets.set(q.total_secrets.get() + 1);
    self.set(q.wait, -1);
    if (self.message.isNull())
        self.message = S("You found a secret area!");
    if (self.sounds == 0)
        self.sounds = 1;

    if (self.sounds == 1) {
        _ = q.precache_sound(S("misc/secret.wav"));
        self.noise = S("misc/secret.wav");
    } else if (self.sounds == 2) {
        _ = q.precache_sound(S("misc/talk.wav"));
        self.noise = S("misc/talk.wav");
    }

    trigger_multiple();
}

//=============================================================================

pub fn counter_use() callconv(.c) void {
    const self = q.me();
    const activator = q.activator.get();

    self.set(q.count, self.get(q.count) - 1);
    if (self.get(q.count) < 0)
        return;

    if (self.get(q.count) != 0) {
        if (activator.classname.eql(S("player")) and q.band(self.spawnflags, SPAWNFLAG_NOMESSAGE) == 0) {
            if (self.get(q.count) >= 4) {
                q.centerprint(activator, S("There are more to go..."));
            } else if (self.get(q.count) == 3) {
                q.centerprint(activator, S("Only 3 more to go..."));
            } else if (self.get(q.count) == 2) {
                q.centerprint(activator, S("Only 2 more to go..."));
            } else {
                q.centerprint(activator, S("Only 1 more to go..."));
            }
        }
        return;
    }

    if (activator.classname.eql(S("player")) and q.band(self.spawnflags, SPAWNFLAG_NOMESSAGE) == 0)
        q.centerprint(activator, S("Sequence completed!"));
    self.enemy = activator;
    multi_trigger();
}

/// QUAKED trigger_counter (.5 .5 .5) ? nomessage
/// Acts as an intermediary for an action that takes multiple inputs.
///
/// If nomessage is not set, t will print "1 more.. " etc when triggered and
/// "sequence complete" when finished.
///
/// After the counter has been triggered "count" times (default 2), it will
/// fire all of it's targets and remove itself.
pub export fn trigger_counter() void {
    const self = q.me();
    self.set(q.wait, -1);
    if (self.get(q.count) == 0)
        self.set(q.count, 2);

    self.use = q.func(counter_use);
}

// ==============================================================================
//
// TELEPORT TRIGGERS
//
// ==============================================================================

pub const PLAYER_ONLY: f32 = 1;
pub const SILENT: f32 = 2;

pub fn play_teleport() callconv(.c) void {
    const self = q.me();

    const v = q.random() * 5;
    const tmpstr = if (v < 1)
        S("misc/r_tele1.wav")
    else if (v < 2)
        S("misc/r_tele2.wav")
    else if (v < 3)
        S("misc/r_tele3.wav")
    else if (v < 4)
        S("misc/r_tele4.wav")
    else
        S("misc/r_tele5.wav");

    q.sound(self, q.CHAN_VOICE, tmpstr, 1, q.ATTN_NORM);
    q.remove(self);
}

pub fn spawn_tfog(org: Vector) callconv(.c) void {
    s.set(q.spawn());
    const e = s.get();
    e.origin = org;
    e.nextthink = q.time.get() + 0.2;
    e.think = q.func(play_teleport);

    q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
    q.WriteByte(q.MSG_MULTICAST, q.TE_TELEPORT);
    q.WriteCoord(q.MSG_MULTICAST, org[0]);
    q.WriteCoord(q.MSG_MULTICAST, org[1]);
    q.WriteCoord(q.MSG_MULTICAST, org[2]);
    q.multicast(org, q.MULTICAST_PHS);
}

pub fn tdeath_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (other.eql(self.owner))
        return;

    // frag anyone who teleports in on top of an invincible player
    if (other.classname.eql(S("player"))) {
        if (other.get(q.invincible_finished) > q.time.get() and
            self.owner.get(q.invincible_finished) > q.time.get())
        {
            self.classname = S("teledeath3");
            other.set(q.invincible_finished, 0);
            self.owner.set(q.invincible_finished, 0);
            combat.T_Damage(other, self, self, 50000);
            const other2 = self.owner;
            self.owner = other;
            combat.T_Damage(other2, self, self, 50000);
        }

        if (other.get(q.invincible_finished) > q.time.get()) {
            self.classname = S("teledeath2");
            combat.T_Damage(self.owner, self, self, 50000);
            return;
        }
    }

    if (other.health != 0) {
        combat.T_Damage(other, self, self, 50000);
    }
}

pub fn spawn_tdeath(org: Vector, death_owner: Entity) callconv(.c) void {
    const death = q.spawn();
    death.classname = S("teledeath");
    death.movetype = q.MOVETYPE_NONE;
    death.solid = q.SOLID_TRIGGER;
    death.angles = q.vec(0, 0, 0);
    q.setsize(death, death_owner.mins - q.vec(1, 1, 1), death_owner.maxs + q.vec(1, 1, 1));
    q.setorigin(death, org);
    death.touch = q.func(tdeath_touch);
    death.nextthink = q.time.get() + 0.2;
    death.think = q.func(subs.SUB_Remove);
    death.owner = death_owner;

    q.force_retouch.set(2); // make sure even still objects get hit
}

pub fn teleport_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (!self.targetname.isNull()) {
        if (self.nextthink < q.time.get()) {
            return; // not fired yet
        }
    }

    if (q.has(self.spawnflags, PLAYER_ONLY)) {
        if (other.classname.neq(S("player")))
            return;
    }

    // only teleport living creatures
    if (other.health <= 0 or other.solid != q.SOLID_SLIDEBOX)
        return;

    subs.SUB_UseTargets();

    // put a tfog where the player was
    spawn_tfog(other.origin);

    const t = q.find(q.world.get(), .targetname, self.target);
    if (t.isNull())
        q.objerror(S("couldn't find target"));

    // spawn a tfog flash in front of the destination
    q.makevectors(t.get(q.mangle));
    const org = t.origin + q.scale(q.v_forward.get(), 32);

    spawn_tfog(org);
    spawn_tdeath(t.origin, other);

    // move the player and lock him down for a little while
    if (other.health == 0) {
        other.origin = t.origin;
        const vel = other.velocity;
        other.velocity = q.scale(q.v_forward.get(), vel[0]) + q.scale(q.v_forward.get(), vel[1]);
        return;
    }

    q.setorigin(other, t.origin);
    other.angles = t.get(q.mangle);
    if (other.classname.eql(S("player"))) {
        other.fixangle = 1; // turn this way immediately
        other.teleport_time = q.time.get() + 0.7;
        if (q.has(other.flags, q.FL_ONGROUND))
            other.flags = other.flags - q.FL_ONGROUND;
        other.velocity = q.scale(q.v_forward.get(), 300);
    }
    // `other.flags - other.flags & FL_ONGROUND`: `&` binds tighter than `-`
    // in QuakeC.
    other.flags = other.flags - q.band(other.flags, q.FL_ONGROUND);
}

/// QUAKED info_teleport_destination (.5 .5 .5) (-8 -8 -8) (8 8 32)
/// This is the destination marker for a teleporter.  It should have a
/// "targetname" field with the same value as a teleporter's "target" field.
pub export fn info_teleport_destination() void {
    const self = q.me();
    // this does nothing, just serves as a target spot
    self.set(q.mangle, self.angles);
    self.angles = q.vec(0, 0, 0);
    self.model = S("");
    self.origin = self.origin + q.vec(0, 0, 27);
    if (self.targetname.isNull())
        q.objerror(S("no targetname"));
}

pub fn teleport_use() callconv(.c) void {
    const self = q.me();
    self.nextthink = q.time.get() + 0.2;
    q.force_retouch.set(2); // make sure even still objects get hit
    self.think = q.func(subs.SUB_Null);
}

/// QUAKED trigger_teleport (.5 .5 .5) ? PLAYER_ONLY SILENT
/// Any object touching this will be transported to the corresponding
/// info_teleport_destination entity. You must set the "target" field, and
/// create an object with a "targetname" field that matches.
///
/// If the trigger_teleport has a targetname, it will only teleport entities
/// when it has been fired.
pub export fn trigger_teleport() void {
    const self = q.me();

    subs.InitTrigger();
    self.touch = q.func(teleport_touch);
    // find the destination
    if (self.target.isNull())
        q.objerror(S("no target"));
    self.use = q.func(teleport_use);

    if (!q.has(self.spawnflags, SILENT)) {
        _ = q.precache_sound(S("ambience/hum1.wav"));
        const o = q.scale(@as(Vector, self.mins) + self.maxs, 0.5);
        q.ambientsound(o, S("ambience/hum1.wav"), 0.5, q.ATTN_STATIC);
    }
}

// ==============================================================================
//
// trigger_setskill
//
// ==============================================================================

/// QUAKED trigger_setskill (.5 .5 .5) ?
/// sets skill level to the value of "message".
/// Only used on start map.
pub export fn trigger_setskill() void {
    q.remove(q.me());
}

// ==============================================================================
//
// ONLY REGISTERED TRIGGERS
//
// ==============================================================================

pub fn trigger_onlyregistered_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    if (other.classname.neq(S("player")))
        return;
    if (self.get(q.attack_finished) > q.time.get())
        return;

    self.set(q.attack_finished, q.time.get() + 2);
    if (q.cvar(S("registered")) != 0) {
        self.message = S("");
        subs.SUB_UseTargets();
        q.remove(self);
    } else {
        if (self.message.neq(S(""))) {
            q.centerprint(other, self.message);
            q.sound(other, q.CHAN_BODY, S("misc/talk.wav"), 1, q.ATTN_NORM);
        }
    }
}

/// QUAKED trigger_onlyregistered (.5 .5 .5) ?
/// Only fires if playing the registered version, otherwise prints the message
pub export fn trigger_onlyregistered() void {
    _ = q.precache_sound(S("misc/talk.wav"));
    subs.InitTrigger();
    q.me().touch = q.func(trigger_onlyregistered_touch);
}

//============================================================================

pub fn hurt_on() callconv(.c) void {
    const self = q.me();
    self.solid = q.SOLID_TRIGGER;
    self.nextthink = -1;
}

pub fn hurt_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    if (other.takedamage != 0) {
        self.solid = q.SOLID_NOT;
        combat.T_Damage(other, self, self, self.get(q.dmg));
        self.think = q.func(hurt_on);
        self.nextthink = q.time.get() + 1;
    }

    return;
}

/// QUAKED trigger_hurt (.5 .5 .5) ?
/// Any object touching this will be hurt
/// set dmg to damage amount
/// defalt dmg = 5
pub export fn trigger_hurt() void {
    const self = q.me();
    subs.InitTrigger();
    self.touch = q.func(hurt_touch);
    if (self.get(q.dmg) == 0)
        self.set(q.dmg, 5);
}

//============================================================================

pub const PUSH_ONCE: f32 = 1;

pub fn trigger_push_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    if (other.classname.eql(S("grenade"))) {
        other.velocity = q.scale(self.movedir, self.get(q.speed) * 10);
    } else if (other.health > 0) {
        other.velocity = q.scale(self.movedir, self.get(q.speed) * 10);
        if (other.classname.eql(S("player"))) {
            if (other.get(q.fly_sound) < q.time.get()) {
                other.set(q.fly_sound, q.time.get() + 1.5);
                q.sound(other, q.CHAN_AUTO, S("ambience/windfly.wav"), 1, q.ATTN_NORM);
            }
        }
    }
    if (q.has(self.spawnflags, PUSH_ONCE))
        q.remove(self);
}

/// QUAKED trigger_push (.5 .5 .5) ? PUSH_ONCE
/// Pushes the player
pub export fn trigger_push() void {
    const self = q.me();
    subs.InitTrigger();
    _ = q.precache_sound(S("ambience/windfly.wav"));
    self.touch = q.func(trigger_push_touch);
    if (self.get(q.speed) == 0)
        self.set(q.speed, 1000);
}

//============================================================================

pub fn trigger_monsterjump_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    // `other.flags & (...) != FL_MONSTER`: `&` binds tighter than `!=` in
    // QuakeC.
    if (q.band(other.flags, q.bor(q.bor(q.FL_MONSTER, q.FL_FLY), q.FL_SWIM)) != q.FL_MONSTER)
        return;

    // set XY even if not on ground, so the jump will clear lips
    var vel = other.velocity;
    vel[0] = self.movedir[0] * self.get(q.speed);
    vel[1] = self.movedir[1] * self.get(q.speed);
    other.velocity = vel;

    if (!q.has(other.flags, q.FL_ONGROUND))
        return;

    other.flags = other.flags - q.FL_ONGROUND;

    vel = other.velocity;
    vel[2] = self.get(q.height);
    other.velocity = vel;
}

/// QUAKED trigger_monsterjump (.5 .5 .5) ?
/// Walking monsters that touch this will jump in the direction of the
/// trigger's angle
/// "speed" default to 200, the speed thrown forward
/// "height" default to 200, the speed thrown upwards
pub export fn trigger_monsterjump() void {
    const self = q.me();
    if (self.get(q.speed) == 0)
        self.set(q.speed, 200);
    if (self.get(q.height) == 0)
        self.set(q.height, 200);
    if (q.veq(self.angles, q.vec(0, 0, 0)))
        self.angles = q.vec(0, 360, 0);
    subs.InitTrigger();
    self.touch = q.func(trigger_monsterjump_touch);
}
