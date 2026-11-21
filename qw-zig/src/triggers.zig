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
    q.me().set(.solid, q.SOLID_TRIGGER);
}

//=============================================================================

pub const SPAWNFLAG_NOMESSAGE: f32 = 1;
pub const SPAWNFLAG_NOTOUCH: f32 = 1;

/// the wait time has passed, so set back up for another activation
pub fn multi_wait() callconv(.c) void {
    const self = q.me();
    if (self.get(.max_health) != 0) {
        self.set(.health, self.get(.max_health));
        self.set(.takedamage, q.DAMAGE_YES);
        self.set(.solid, q.SOLID_BBOX);
    }
}

/// the trigger was just touched/killed/used
/// self.enemy should be set to the activator so it can be held through a delay
/// so wait for the delay time before firing
pub fn multi_trigger() callconv(.c) void {
    const self = q.me();
    if (self.get(.nextthink) > q.time.get()) {
        return; // allready been triggered
    }

    if (self.get(.classname).eql(S("trigger_secret"))) {
        if (self.get(.enemy).get(.classname).neq(S("player")))
            return;
        q.found_secrets.set(q.found_secrets.get() + 1);
        q.WriteByte(q.MSG_ALL, q.SVC_FOUNDSECRET);
    }

    if (!self.get(.noise).isNull())
        q.sound(self, q.CHAN_VOICE, self.get(.noise), 1, q.ATTN_NORM);

    // don't trigger again until reset
    self.set(.takedamage, q.DAMAGE_NO);

    q.activator.set(self.get(.enemy));

    subs.SUB_UseTargets();

    if (self.get(q.wait) > 0) {
        self.set(.think, q.func(multi_wait));
        self.set(.nextthink, q.time.get() + self.get(q.wait));
    } else { // we can't just remove (self) here, because this is a touch function
        // called wheil C code is looping through area links...
        self.set(.touch, q.func(subs.SUB_Null));
        self.set(.nextthink, q.time.get() + 0.1);
        self.set(.think, q.func(subs.SUB_Remove));
    }
}

pub fn multi_killed() callconv(.c) void {
    q.me().set(.enemy, q.damage_attacker.get());
    multi_trigger();
}

pub fn multi_use() callconv(.c) void {
    q.me().set(.enemy, q.activator.get());
    multi_trigger();
}

pub fn multi_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    if (other.get(.classname).neq(S("player")))
        return;

    // if the trigger has an angles field, check player's facing direction
    if (!q.veq(self.get(.movedir), q.vec(0, 0, 0))) {
        q.makevectors(other.get(.angles));
        if (q.dot(q.v_forward.get(), self.get(.movedir)) < 0)
            return; // not facing the right way
    }

    self.set(.enemy, other);
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
    if (self.get(.sounds) == 1) {
        _ = q.precache_sound(S("misc/secret.wav"));
        self.set(.noise, S("misc/secret.wav"));
    } else if (self.get(.sounds) == 2) {
        _ = q.precache_sound(S("misc/talk.wav"));
        self.set(.noise, S("misc/talk.wav"));
    } else if (self.get(.sounds) == 3) {
        _ = q.precache_sound(S("misc/trigger1.wav"));
        self.set(.noise, S("misc/trigger1.wav"));
    }

    if (self.get(q.wait) == 0)
        self.set(q.wait, 0.2);
    self.set(.use, q.func(multi_use));

    subs.InitTrigger();

    if (self.get(.health) != 0) {
        if (q.has(self.get(.spawnflags), SPAWNFLAG_NOTOUCH))
            q.objerror(S("health and notouch don't make sense\n"));
        self.set(.max_health, self.get(.health));
        self.set(q.th_die, q.func(multi_killed));
        self.set(.takedamage, q.DAMAGE_YES);
        self.set(.solid, q.SOLID_BBOX);
        q.setorigin(self, self.get(.origin)); // make sure it links into the world
    } else {
        if (!q.has(self.get(.spawnflags), SPAWNFLAG_NOTOUCH)) {
            self.set(.touch, q.func(multi_touch));
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
    q.me().set(.use, q.func(subs.SUB_UseTargets));
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
    if (self.get(.message).isNull())
        self.set(.message, S("You found a secret area!"));
    if (self.get(.sounds) == 0)
        self.set(.sounds, 1);

    if (self.get(.sounds) == 1) {
        _ = q.precache_sound(S("misc/secret.wav"));
        self.set(.noise, S("misc/secret.wav"));
    } else if (self.get(.sounds) == 2) {
        _ = q.precache_sound(S("misc/talk.wav"));
        self.set(.noise, S("misc/talk.wav"));
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
        if (activator.get(.classname).eql(S("player")) and q.band(self.get(.spawnflags), SPAWNFLAG_NOMESSAGE) == 0) {
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

    if (activator.get(.classname).eql(S("player")) and q.band(self.get(.spawnflags), SPAWNFLAG_NOMESSAGE) == 0)
        q.centerprint(activator, S("Sequence completed!"));
    self.set(.enemy, activator);
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

    self.set(.use, q.func(counter_use));
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
    e.set(.origin, org);
    e.set(.nextthink, q.time.get() + 0.2);
    e.set(.think, q.func(play_teleport));

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

    if (other.eql(self.get(.owner)))
        return;

    // frag anyone who teleports in on top of an invincible player
    if (other.get(.classname).eql(S("player"))) {
        if (other.get(q.invincible_finished) > q.time.get() and
            self.get(.owner).get(q.invincible_finished) > q.time.get())
        {
            self.set(.classname, S("teledeath3"));
            other.set(q.invincible_finished, 0);
            self.get(.owner).set(q.invincible_finished, 0);
            combat.T_Damage(other, self, self, 50000);
            const other2 = self.get(.owner);
            self.set(.owner, other);
            combat.T_Damage(other2, self, self, 50000);
        }

        if (other.get(q.invincible_finished) > q.time.get()) {
            self.set(.classname, S("teledeath2"));
            combat.T_Damage(self.get(.owner), self, self, 50000);
            return;
        }
    }

    if (other.get(.health) != 0) {
        combat.T_Damage(other, self, self, 50000);
    }
}

pub fn spawn_tdeath(org: Vector, death_owner: Entity) callconv(.c) void {
    const death = q.spawn();
    death.set(.classname, S("teledeath"));
    death.set(.movetype, q.MOVETYPE_NONE);
    death.set(.solid, q.SOLID_TRIGGER);
    death.set(.angles, q.vec(0, 0, 0));
    q.setsize(death, death_owner.get(.mins) - q.vec(1, 1, 1), death_owner.get(.maxs) + q.vec(1, 1, 1));
    q.setorigin(death, org);
    death.set(.touch, q.func(tdeath_touch));
    death.set(.nextthink, q.time.get() + 0.2);
    death.set(.think, q.func(subs.SUB_Remove));
    death.set(.owner, death_owner);

    q.force_retouch.set(2); // make sure even still objects get hit
}

pub fn teleport_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (!self.get(.targetname).isNull()) {
        if (self.get(.nextthink) < q.time.get()) {
            return; // not fired yet
        }
    }

    if (q.has(self.get(.spawnflags), PLAYER_ONLY)) {
        if (other.get(.classname).neq(S("player")))
            return;
    }

    // only teleport living creatures
    if (other.get(.health) <= 0 or other.get(.solid) != q.SOLID_SLIDEBOX)
        return;

    subs.SUB_UseTargets();

    // put a tfog where the player was
    spawn_tfog(other.get(.origin));

    const t = q.find(q.world.get(), q.fieldRef(.targetname), self.get(.target));
    if (t.isNull())
        q.objerror(S("couldn't find target"));

    // spawn a tfog flash in front of the destination
    q.makevectors(t.get(q.mangle));
    const org = t.get(.origin) + q.scale(q.v_forward.get(), 32);

    spawn_tfog(org);
    spawn_tdeath(t.get(.origin), other);

    // move the player and lock him down for a little while
    if (other.get(.health) == 0) {
        other.set(.origin, t.get(.origin));
        const vel = other.get(.velocity);
        other.set(.velocity, q.scale(q.v_forward.get(), vel[0]) + q.scale(q.v_forward.get(), vel[1]));
        return;
    }

    q.setorigin(other, t.get(.origin));
    other.set(.angles, t.get(q.mangle));
    if (other.get(.classname).eql(S("player"))) {
        other.set(.fixangle, 1); // turn this way immediately
        other.set(.teleport_time, q.time.get() + 0.7);
        if (q.has(other.get(.flags), q.FL_ONGROUND))
            other.set(.flags, other.get(.flags) - q.FL_ONGROUND);
        other.set(.velocity, q.scale(q.v_forward.get(), 300));
    }
    // `other.flags - other.flags & FL_ONGROUND`: `&` binds tighter than `-`
    // in QuakeC.
    other.set(.flags, other.get(.flags) - q.band(other.get(.flags), q.FL_ONGROUND));
}

/// QUAKED info_teleport_destination (.5 .5 .5) (-8 -8 -8) (8 8 32)
/// This is the destination marker for a teleporter.  It should have a
/// "targetname" field with the same value as a teleporter's "target" field.
pub export fn info_teleport_destination() void {
    const self = q.me();
    // this does nothing, just serves as a target spot
    self.set(q.mangle, self.get(.angles));
    self.set(.angles, q.vec(0, 0, 0));
    self.set(.model, S(""));
    self.set(.origin, self.get(.origin) + q.vec(0, 0, 27));
    if (self.get(.targetname).isNull())
        q.objerror(S("no targetname"));
}

pub fn teleport_use() callconv(.c) void {
    const self = q.me();
    self.set(.nextthink, q.time.get() + 0.2);
    q.force_retouch.set(2); // make sure even still objects get hit
    self.set(.think, q.func(subs.SUB_Null));
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
    self.set(.touch, q.func(teleport_touch));
    // find the destination
    if (self.get(.target).isNull())
        q.objerror(S("no target"));
    self.set(.use, q.func(teleport_use));

    if (!q.has(self.get(.spawnflags), SILENT)) {
        _ = q.precache_sound(S("ambience/hum1.wav"));
        const o = q.scale(self.get(.mins) + self.get(.maxs), 0.5);
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
    if (other.get(.classname).neq(S("player")))
        return;
    if (self.get(q.attack_finished) > q.time.get())
        return;

    self.set(q.attack_finished, q.time.get() + 2);
    if (q.cvar(S("registered")) != 0) {
        self.set(.message, S(""));
        subs.SUB_UseTargets();
        q.remove(self);
    } else {
        if (self.get(.message).neq(S(""))) {
            q.centerprint(other, self.get(.message));
            q.sound(other, q.CHAN_BODY, S("misc/talk.wav"), 1, q.ATTN_NORM);
        }
    }
}

/// QUAKED trigger_onlyregistered (.5 .5 .5) ?
/// Only fires if playing the registered version, otherwise prints the message
pub export fn trigger_onlyregistered() void {
    _ = q.precache_sound(S("misc/talk.wav"));
    subs.InitTrigger();
    q.me().set(.touch, q.func(trigger_onlyregistered_touch));
}

//============================================================================

pub fn hurt_on() callconv(.c) void {
    const self = q.me();
    self.set(.solid, q.SOLID_TRIGGER);
    self.set(.nextthink, -1);
}

pub fn hurt_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    if (other.get(.takedamage) != 0) {
        self.set(.solid, q.SOLID_NOT);
        combat.T_Damage(other, self, self, self.get(q.dmg));
        self.set(.think, q.func(hurt_on));
        self.set(.nextthink, q.time.get() + 1);
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
    self.set(.touch, q.func(hurt_touch));
    if (self.get(q.dmg) == 0)
        self.set(q.dmg, 5);
}

//============================================================================

pub const PUSH_ONCE: f32 = 1;

pub fn trigger_push_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    if (other.get(.classname).eql(S("grenade"))) {
        other.set(.velocity, q.scale(self.get(.movedir), self.get(q.speed) * 10));
    } else if (other.get(.health) > 0) {
        other.set(.velocity, q.scale(self.get(.movedir), self.get(q.speed) * 10));
        if (other.get(.classname).eql(S("player"))) {
            if (other.get(q.fly_sound) < q.time.get()) {
                other.set(q.fly_sound, q.time.get() + 1.5);
                q.sound(other, q.CHAN_AUTO, S("ambience/windfly.wav"), 1, q.ATTN_NORM);
            }
        }
    }
    if (q.has(self.get(.spawnflags), PUSH_ONCE))
        q.remove(self);
}

/// QUAKED trigger_push (.5 .5 .5) ? PUSH_ONCE
/// Pushes the player
pub export fn trigger_push() void {
    const self = q.me();
    subs.InitTrigger();
    _ = q.precache_sound(S("ambience/windfly.wav"));
    self.set(.touch, q.func(trigger_push_touch));
    if (self.get(q.speed) == 0)
        self.set(q.speed, 1000);
}

//============================================================================

pub fn trigger_monsterjump_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    // `other.flags & (...) != FL_MONSTER`: `&` binds tighter than `!=` in
    // QuakeC.
    if (q.band(other.get(.flags), q.bor(q.bor(q.FL_MONSTER, q.FL_FLY), q.FL_SWIM)) != q.FL_MONSTER)
        return;

    // set XY even if not on ground, so the jump will clear lips
    var vel = other.get(.velocity);
    vel[0] = self.get(.movedir)[0] * self.get(q.speed);
    vel[1] = self.get(.movedir)[1] * self.get(q.speed);
    other.set(.velocity, vel);

    if (!q.has(other.get(.flags), q.FL_ONGROUND))
        return;

    other.set(.flags, other.get(.flags) - q.FL_ONGROUND);

    vel = other.get(.velocity);
    vel[2] = self.get(q.height);
    other.set(.velocity, vel);
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
    if (q.veq(self.get(.angles), q.vec(0, 0, 0)))
        self.set(.angles, q.vec(0, 360, 0));
    subs.InitTrigger();
    self.set(.touch, q.func(trigger_monsterjump_touch));
}
