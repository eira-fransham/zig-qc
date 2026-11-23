//! client.qc
//!
//! client functions
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
const player = @import("player.zig");
const subs = @import("subs.zig");
const triggers = @import("triggers.zig");
const weapons = @import("weapons.zig");
const world = @import("world.zig");

pub const modelindex_eyes = q.userGlobal(f32, "modelindex_eyes");
pub const modelindex_player = q.userGlobal(f32, "modelindex_player");

// =============================================================================
//
//                              LEVEL CHANGING / INTERMISSION
//
// =============================================================================

pub const nextmap = q.userGlobal(q.String, "nextmap");

pub const intermission_running = q.userGlobal(f32, "intermission_running");
pub const intermission_exittime = q.userGlobal(f32, "intermission_exittime");

/// QUAKED info_intermission (1 0.5 0.5) (-16 -16 -16) (16 16 16)
/// This is the camera point for the intermission.
/// Use mangle instead of angle, so you can set pitch or roll as well as yaw.  'pitch roll yaw'
pub export fn info_intermission() void {
    const self = q.me();
    self.set(.angles, self.get(q.mangle)); // so C can get at it
}

pub export fn SetChangeParms() void {
    const self = q.me();
    if (self.get(.health) <= 0) {
        SetNewParms();
        return;
    }

    // remove items
    self.set(.items, self.get(.items) - q.band(self.get(.items), q.bor(q.bor(q.bor(q.IT_KEY1, q.IT_KEY2), q.bor(q.IT_INVISIBILITY, q.IT_INVULNERABILITY)), q.bor(q.IT_SUIT, q.IT_QUAD))));

    // cap super health
    if (self.get(.health) > 100)
        self.set(.health, 100);
    if (self.get(.health) < 50)
        self.set(.health, 50);
    q.parm1.set(self.get(.items));
    q.parm2.set(self.get(.health));
    q.parm3.set(self.get(.armorvalue));
    if (self.get(.ammo_shells) < 25) {
        q.parm4.set(25);
    } else {
        q.parm4.set(self.get(.ammo_shells));
    }
    q.parm5.set(self.get(.ammo_nails));
    q.parm6.set(self.get(.ammo_rockets));
    q.parm7.set(self.get(.ammo_cells));
    q.parm8.set(self.get(.weapon));
    q.parm9.set(self.get(.armortype) * 100);
}

pub export fn SetNewParms() void {
    q.parm1.set(q.bor(q.IT_SHOTGUN, q.IT_AXE));
    q.parm2.set(100);
    q.parm3.set(0);
    q.parm4.set(25);
    q.parm5.set(0);
    q.parm6.set(0);
    q.parm7.set(0);
    q.parm8.set(1);
    q.parm9.set(0);
}

pub fn DecodeLevelParms() callconv(.c) void {
    const self = q.me();
    if (q.serverflags.get() != 0) {
        if (q.world.get().get(.model).eql(S("maps/start.bsp")))
            SetNewParms(); // take away all stuff on starting new episode
    }

    self.set(.items, q.parm1.get());
    self.set(.health, q.parm2.get());
    self.set(.armorvalue, q.parm3.get());
    self.set(.ammo_shells, q.parm4.get());
    self.set(.ammo_nails, q.parm5.get());
    self.set(.ammo_rockets, q.parm6.get());
    self.set(.ammo_cells, q.parm7.get());
    self.set(.weapon, q.parm8.get());
    self.set(.armortype, q.parm9.get() * 0.01);
}

/// FindIntermission
///
/// Returns the entity to view from
pub fn FindIntermission() callconv(.c) Entity {
    // look for info_intermission first
    var spot = q.find(q.world.get(), .classname, S("info_intermission"));
    if (!spot.isNull()) { // pick a random one
        var cyc = q.random() * 4;
        while (cyc > 1) {
            spot = q.find(spot, .classname, S("info_intermission"));
            if (spot.isNull())
                spot = q.find(spot, .classname, S("info_intermission"));
            cyc = cyc - 1;
        }
        return spot;
    }

    // then look for the start position
    spot = q.find(q.world.get(), .classname, S("info_player_start"));
    if (!spot.isNull())
        return spot;

    q.objerror(S("FindIntermission: no spot"));
    // (The original falls off the end here, which returns `world`.)
    return q.world.get();
}

pub fn GotoNextMap() callconv(.c) void {
    //ZOID: 12-13-96, samelevel is overloaded, only 1 works for same level

    if (q.cvar(S("samelevel")) == 1) { // if samelevel is set, stay on same level
        q.changelevel(q.mapname.get());
    } else {
        // configurable map lists, see if the current map exists as a
        // serverinfo/localinfo var
        const newmap = q.infokey(q.world.get(), q.mapname.get());
        if (newmap.neq(S(""))) {
            q.changelevel(newmap);
        } else {
            q.changelevel(nextmap.get());
        }
    }
}

/// IntermissionThink
///
/// When the player presses attack or jump, change to the next level
pub fn IntermissionThink() callconv(.c) void {
    const self = q.me();
    if (q.time.get() < intermission_exittime.get())
        return;

    if (self.get(.button0) == 0 and self.get(.button1) == 0 and self.get(.button2) == 0)
        return;

    GotoNextMap();
}

/// execute_changelevel
///
/// The global "nextmap" has been set previously.
/// Take the players to the intermission spot
pub fn execute_changelevel() callconv(.c) void {
    intermission_running.set(1);

    // enforce a wait time before allowing changelevel
    intermission_exittime.set(q.time.get() + 5);

    const pos = FindIntermission();

    // play intermission music
    q.WriteByte(q.MSG_ALL, q.SVC_CDTRACK);
    q.WriteByte(q.MSG_ALL, 3);

    const origin = pos.get(.origin);
    const mangle = pos.get(q.mangle);
    q.WriteByte(q.MSG_ALL, q.SVC_INTERMISSION);
    q.WriteCoord(q.MSG_ALL, origin[0]);
    q.WriteCoord(q.MSG_ALL, origin[1]);
    q.WriteCoord(q.MSG_ALL, origin[2]);
    q.WriteAngle(q.MSG_ALL, mangle[0]);
    q.WriteAngle(q.MSG_ALL, mangle[1]);
    q.WriteAngle(q.MSG_ALL, mangle[2]);

    q.other.set(q.find(q.world.get(), .classname, S("player")));
    while (q.other.get().neq(q.world.get())) {
        const other = q.other.get();
        other.set(.takedamage, q.DAMAGE_NO);
        other.set(.solid, q.SOLID_NOT);
        other.set(.movetype, q.MOVETYPE_NONE);
        other.set(.modelindex, 0);
        q.other.set(q.find(other, .classname, S("player")));
    }
}

pub fn changelevel_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (other.get(.classname).neq(S("player")))
        return;

    // if "noexit" is set, blow up the player trying to leave
    //ZOID, 12-13-96, noexit isn't supported in QW.  Overload samelevel
    //      if ((cvar("noexit") == 1) || ((cvar("noexit") == 2) && (mapname != "start")))
    if (q.cvar(S("samelevel")) == 2 or (q.cvar(S("samelevel")) == 3 and q.mapname.get().neq(S("start")))) {
        combat.T_Damage(other, self, self, 50000);
        return;
    }

    q.bprint(q.PRINT_HIGH, other.get(.netname));
    q.bprint(q.PRINT_HIGH, S(" exited the level\n"));

    nextmap.set(self.get(q.map));

    subs.SUB_UseTargets();

    self.set(.touch, q.func(subs.SUB_Null));

    // we can't move people right now, because touch functions are called
    // in the middle of C movement code, so set a think time to do it
    self.set(.think, q.func(execute_changelevel));
    self.set(.nextthink, q.time.get() + 0.1);
}

/// QUAKED trigger_changelevel (0.5 0.5 0.5) ? NO_INTERMISSION
/// When the player touches this, he gets sent to the map listed in the "map"
/// variable.  Unless the NO_INTERMISSION flag is set, the view will go to the
/// info_intermission spot and display stats.
pub export fn trigger_changelevel() void {
    const self = q.me();
    if (self.get(q.map).isNull())
        q.objerror(S("chagnelevel trigger doesn't have map"));

    subs.InitTrigger();
    self.set(.touch, q.func(changelevel_touch));
}

// =============================================================================
//
//                              PLAYER GAME EDGE FUNCTIONS
//
// =============================================================================

/// called by ClientKill and DeadThink
pub fn respawn() callconv(.c) void {
    // make a copy of the dead body for appearances sake
    world.CopyToBodyQue(q.me());
    // set default spawn parms
    SetNewParms();
    // respawn
    PutClientInServer();
}

/// ClientKill
///
/// Player entered the suicide command
pub export fn ClientKill() void {
    const self = q.me();
    q.bprint(q.PRINT_MEDIUM, self.get(.netname));
    q.bprint(q.PRINT_MEDIUM, S(" suicides\n"));
    player.set_suicide_frame();
    self.set(.modelindex, modelindex_player.get());
    q.logfrag(self, self);
    self.set(.frags, self.get(.frags) - 2); // extra penalty
    respawn();
}

pub fn CheckSpawnPoint(v: Vector) callconv(.c) f32 {
    _ = v;
    return q.FALSE;
}

/// SelectSpawnPoint
///
/// Returns the entity to spawn at
pub fn SelectSpawnPoint() callconv(.c) Entity {
    var numspots: f32 = 0;
    var totalspots: f32 = 0;

    // testinfo_player_start is only found in regioned levels
    var spot = q.find(q.world.get(), .classname, S("testplayerstart"));
    if (!spot.isNull())
        return spot;

    // choose a info_player_deathmatch point

    // ok, find all spots that don't have players nearby

    var spots = q.world.get();
    spot = q.find(q.world.get(), .classname, S("info_player_deathmatch"));
    while (!spot.isNull()) {
        totalspots = totalspots + 1;

        var thing = q.findradius(spot.get(.origin), 84);
        var pcount: f32 = 0;
        while (!thing.isNull()) {
            if (thing.get(.classname).eql(S("player")))
                pcount = pcount + 1;
            thing = thing.get(.chain);
        }
        if (pcount == 0) {
            spot.set(.goalentity, spots);
            spots = spot;
            numspots = numspots + 1;
        }

        // Get the next spot in the chain
        spot = q.find(spot, .classname, S("info_player_deathmatch"));
    }
    totalspots = totalspots - 1;
    if (numspots == 0) {
        // ack, they are all full, just pick one at random
        //      bprint (PRINT_HIGH, "Ackk! All spots are full. Selecting random spawn spot\n");
        totalspots = q.rint(q.random() * totalspots);
        spot = q.find(q.world.get(), .classname, S("info_player_deathmatch"));
        while (totalspots > 0) {
            totalspots = totalspots - 1;
            spot = q.find(spot, .classname, S("info_player_deathmatch"));
        }
        return spot;
    }

    // We now have the number of spots available on the map in numspots

    // Generate a random number between 1 and numspots

    numspots = numspots - 1;

    numspots = q.rint(q.random() * numspots);

    spot = spots;
    while (numspots > 0) {
        spot = spot.get(.goalentity);
        numspots = numspots - 1;
    }
    return spot;
}

/// ValidateUser
///
/// (The whole body is commented out in the original, which then returns 0.
/// It used `serverinfo` and `masterinfo` builtins that QuakeWorld doesn't
/// have.)
pub fn ValidateUser(e: Entity) callconv(.c) f32 {
    _ = e;
    return q.FALSE;
}

/// PutClientInServer
///
/// called each time a player enters a new level
pub export fn PutClientInServer() void {
    const self = q.me();

    self.set(.classname, S("player"));
    self.set(.health, 10);
    self.set(.takedamage, q.DAMAGE_AIM);
    self.set(.solid, q.SOLID_SLIDEBOX);
    self.set(.movetype, q.MOVETYPE_WALK);
    self.set(q.show_hostile, 0);
    self.set(.max_health, 100);
    self.set(.flags, q.FL_CLIENT);
    self.set(q.air_finished, q.time.get() + 12);
    self.set(q.dmg, 2); // initial water damage
    self.set(q.super_damage_finished, 0);
    self.set(q.radsuit_finished, 0);
    self.set(q.invisible_finished, 0);
    self.set(q.invincible_finished, 0);
    self.set(.effects, 0);
    self.set(q.invincible_time, 0);

    DecodeLevelParms();

    weapons.W_SetCurrentAmmo();

    self.set(q.attack_finished, q.time.get());
    self.set(q.th_pain, q.func(player.player_pain));
    self.set(q.th_die, q.func(player.PlayerDie));

    self.set(.deadflag, q.DEAD_NO);
    // paustime is set by teleporters to keep the player from moving a while
    self.set(q.pausetime, 0);

    const spot = SelectSpawnPoint();

    self.set(.origin, spot.get(.origin) + q.vec(0, 0, 1));
    self.set(.angles, spot.get(.angles));
    self.set(.fixangle, q.TRUE); // turn this way immediately

    // oh, this is a hack!
    q.setmodel(self, S("progs/eyes.mdl"));
    modelindex_eyes.set(self.get(.modelindex));

    q.setmodel(self, S("progs/player.mdl"));
    modelindex_player.set(self.get(.modelindex));

    q.setsize(self, q.VEC_HULL_MIN, q.VEC_HULL_MAX);

    self.set(.view_ofs, q.vec(0, 0, 22));

    // Mod - Xian (May.20.97)
    // Bug where player would have velocity from their last kill

    self.set(.velocity, q.vec(0, 0, 0));

    player.player_stand1();

    q.makevectors(self.get(.angles));
    triggers.spawn_tfog(self.get(.origin) + q.scale(q.v_forward.get(), 20));

    triggers.spawn_tdeath(self.get(.origin), self);

    // Set Rocket Jump Modifiers
    if (q.stof(q.infokey(q.world.get(), S("rj"))) != 0) {
        q.rj.set(q.stof(q.infokey(q.world.get(), S("rj"))));
    }

    const all_armor = q.bor(q.bor(q.IT_ARMOR1, q.IT_ARMOR2), q.IT_ARMOR3);

    if (q.deathmatch.get() == 4) {
        self.set(.ammo_shells, 0);
        if (q.stof(q.infokey(q.world.get(), S("axe"))) == 0) {
            self.set(.ammo_nails, 255);
            self.set(.ammo_shells, 255);
            self.set(.ammo_rockets, 255);
            self.set(.ammo_cells, 255);
            self.set(.items, q.bor(self.get(.items), q.IT_NAILGUN));
            self.set(.items, q.bor(self.get(.items), q.IT_SUPER_NAILGUN));
            self.set(.items, q.bor(self.get(.items), q.IT_SUPER_SHOTGUN));
            self.set(.items, q.bor(self.get(.items), q.IT_ROCKET_LAUNCHER));
            //      self.items = self.items | IT_GRENADE_LAUNCHER;
            self.set(.items, q.bor(self.get(.items), q.IT_LIGHTNING));
        }
        self.set(.items, self.get(.items) - q.band(self.get(.items), all_armor) + q.IT_ARMOR3);
        self.set(.armorvalue, 200);
        self.set(.armortype, 0.8);
        self.set(.health, 250);
        self.set(.items, q.bor(self.get(.items), q.IT_INVULNERABILITY));
        self.set(q.invincible_time, 1);
        self.set(q.invincible_finished, q.time.get() + 3);
    }

    if (q.deathmatch.get() == 5) {
        self.set(.ammo_nails, 80);
        self.set(.ammo_shells, 30);
        self.set(.ammo_rockets, 10);
        self.set(.ammo_cells, 30);
        self.set(.items, q.bor(self.get(.items), q.IT_NAILGUN));
        self.set(.items, q.bor(self.get(.items), q.IT_SUPER_NAILGUN));
        self.set(.items, q.bor(self.get(.items), q.IT_SUPER_SHOTGUN));
        self.set(.items, q.bor(self.get(.items), q.IT_ROCKET_LAUNCHER));
        self.set(.items, q.bor(self.get(.items), q.IT_GRENADE_LAUNCHER));
        self.set(.items, q.bor(self.get(.items), q.IT_LIGHTNING));
        self.set(.items, self.get(.items) - q.band(self.get(.items), all_armor) + q.IT_ARMOR3);
        self.set(.armorvalue, 200);
        self.set(.armortype, 0.8);
        self.set(.health, 200);
        self.set(.items, q.bor(self.get(.items), q.IT_INVULNERABILITY));
        self.set(q.invincible_time, 1);
        self.set(q.invincible_finished, q.time.get() + 3);
    }
}

// =============================================================================
//
//                              QUAKED FUNCTIONS
//
// =============================================================================

/// QUAKED info_player_start (1 0 0) (-16 -16 -24) (16 16 24)
/// The normal starting point for a level.
pub export fn info_player_start() void {}

/// QUAKED info_player_start2 (1 0 0) (-16 -16 -24) (16 16 24)
/// Only used on start map for the return point from an episode.
pub export fn info_player_start2() void {}

/// QUAKED info_player_deathmatch (1 0 1) (-16 -16 -24) (16 16 24)
/// potential spawning position for deathmatch games
pub export fn info_player_deathmatch() void {}

/// QUAKED info_player_coop (1 0 1) (-16 -16 -24) (16 16 24)
/// potential spawning position for coop games
pub export fn info_player_coop() void {}

// ===============================================================================
//
// RULES
//
// ===============================================================================

/// go to the next level for deathmatch
pub fn NextLevel() callconv(.c) void {
    if (nextmap.get().neq(S("")))
        return; // already done

    var o: Entity = undefined;
    if (q.mapname.get().eql(S("start"))) {
        const sf = q.serverflags.get();
        if (q.cvar(S("registered")) == 0) {
            q.mapname.set(S("e1m1"));
        } else if (!q.has(sf, 1)) {
            q.mapname.set(S("e1m1"));
            q.serverflags.set(q.bor(sf, 1));
        } else if (!q.has(sf, 2)) {
            q.mapname.set(S("e2m1"));
            q.serverflags.set(q.bor(sf, 2));
        } else if (!q.has(sf, 4)) {
            q.mapname.set(S("e3m1"));
            q.serverflags.set(q.bor(sf, 4));
        } else if (!q.has(sf, 8)) {
            q.mapname.set(S("e4m1"));
            q.serverflags.set(sf - 7);
        }

        o = q.spawn();
        o.set(q.map, q.mapname.get());
    } else {
        // find a trigger changelevel
        o = q.find(q.world.get(), .classname, S("trigger_changelevel"));
        if (o.isNull() or q.mapname.get().eql(S("start"))) { // go back to same map if no trigger_changelevel
            o = q.spawn();
            o.set(q.map, q.mapname.get());
        }
    }

    nextmap.set(o.get(q.map));

    if (o.get(.nextthink) < q.time.get()) {
        o.set(.think, q.func(execute_changelevel));
        o.set(.nextthink, q.time.get() + 0.1);
    }
}

/// CheckRules
///
/// Exit deathmatch games upon conditions
pub fn CheckRules() callconv(.c) void {
    const self = q.me();
    if (q.timelimit.get() != 0 and q.time.get() >= q.timelimit.get())
        NextLevel();

    if (q.fraglimit.get() != 0 and self.get(.frags) >= q.fraglimit.get())
        NextLevel();
}

//============================================================================

pub fn PlayerDeathThink() callconv(.c) void {
    const self = q.me();

    if (q.has(self.get(.flags), q.FL_ONGROUND)) {
        var forward = q.vlen(self.get(.velocity));
        forward = forward - 20;
        if (forward <= 0) {
            self.set(.velocity, q.vec(0, 0, 0));
        } else {
            self.set(.velocity, q.scale(q.normalize(self.get(.velocity)), forward));
        }
    }

    // wait for all buttons released
    if (self.get(.deadflag) == q.DEAD_DEAD) {
        if (self.get(.button2) != 0 or self.get(.button1) != 0 or self.get(.button0) != 0)
            return;
        self.set(.deadflag, q.DEAD_RESPAWNABLE);
        return;
    }

    // wait for any button down
    if (self.get(.button2) == 0 and self.get(.button1) == 0 and self.get(.button0) == 0)
        return;

    self.set(.button0, 0);
    self.set(.button1, 0);
    self.set(.button2, 0);
    respawn();
}

pub fn PlayerJump() callconv(.c) void {
    const self = q.me();

    if (q.has(self.get(.flags), q.FL_WATERJUMP))
        return;

    if (self.get(.waterlevel) >= 2) {
        // play swiming sound
        if (self.get(q.swim_flag) < q.time.get()) {
            self.set(q.swim_flag, q.time.get() + 1);
            if (q.random() < 0.5) {
                q.sound(self, q.CHAN_BODY, S("misc/water1.wav"), 1, q.ATTN_NORM);
            } else {
                q.sound(self, q.CHAN_BODY, S("misc/water2.wav"), 1, q.ATTN_NORM);
            }
        }

        return;
    }

    if (!q.has(self.get(.flags), q.FL_ONGROUND))
        return;

    if (!q.has(self.get(.flags), q.FL_JUMPRELEASED))
        return; // don't pogo stick

    self.set(.flags, self.get(.flags) - q.band(self.get(.flags), q.FL_JUMPRELEASED));
    self.set(.button2, 0);

    // player jumping sound
    q.sound(self, q.CHAN_BODY, S("player/plyrjmp8.wav"), 1, q.ATTN_NORM);
}

/// WaterMove
pub const dmgtime = q.userField(f32, "dmgtime");

pub fn WaterMove() callconv(.c) void {
    const self = q.me();
    const time = q.time.get();

    //dprint (ftos(self.waterlevel));
    if (self.get(.movetype) == q.MOVETYPE_NOCLIP)
        return;
    if (self.get(.health) < 0)
        return;

    if (self.get(.waterlevel) != 3) {
        if (self.get(q.air_finished) < time) {
            q.sound(self, q.CHAN_VOICE, S("player/gasp2.wav"), 1, q.ATTN_NORM);
        } else if (self.get(q.air_finished) < time + 9) {
            q.sound(self, q.CHAN_VOICE, S("player/gasp1.wav"), 1, q.ATTN_NORM);
        }
        self.set(q.air_finished, time + 12);
        self.set(q.dmg, 2);
    } else if (self.get(q.air_finished) < time) { // drown!
        if (self.get(q.pain_finished) < time) {
            self.set(q.dmg, self.get(q.dmg) + 2);
            if (self.get(q.dmg) > 15)
                self.set(q.dmg, 10);
            combat.T_Damage(self, q.world.get(), q.world.get(), self.get(q.dmg));
            self.set(q.pain_finished, time + 1);
        }
    }

    if (self.get(.waterlevel) == 0) {
        if (q.has(self.get(.flags), q.FL_INWATER)) {
            // play leave water sound
            q.sound(self, q.CHAN_BODY, S("misc/outwater.wav"), 1, q.ATTN_NORM);
            self.set(.flags, self.get(.flags) - q.FL_INWATER);
        }
        return;
    }

    if (self.get(.watertype) == q.CONTENT_LAVA) { // do damage
        if (self.get(dmgtime) < time) {
            if (self.get(q.radsuit_finished) > time) {
                self.set(dmgtime, time + 1);
            } else {
                self.set(dmgtime, time + 0.2);
            }

            combat.T_Damage(self, q.world.get(), q.world.get(), 10 * self.get(.waterlevel));
        }
    } else if (self.get(.watertype) == q.CONTENT_SLIME) { // do damage
        if (self.get(dmgtime) < time and self.get(q.radsuit_finished) < time) {
            self.set(dmgtime, time + 1);
            combat.T_Damage(self, q.world.get(), q.world.get(), 4 * self.get(.waterlevel));
        }
    }

    if (!q.has(self.get(.flags), q.FL_INWATER)) {

        // player enter water sound

        if (self.get(.watertype) == q.CONTENT_LAVA)
            q.sound(self, q.CHAN_BODY, S("player/inlava.wav"), 1, q.ATTN_NORM);
        if (self.get(.watertype) == q.CONTENT_WATER)
            q.sound(self, q.CHAN_BODY, S("player/inh2o.wav"), 1, q.ATTN_NORM);
        if (self.get(.watertype) == q.CONTENT_SLIME)
            q.sound(self, q.CHAN_BODY, S("player/slimbrn2.wav"), 1, q.ATTN_NORM);

        self.set(.flags, self.get(.flags) + q.FL_INWATER);
        self.set(dmgtime, 0);
    }
}

pub fn CheckWaterJump() callconv(.c) void {
    const self = q.me();

    // check for a jump-out-of-water
    q.makevectors(self.get(.angles));
    var start = self.get(.origin);
    start[2] = start[2] + 8;
    q.v_forward_z.set(0);
    _ = q.normalize(q.v_forward.get()); // (the result is discarded in the original)
    var end = start + q.scale(q.v_forward.get(), 24);
    q.traceline(start, end, q.TRUE, self);
    if (q.trace_fraction.get() < 1) { // solid at waist
        start[2] = start[2] + self.get(.maxs)[2] - 8;
        end = start + q.scale(q.v_forward.get(), 24);
        self.set(.movedir, q.scale(q.trace_plane_normal.get(), -50));
        q.traceline(start, end, q.TRUE, self);
        if (q.trace_fraction.get() == 1) { // open at eye level
            self.set(.flags, q.bor(self.get(.flags), q.FL_WATERJUMP));
            var v = self.get(.velocity);
            v[2] = 225;
            self.set(.velocity, v);
            self.set(.flags, self.get(.flags) - q.band(self.get(.flags), q.FL_JUMPRELEASED));
            self.set(.teleport_time, q.time.get() + 2); // safety net
            return;
        }
    }
}

/// PlayerPreThink
///
/// Called every frame before physics are run
pub export fn PlayerPreThink() void {
    const self = q.me();

    if (intermission_running.get() != 0) {
        IntermissionThink(); // otherwise a button could be missed between
        return; // the think tics
    }

    if (q.veq(self.get(.view_ofs), q.vec(0, 0, 0)))
        return; // intermission or finale

    q.makevectors(self.get(.v_angle)); // is this still used

    self.set(q.deathtype, S(""));

    CheckRules();
    WaterMove();
    //  if (self.waterlevel == 2)
    //      CheckWaterJump ();

    if (self.get(.deadflag) >= q.DEAD_DEAD) {
        PlayerDeathThink();
        return;
    }

    if (self.get(.deadflag) == q.DEAD_DYING)
        return; // dying, so do nothing

    if (self.get(.button2) != 0) {
        PlayerJump();
    } else {
        self.set(.flags, q.bor(self.get(.flags), q.FL_JUMPRELEASED));
    }

    // teleporters can force a non-moving pause time
    if (q.time.get() < self.get(q.pausetime))
        self.set(.velocity, q.vec(0, 0, 0));

    if (q.time.get() > self.get(q.attack_finished) and self.get(.currentammo) == 0 and self.get(.weapon) != q.IT_AXE) {
        self.set(.weapon, weapons.W_BestWeapon());
        weapons.W_SetCurrentAmmo();
    }
}

/// CheckPowerups
///
/// Check for turning off powerups
pub fn CheckPowerups() callconv(.c) void {
    const self = q.me();
    const time = q.time.get();

    if (self.get(.health) <= 0)
        return;

    // invisibility
    if (self.get(q.invisible_finished) != 0) {
        // sound and screen flash when items starts to run out
        if (self.get(q.invisible_sound) < time) {
            q.sound(self, q.CHAN_AUTO, S("items/inv3.wav"), 0.5, q.ATTN_IDLE);
            self.set(q.invisible_sound, time + ((q.random() * 3) + 1));
        }

        if (self.get(q.invisible_finished) < time + 3) {
            if (self.get(q.invisible_time) == 1) {
                q.sprint(self, q.PRINT_HIGH, S("Ring of Shadows magic is fading\n"));
                q.stuffcmd(self, S("bf\n"));
                q.sound(self, q.CHAN_AUTO, S("items/inv2.wav"), 1, q.ATTN_NORM);
                self.set(q.invisible_time, time + 1);
            }

            if (self.get(q.invisible_time) < time) {
                self.set(q.invisible_time, time + 1);
                q.stuffcmd(self, S("bf\n"));
            }
        }

        if (self.get(q.invisible_finished) < time) { // just stopped
            self.set(.items, self.get(.items) - q.IT_INVISIBILITY);
            self.set(q.invisible_finished, 0);
            self.set(q.invisible_time, 0);
        }

        // use the eyes
        self.set(.frame, 0);
        self.set(.modelindex, modelindex_eyes.get());
    } else {
        self.set(.modelindex, modelindex_player.get()); // don't use eyes
    }

    // invincibility
    if (self.get(q.invincible_finished) != 0) {
        // sound and screen flash when items starts to run out
        if (self.get(q.invincible_finished) < time + 3) {
            if (self.get(q.invincible_time) == 1) {
                q.sprint(self, q.PRINT_HIGH, S("Protection is almost burned out\n"));
                q.stuffcmd(self, S("bf\n"));
                q.sound(self, q.CHAN_AUTO, S("items/protect2.wav"), 1, q.ATTN_NORM);
                self.set(q.invincible_time, time + 1);
            }

            if (self.get(q.invincible_time) < time) {
                self.set(q.invincible_time, time + 1);
                q.stuffcmd(self, S("bf\n"));
            }
        }

        if (self.get(q.invincible_finished) < time) { // just stopped
            self.set(.items, self.get(.items) - q.IT_INVULNERABILITY);
            self.set(q.invincible_time, 0);
            self.set(q.invincible_finished, 0);
        }
        if (self.get(q.invincible_finished) > time) {
            self.set(.effects, q.bor(self.get(.effects), q.EF_DIMLIGHT));
            self.set(.effects, q.bor(self.get(.effects), q.EF_RED));
        } else {
            self.set(.effects, self.get(.effects) - q.band(self.get(.effects), q.EF_DIMLIGHT));
            self.set(.effects, self.get(.effects) - q.band(self.get(.effects), q.EF_RED));
        }
    }

    // super damage
    if (self.get(q.super_damage_finished) != 0) {

        // sound and screen flash when items starts to run out

        if (self.get(q.super_damage_finished) < time + 3) {
            if (self.get(q.super_time) == 1) {
                if (q.deathmatch.get() == 4) {
                    q.sprint(self, q.PRINT_HIGH, S("OctaPower is wearing off\n"));
                } else {
                    q.sprint(self, q.PRINT_HIGH, S("Quad Damage is wearing off\n"));
                }
                q.stuffcmd(self, S("bf\n"));
                q.sound(self, q.CHAN_AUTO, S("items/damage2.wav"), 1, q.ATTN_NORM);
                self.set(q.super_time, time + 1);
            }

            if (self.get(q.super_time) < time) {
                self.set(q.super_time, time + 1);
                q.stuffcmd(self, S("bf\n"));
            }
        }

        if (self.get(q.super_damage_finished) < time) { // just stopped
            self.set(.items, self.get(.items) - q.IT_QUAD);
            if (q.deathmatch.get() == 4) {
                self.set(.ammo_cells, 255);
                self.set(.armorvalue, 1);
                self.set(.armortype, 0.8);
                self.set(.health, 100);
            }
            self.set(q.super_damage_finished, 0);
            self.set(q.super_time, 0);
        }
        if (self.get(q.super_damage_finished) > time) {
            self.set(.effects, q.bor(self.get(.effects), q.EF_DIMLIGHT));
            self.set(.effects, q.bor(self.get(.effects), q.EF_BLUE));
        } else {
            self.set(.effects, self.get(.effects) - q.band(self.get(.effects), q.EF_DIMLIGHT));
            self.set(.effects, self.get(.effects) - q.band(self.get(.effects), q.EF_BLUE));
        }
    }

    // suit
    if (self.get(q.radsuit_finished) != 0) {
        self.set(q.air_finished, time + 12); // don't drown

        // sound and screen flash when items starts to run out
        if (self.get(q.radsuit_finished) < time + 3) {
            if (self.get(q.rad_time) == 1) {
                q.sprint(self, q.PRINT_HIGH, S("Air supply in Biosuit expiring\n"));
                q.stuffcmd(self, S("bf\n"));
                q.sound(self, q.CHAN_AUTO, S("items/suit2.wav"), 1, q.ATTN_NORM);
                self.set(q.rad_time, time + 1);
            }

            if (self.get(q.rad_time) < time) {
                self.set(q.rad_time, time + 1);
                q.stuffcmd(self, S("bf\n"));
            }
        }

        if (self.get(q.radsuit_finished) < time) { // just stopped
            self.set(.items, self.get(.items) - q.IT_SUIT);
            self.set(q.rad_time, 0);
            self.set(q.radsuit_finished, 0);
        }
    }
}

/// PlayerPostThink
///
/// Called every frame after physics are run
pub export fn PlayerPostThink() void {
    const self = q.me();

    //dprint ("post think\n");
    if (q.veq(self.get(.view_ofs), q.vec(0, 0, 0)))
        return; // intermission or finale
    if (self.get(.deadflag) != 0)
        return;

    // check to see if player landed and play landing sound
    if (self.get(q.jump_flag) < -300 and q.has(self.get(.flags), q.FL_ONGROUND)) {
        if (self.get(.watertype) == q.CONTENT_WATER) {
            q.sound(self, q.CHAN_BODY, S("player/h2ojump.wav"), 1, q.ATTN_NORM);
        } else if (self.get(q.jump_flag) < -650) {
            self.set(q.deathtype, S("falling"));
            combat.T_Damage(self, q.world.get(), q.world.get(), 5);
            q.sound(self, q.CHAN_VOICE, S("player/land2.wav"), 1, q.ATTN_NORM);
        } else {
            q.sound(self, q.CHAN_VOICE, S("player/land.wav"), 1, q.ATTN_NORM);
        }
    }

    self.set(q.jump_flag, self.get(.velocity)[2]);

    CheckPowerups();

    weapons.W_WeaponFrame();
}

/// ClientConnect
///
/// called when a player connects to a server
pub export fn ClientConnect() void {
    const self = q.me();
    q.bprint(q.PRINT_HIGH, self.get(.netname));
    q.bprint(q.PRINT_HIGH, S(" entered the game\n"));

    // a client connecting during an intermission can cause problems
    if (intermission_running.get() != 0)
        GotoNextMap();
}

/// ClientDisconnect
///
/// called when a player disconnects from a server
pub export fn ClientDisconnect() void {
    const self = q.me();
    // let everyone else know
    q.bprint(q.PRINT_HIGH, self.get(.netname));
    q.bprint(q.PRINT_HIGH, S(" left the game with "));
    q.bprint(q.PRINT_HIGH, q.ftos(self.get(.frags)));
    q.bprint(q.PRINT_HIGH, S(" frags\n"));
    q.sound(self, q.CHAN_BODY, S("player/tornoff2.wav"), 1, q.ATTN_NONE);
    player.set_suicide_frame();
}

/// ClientObituary
///
/// called when a player dies
pub fn ClientObituary(targ: Entity, attacker: Entity) callconv(.c) void {
    const M = q.PRINT_MEDIUM;

    var rnum = q.random();
    //ZOID 12-13-96: self.team doesn't work in QW.  Use keys
    const attackerteam = q.infokey(attacker, S("team"));
    const targteam = q.infokey(targ, S("team"));

    if (targ.get(.classname).neq(S("player")))
        return;

    if (q.deathmatch.get() > 3) {
        if (targ.get(q.deathtype).eql(S("selfwater"))) {
            q.bprint(M, targ.get(.netname));
            q.bprint(M, S(" electrocutes himself.\n "));
            targ.set(.frags, targ.get(.frags) - 1);
            return;
        }
    }

    if (attacker.get(.classname).eql(S("teledeath"))) {
        q.bprint(M, targ.get(.netname));
        q.bprint(M, S(" was telefragged by "));
        q.bprint(M, attacker.get(.owner).get(.netname));
        q.bprint(M, S("\n"));
        q.logfrag(attacker.get(.owner), targ);

        attacker.get(.owner).set(.frags, attacker.get(.owner).get(.frags) + 1);
        return;
    }

    if (attacker.get(.classname).eql(S("teledeath2"))) {
        q.bprint(M, S("Satan's power deflects "));
        q.bprint(M, targ.get(.netname));
        q.bprint(M, S("'s telefrag\n"));

        targ.set(.frags, targ.get(.frags) - 1);
        q.logfrag(targ, targ);
        return;
    }

    // double 666 telefrag (can happen often in deathmatch 4)
    if (attacker.get(.classname).eql(S("teledeath3"))) {
        q.bprint(M, targ.get(.netname));
        q.bprint(M, S(" was telefragged by "));
        q.bprint(M, attacker.get(.owner).get(.netname));
        q.bprint(M, S("'s Satan's power\n"));
        targ.set(.frags, targ.get(.frags) - 1);
        q.logfrag(targ, targ);
        return;
    }

    if (targ.get(q.deathtype).eql(S("squish"))) {
        if (q.teamplay.get() != 0 and targteam.eql(attackerteam) and attackerteam.neq(S("")) and targ.neq(attacker)) {
            q.logfrag(attacker, attacker);
            attacker.set(.frags, attacker.get(.frags) - 1);
            q.bprint(M, attacker.get(.netname));
            q.bprint(M, S(" squished a teammate\n"));
            return;
        } else if (attacker.get(.classname).eql(S("player")) and attacker.neq(targ)) {
            q.bprint(M, attacker.get(.netname));
            q.bprint(M, S(" squishes "));
            q.bprint(M, targ.get(.netname));
            q.bprint(M, S("\n"));
            q.logfrag(attacker, targ);
            attacker.set(.frags, attacker.get(.frags) + 1);
            return;
        } else {
            q.logfrag(targ, targ);
            targ.set(.frags, targ.get(.frags) - 1); // killed self
            q.bprint(M, targ.get(.netname));
            q.bprint(M, S(" was squished\n"));
            return;
        }
    }

    if (attacker.get(.classname).eql(S("player"))) {
        if (targ.eql(attacker)) {
            // killed self
            q.logfrag(attacker, attacker);
            attacker.set(.frags, attacker.get(.frags) - 1);
            q.bprint(M, targ.get(.netname));
            if (targ.get(q.deathtype).eql(S("grenade"))) {
                q.bprint(M, S(" tries to put the pin back in\n"));
            } else if (targ.get(q.deathtype).eql(S("rocket"))) {
                q.bprint(M, S(" becomes bored with life\n"));
            } else if (targ.get(.weapon) == 64 and targ.get(.waterlevel) > 1) {
                if (targ.get(.watertype) == q.CONTENT_SLIME) {
                    q.bprint(M, S(" discharges into the slime\n"));
                } else if (targ.get(.watertype) == q.CONTENT_LAVA) {
                    q.bprint(M, S(" discharges into the lava\n"));
                } else {
                    q.bprint(M, S(" discharges into the water.\n"));
                }
            } else {
                q.bprint(M, S(" becomes bored with life\n"));
            }
            return;
        } else if (q.teamplay.get() == 2 and targteam.eql(attackerteam) and attackerteam.neq(S(""))) {
            const deathstring = if (rnum < 0.25)
                S(" mows down a teammate\n")
            else if (rnum < 0.50)
                S(" checks his glasses\n")
            else if (rnum < 0.75)
                S(" gets a frag for the other team\n")
            else
                S(" loses another friend\n");
            q.bprint(M, attacker.get(.netname));
            q.bprint(M, deathstring);
            attacker.set(.frags, attacker.get(.frags) - 1);
            //ZOID 12-13-96:  killing a teammate logs as suicide
            q.logfrag(attacker, attacker);
            return;
        } else {
            q.logfrag(attacker, targ);
            attacker.set(.frags, attacker.get(.frags) + 1);

            // (These are unset in the original if nothing below matches.)
            var deathstring = q.string_null.get();
            var deathstring2 = q.string_null.get();

            rnum = attacker.get(.weapon);
            const deathtype = targ.get(q.deathtype);
            if (deathtype.eql(S("nail"))) {
                deathstring = S(" was nailed by ");
                deathstring2 = S("\n");
            } else if (deathtype.eql(S("supernail"))) {
                deathstring = S(" was punctured by ");
                deathstring2 = S("\n");
            } else if (deathtype.eql(S("grenade"))) {
                deathstring = S(" eats ");
                deathstring2 = S("'s pineapple\n");
                if (targ.get(.health) < -40) {
                    deathstring = S(" was gibbed by ");
                    deathstring2 = S("'s grenade\n");
                }
            } else if (deathtype.eql(S("rocket"))) {
                if (attacker.get(q.super_damage_finished) > 0 and targ.get(.health) < -40) {
                    rnum = q.random();
                    if (rnum < 0.3) {
                        deathstring = S(" was brutalized by ");
                    } else if (rnum < 0.6) {
                        deathstring = S(" was smeared by ");
                    } else {
                        q.bprint(M, attacker.get(.netname));
                        q.bprint(M, S(" rips "));
                        q.bprint(M, targ.get(.netname));
                        q.bprint(M, S(" a new one\n"));
                        return;
                    }
                    deathstring2 = S("'s quad rocket\n");
                } else {
                    deathstring = S(" rides ");
                    deathstring2 = S("'s rocket\n");
                    if (targ.get(.health) < -40) {
                        deathstring = S(" was gibbed by ");
                        deathstring2 = S("'s rocket\n");
                    }
                }
            } else if (rnum == q.IT_AXE) {
                deathstring = S(" was ax-murdered by ");
                deathstring2 = S("\n");
            } else if (rnum == q.IT_SHOTGUN) {
                deathstring = S(" chewed on ");
                deathstring2 = S("'s boomstick\n");
            } else if (rnum == q.IT_SUPER_SHOTGUN) {
                deathstring = S(" ate 2 loads of ");
                deathstring2 = S("'s buckshot\n");
            } else if (rnum == q.IT_LIGHTNING) {
                deathstring = S(" accepts ");
                if (attacker.get(.waterlevel) > 1) {
                    deathstring2 = S("'s discharge\n");
                } else {
                    deathstring2 = S("'s shaft\n");
                }
            }
            q.bprint(M, targ.get(.netname));
            q.bprint(M, deathstring);
            q.bprint(M, attacker.get(.netname));
            q.bprint(M, deathstring2);
        }
        return;
    } else {
        q.logfrag(targ, targ);
        targ.set(.frags, targ.get(.frags) - 1); // killed self
        rnum = targ.get(.watertype);

        q.bprint(M, targ.get(.netname));
        if (rnum == -3) {
            if (q.random() < 0.5) {
                q.bprint(M, S(" sleeps with the fishes\n"));
            } else {
                q.bprint(M, S(" sucks it down\n"));
            }
            return;
        } else if (rnum == -4) {
            if (q.random() < 0.5) {
                q.bprint(M, S(" gulped a load of slime\n"));
            } else {
                q.bprint(M, S(" can't exist on slime alone\n"));
            }
            return;
        } else if (rnum == -5) {
            if (targ.get(.health) < -15) {
                q.bprint(M, S(" burst into flames\n"));
                return;
            }
            if (q.random() < 0.5) {
                q.bprint(M, S(" turned into hot slag\n"));
            } else {
                q.bprint(M, S(" visits the Volcano God\n"));
            }
            return;
        }

        const deathtype = targ.get(q.deathtype);
        if (attacker.get(.classname).eql(S("explo_box"))) {
            q.bprint(M, S(" blew up\n"));
            return;
        }
        if (deathtype.eql(S("falling"))) {
            q.bprint(M, S(" fell to his death\n"));
            return;
        }
        if (deathtype.eql(S("nail")) or deathtype.eql(S("supernail"))) {
            q.bprint(M, S(" was spiked\n"));
            return;
        }
        if (deathtype.eql(S("laser"))) {
            q.bprint(M, S(" was zapped\n"));
            return;
        }
        if (attacker.get(.classname).eql(S("fireball"))) {
            q.bprint(M, S(" ate a lavaball\n"));
            return;
        }
        if (attacker.get(.classname).eql(S("trigger_changelevel"))) {
            q.bprint(M, S(" tried to leave\n"));
            return;
        }

        q.bprint(M, S(" died\n"));
    }
}
