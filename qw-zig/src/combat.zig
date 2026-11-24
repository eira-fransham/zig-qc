//! combat.qc
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
const Entity = q.Entity;
const String = q.String;

const client = @import("client.zig");
const subs = @import("subs.zig");

//============================================================================

/// CanDamage
///
/// Returns true if the inflictor can directly damage the target.  Used for
/// explosions and melee attacks.
pub fn CanDamage(targ: Entity, inflictor: Entity) callconv(.c) f32 {
    const self = q.me();
    // bmodels need special checking because their origin is 0,0,0
    if (targ.movetype == q.MOVETYPE_PUSH) {
        q.traceline(inflictor.origin, q.scale(@as(q.Vector, targ.absmin) + targ.absmax, 0.5), q.TRUE, self);
        if (q.trace_fraction.get() == 1)
            return q.TRUE;
        if (q.trace_ent.get().eql(targ))
            return q.TRUE;
        return q.FALSE;
    }

    q.traceline(inflictor.origin, targ.origin, q.TRUE, self);
    if (q.trace_fraction.get() == 1)
        return q.TRUE;
    q.traceline(inflictor.origin, targ.origin + q.vec(15, 15, 0), q.TRUE, self);
    if (q.trace_fraction.get() == 1)
        return q.TRUE;
    q.traceline(inflictor.origin, targ.origin + q.vec(-15, -15, 0), q.TRUE, self);
    if (q.trace_fraction.get() == 1)
        return q.TRUE;
    q.traceline(inflictor.origin, targ.origin + q.vec(-15, 15, 0), q.TRUE, self);
    if (q.trace_fraction.get() == 1)
        return q.TRUE;
    q.traceline(inflictor.origin, targ.origin + q.vec(15, -15, 0), q.TRUE, self);
    if (q.trace_fraction.get() == 1)
        return q.TRUE;

    return q.FALSE;
}

/// Killed
pub fn Killed(targ: Entity, attacker: Entity) callconv(.c) void {
    const oself = q.self.get();
    q.self.set(targ);
    const self = targ;

    if (self.health < -99)
        self.health = -99; // don't let sbar look bad if a player

    if (self.movetype == q.MOVETYPE_PUSH or self.movetype == q.MOVETYPE_NONE) { // doors, triggers, etc
        self.get(g.th_die).?();
        q.self.set(oself);
        return;
    }

    self.enemy = attacker;

    // bump the monster counter
    if (q.has(self.flags, q.FL_MONSTER)) {
        q.killed_monsters.set(q.killed_monsters.get() + 1);
        q.WriteByte(q.MSG_ALL, q.SVC_KILLEDMONSTER);
    }

    client.ClientObituary(self, attacker);

    self.takedamage = q.DAMAGE_NO;
    self.touch = q.func(subs.SUB_Null);
    self.effects = 0;

    // (SERVER: monster_death_use(); is commented out in the original.)

    self.get(g.th_die).?();

    q.self.set(oself);
}

/// T_Damage
///
/// The damage is coming from inflictor, but get mad at attacker
/// This should be the only function that ever reduces health.
pub fn T_Damage(targ: Entity, inflictor: Entity, attacker: Entity, damage_: f32) callconv(.c) void {
    var damage = damage_;

    if (targ.takedamage == 0)
        return;

    // used by buttons and triggers to set activator for target firing
    g.damage_attacker.set(attacker);

    // check for quad damage powerup on the attacker
    if (attacker.get(g.super_damage_finished) > q.time.get() and inflictor.classname.neq(S("door"))) {
        if (g.deathmatch.get() == 4) {
            damage = damage * 8;
        } else {
            damage = damage * 4;
        }
    }

    // save damage based on the target's armor level
    var save = q.ceil(targ.armortype * damage);
    if (save >= targ.armorvalue) {
        save = targ.armorvalue;
        targ.armortype = 0; // lost all armor
        targ.items = targ.items - q.band(targ.items, q.bor(q.bor(q.IT_ARMOR1, q.IT_ARMOR2), q.IT_ARMOR3));
    }

    targ.armorvalue = targ.armorvalue - save;
    const take = q.ceil(damage - save);

    // add to the damage total for clients, which will be sent as a single
    // message at the end of the frame
    // FIXME: remove after combining shotgun blasts?
    if (q.has(targ.flags, q.FL_CLIENT)) {
        targ.dmg_take = targ.dmg_take + take;
        targ.dmg_save = targ.dmg_save + save;
        targ.dmg_inflictor = inflictor;
    }

    g.damage_inflictor.set(inflictor);

    // figure momentum add
    if (inflictor.neq(q.world.get()) and targ.movetype == q.MOVETYPE_WALK) {
        var dir = targ.origin - q.scale(@as(q.Vector, inflictor.absmin) + inflictor.absmax, 0.5);
        dir = q.normalize(dir);
        // Set kickback for smaller weapons
        //Zoid -- use normal NQ kickback
        //      // Read: only if it's not yourself doing the damage
        //      if ( (damage < 6) & ((attacker.classname == "player") & (targ.classname == "player")) & ( attacker.netname != targ.netname))
        //          targ.velocity = targ.velocity + dir * damage * 11;
        //      else
        // Otherwise, these rules apply to rockets and grenades
        // for blast velocity
        targ.velocity = targ.velocity + q.scale(dir, damage * 8);

        // Rocket Jump modifiers
        if (g.rj.get() > 1 and (attacker.classname.eql(S("player")) and targ.classname.eql(S("player"))) and attacker.netname.eql(targ.netname))
            targ.velocity = targ.velocity + q.scale(dir, damage * g.rj.get());
    }

    // check for godmode or invincibility
    if (q.has(targ.flags, q.FL_GODMODE))
        return;
    if (targ.get(g.invincible_finished) >= q.time.get()) {
        // (The original checks and sets `self.invincible_sound`, not `targ`'s.)
        const self = q.me();
        if (self.get(g.invincible_sound) < q.time.get()) {
            q.sound(targ, q.CHAN_ITEM, S("items/protect3.wav"), 1, q.ATTN_NORM);
            self.set(g.invincible_sound, q.time.get() + 2);
        }
        return;
    }

    // team play damage avoidance
    //ZOID 12-13-96: self.team doesn't work in QW.  Use keys
    const attackerteam = q.infokey(attacker, S("team"));
    const targteam = q.infokey(targ, S("team"));

    if (g.teamplay.get() == 1 and targteam.eql(attackerteam) and
        attacker.classname.eql(S("player")) and attackerteam.neq(S("")) and
        inflictor.classname.neq(S("door")))
        return;

    if (g.teamplay.get() == 3 and targteam.eql(attackerteam) and
        attacker.classname.eql(S("player")) and attackerteam.neq(S("")) and
        targ.neq(attacker) and inflictor.classname.neq(S("door")))
        return;

    // do the damage
    targ.health = targ.health - take;

    if (targ.health <= 0) {
        Killed(targ, attacker);
        return;
    }

    // react to the damage
    const oldself = q.self.get();
    q.self.set(targ);

    // (SERVER: the monster reaction code is commented out in the original.)

    if (!q.fnIsNull(targ.get(g.th_pain))) {
        q.callAs(g.ThPain, targ.get(g.th_pain).?)(attacker, take);
    }

    q.self.set(oldself);
}

/// T_RadiusDamage
pub fn T_RadiusDamage(inflictor: Entity, attacker: Entity, damage: f32, ignore: Entity, dtype: String) callconv(.c) void {
    var head = q.findradius(inflictor.origin, damage + 40);

    while (!head.isNull()) {
        //bprint (PRINT_HIGH, head.classname);
        //bprint (PRINT_HIGH, " | ");
        //bprint (PRINT_HIGH, head.netname);
        //bprint (PRINT_HIGH, "\n");

        if (head.neq(ignore)) {
            if (head.takedamage != 0) {
                const org = head.origin + q.scale(@as(q.Vector, head.mins) + head.maxs, 0.5);
                var points = 0.5 * q.vlen(inflictor.origin - org);
                if (points < 0)
                    points = 0;
                points = damage - points;

                if (head.eql(attacker))
                    points = points * 0.5;
                if (points > 0) {
                    if (CanDamage(head, inflictor) != 0) {
                        head.set(g.deathtype, dtype);
                        T_Damage(head, inflictor, attacker, points);
                    }
                }
            }
        }
        head = head.chain;
    }
}

/// T_BeamDamage
pub fn T_BeamDamage(attacker: Entity, damage: f32) callconv(.c) void {
    var head = q.findradius(attacker.origin, damage + 40);

    while (!head.isNull()) {
        if (head.takedamage != 0) {
            var points = 0.5 * q.vlen(attacker.origin - head.origin);
            if (points < 0)
                points = 0;
            points = damage - points;
            if (head.eql(attacker))
                points = points * 0.5;
            if (points > 0) {
                if (CanDamage(head, attacker) != 0)
                    T_Damage(head, attacker, attacker, points);
            }
        }
        head = head.chain;
    }
}
