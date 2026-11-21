//! misc.qc
//!
//! pretty much everything else
//!
//! Copyright (C) 1996-1997  Id Software, Inc.
//!
//! This program is free software; you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation; either version 2 of the License, or (at your option)
//! any later version. See gnu.txt.

const q = @import("defs.zig");
const S = q.S;
const Vector = q.Vector;

const combat = @import("combat.zig");
const subs = @import("subs.zig");
const weapons = @import("weapons.zig");

/// QUAKED info_null (0 0.5 0) (-4 -4 -4) (4 4 4)
/// Used as a positional target for spotlights, etc.
pub export fn info_null() void {
    q.remove(q.me());
}

/// QUAKED info_notnull (0 0.5 0) (-4 -4 -4) (4 4 4)
/// Used as a positional target for lightning.
pub export fn info_notnull() void {}

//============================================================================

pub const START_OFF: f32 = 1;

pub fn light_use() callconv(.c) void {
    const self = q.me();
    if (q.has(self.get(.spawnflags), START_OFF)) {
        q.lightstyle(self.get(q.style), S("m"));
        self.set(.spawnflags, self.get(.spawnflags) - START_OFF);
    } else {
        q.lightstyle(self.get(q.style), S("a"));
        self.set(.spawnflags, self.get(.spawnflags) + START_OFF);
    }
}

/// QUAKED light (0 1 0) (-8 -8 -8) (8 8 8) START_OFF
/// Non-displayed light.
/// Default light value is 300
/// Default style is 0
/// If targeted, it will toggle between on or off.
pub export fn light() void {
    const self = q.me();
    if (self.get(.targetname).isNull()) { // inert light
        q.remove(self);
        return;
    }

    if (self.get(q.style) >= 32) {
        self.set(.use, q.func(light_use));
        if (q.has(self.get(.spawnflags), START_OFF)) {
            q.lightstyle(self.get(q.style), S("a"));
        } else {
            q.lightstyle(self.get(q.style), S("m"));
        }
    }
}

/// QUAKED light_fluoro (0 1 0) (-8 -8 -8) (8 8 8) START_OFF
/// Non-displayed light.
/// Default light value is 300
/// Default style is 0
/// If targeted, it will toggle between on or off.
/// Makes steady fluorescent humming sound
pub export fn light_fluoro() void {
    const self = q.me();
    if (self.get(q.style) >= 32) {
        self.set(.use, q.func(light_use));
        if (q.has(self.get(.spawnflags), START_OFF)) {
            q.lightstyle(self.get(q.style), S("a"));
        } else {
            q.lightstyle(self.get(q.style), S("m"));
        }
    }

    _ = q.precache_sound(S("ambience/fl_hum1.wav"));
    q.ambientsound(self.get(.origin), S("ambience/fl_hum1.wav"), 0.5, q.ATTN_STATIC);
}

/// QUAKED light_fluorospark (0 1 0) (-8 -8 -8) (8 8 8)
/// Non-displayed light.
/// Default light value is 300
/// Default style is 10
/// Makes sparking, broken fluorescent sound
pub export fn light_fluorospark() void {
    const self = q.me();
    if (self.get(q.style) == 0)
        self.set(q.style, 10);

    _ = q.precache_sound(S("ambience/buzz1.wav"));
    q.ambientsound(self.get(.origin), S("ambience/buzz1.wav"), 0.5, q.ATTN_STATIC);
}

/// QUAKED light_globe (0 1 0) (-8 -8 -8) (8 8 8)
/// Sphere globe light.
/// Default light value is 300
/// Default style is 0
pub export fn light_globe() void {
    const self = q.me();
    _ = q.precache_model(S("progs/s_light.spr"));
    q.setmodel(self, S("progs/s_light.spr"));
    q.makestatic(self);
}

pub fn FireAmbient() callconv(.c) void {
    _ = q.precache_sound(S("ambience/fire1.wav"));
    // attenuate fast
    q.ambientsound(q.me().get(.origin), S("ambience/fire1.wav"), 0.5, q.ATTN_STATIC);
}

/// QUAKED light_torch_small_walltorch (0 .5 0) (-10 -10 -20) (10 10 20)
/// Short wall torch
/// Default light value is 200
/// Default style is 0
pub export fn light_torch_small_walltorch() void {
    const self = q.me();
    _ = q.precache_model(S("progs/flame.mdl"));
    q.setmodel(self, S("progs/flame.mdl"));
    FireAmbient();
    q.makestatic(self);
}

/// QUAKED light_flame_large_yellow (0 1 0) (-10 -10 -12) (12 12 18)
/// Large yellow flame ball
pub export fn light_flame_large_yellow() void {
    const self = q.me();
    _ = q.precache_model(S("progs/flame2.mdl"));
    q.setmodel(self, S("progs/flame2.mdl"));
    self.set(.frame, 1);
    FireAmbient();
    q.makestatic(self);
}

/// QUAKED light_flame_small_yellow (0 1 0) (-8 -8 -8) (8 8 8) START_OFF
/// Small yellow flame ball
pub export fn light_flame_small_yellow() void {
    const self = q.me();
    _ = q.precache_model(S("progs/flame2.mdl"));
    q.setmodel(self, S("progs/flame2.mdl"));
    FireAmbient();
    q.makestatic(self);
}

/// QUAKED light_flame_small_white (0 1 0) (-10 -10 -40) (10 10 40) START_OFF
/// Small white flame ball
pub export fn light_flame_small_white() void {
    const self = q.me();
    _ = q.precache_model(S("progs/flame2.mdl"));
    q.setmodel(self, S("progs/flame2.mdl"));
    FireAmbient();
    q.makestatic(self);
}

//============================================================================

/// QUAKED misc_fireball (0 .5 .8) (-8 -8 -8) (8 8 8)
/// Lava Balls
pub export fn misc_fireball() void {
    const self = q.me();

    _ = q.precache_model(S("progs/lavaball.mdl"));
    self.set(.classname, S("fireball"));
    self.set(.nextthink, q.time.get() + (q.random() * 5));
    self.set(.think, q.func(fire_fly));
    // (`self.speed == 1000;` in the original: a comparison, so `speed` stays
    // 0.)
    if (self.get(q.speed) == 0)
        _ = self.get(q.speed) == 1000;
}

pub fn fire_fly() callconv(.c) void {
    const self = q.me();

    const fireball = q.spawn();
    fireball.set(.solid, q.SOLID_TRIGGER);
    fireball.set(.movetype, q.MOVETYPE_TOSS);
    fireball.set(.velocity, q.vec(
        (q.random() * 100) - 50,
        (q.random() * 100) - 50,
        self.get(q.speed) + (q.random() * 200),
    ));
    fireball.set(.classname, S("fireball"));
    q.setmodel(fireball, S("progs/lavaball.mdl"));
    q.setsize(fireball, q.vec(0, 0, 0), q.vec(0, 0, 0));
    q.setorigin(fireball, self.get(.origin));
    fireball.set(.nextthink, q.time.get() + 5);
    fireball.set(.think, q.func(subs.SUB_Remove));
    fireball.set(.touch, q.func(fire_touch));

    self.set(.nextthink, q.time.get() + (q.random() * 5) + 3);
    self.set(.think, q.func(fire_fly));
}

pub fn fire_touch() callconv(.c) void {
    const self = q.me();
    combat.T_Damage(q.other.get(), self, self, 20);
    q.remove(self);
}

//============================================================================

pub fn barrel_explode() callconv(.c) void {
    const self = q.me();
    self.set(.takedamage, q.DAMAGE_NO);
    self.set(.classname, S("explo_box"));
    // did say self.owner
    combat.T_RadiusDamage(self, self, 160, q.world.get(), S(""));
    const o = self.get(.origin);
    q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
    q.WriteByte(q.MSG_MULTICAST, q.TE_EXPLOSION);
    q.WriteCoord(q.MSG_MULTICAST, o[0]);
    q.WriteCoord(q.MSG_MULTICAST, o[1]);
    q.WriteCoord(q.MSG_MULTICAST, o[2] + 32);
    q.multicast(o, q.MULTICAST_PHS);
    q.remove(self);
}

/// Shared by `misc_explobox` and `misc_explobox2`, which differ only in
/// their model and size.
inline fn explobox(model: q.String, maxs: Vector) void {
    const self = q.me();

    self.set(.solid, q.SOLID_BBOX);
    self.set(.movetype, q.MOVETYPE_NONE);
    q.setmodel(self, model);
    q.setsize(self, q.vec(0, 0, 0), maxs);
    _ = q.precache_sound(S("weapons/r_exp3.wav"));
    self.set(.health, 20);
    self.set(q.th_die, q.func(barrel_explode));
    self.set(.takedamage, q.DAMAGE_AIM);

    var origin = self.get(.origin);
    origin[2] = origin[2] + 2;
    self.set(.origin, origin);
    const oldz = origin[2];
    _ = q.droptofloor();
    if (oldz - self.get(.origin)[2] > 250) {
        q.dprint(S("item fell out of level at "));
        q.dprint(q.vtos(self.get(.origin)));
        q.dprint(S("\n"));
        q.remove(self);
    }
}

/// QUAKED misc_explobox (0 .5 .8) (0 0 0) (32 32 64)
/// TESTING THING
pub export fn misc_explobox() void {
    _ = q.precache_model(S("maps/b_explob.bsp"));
    explobox(S("maps/b_explob.bsp"), q.vec(32, 32, 64));
}

/// QUAKED misc_explobox2 (0 .5 .8) (0 0 0) (32 32 64)
/// Smaller exploding box, REGISTERED ONLY
pub export fn misc_explobox2() void {
    _ = q.precache_model2(S("maps/b_exbox2.bsp"));
    explobox(S("maps/b_exbox2.bsp"), q.vec(32, 32, 32));
}

//============================================================================

pub const SPAWNFLAG_SUPERSPIKE: f32 = 1;
pub const SPAWNFLAG_LASER: f32 = 2;

pub fn Laser_Touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (other.eql(self.get(.owner)))
        return; // don't explode on owner

    if (q.pointcontents(self.get(.origin)) == q.CONTENT_SKY) {
        q.remove(self);
        return;
    }

    q.sound(self, q.CHAN_WEAPON, S("enforcer/enfstop.wav"), 1, q.ATTN_STATIC);
    const org = self.get(.origin) - q.scale(q.normalize(self.get(.velocity)), 8);

    if (other.get(.health) != 0) {
        weapons.SpawnBlood(org, 15);
        other.set(q.deathtype, S("laser"));
        combat.T_Damage(other, self, self.get(.owner), 15);
    } else {
        q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
        q.WriteByte(q.MSG_MULTICAST, q.TE_GUNSHOT);
        q.WriteByte(q.MSG_MULTICAST, 5);
        q.WriteCoord(q.MSG_MULTICAST, org[0]);
        q.WriteCoord(q.MSG_MULTICAST, org[1]);
        q.WriteCoord(q.MSG_MULTICAST, org[2]);
        q.multicast(org, q.MULTICAST_PVS);
    }

    q.remove(self);
}

pub fn LaunchLaser(org: Vector, vec_: Vector) callconv(.c) void {
    const self = q.me();
    // (The original redeclares `local vector vec;`, which qcc resolves to the
    // parameter.)

    if (self.get(.classname).eql(S("monster_enforcer")))
        q.sound(self, q.CHAN_WEAPON, S("enforcer/enfire.wav"), 1, q.ATTN_NORM);

    const vec = q.normalize(vec_);

    q.newmis.set(q.spawn());
    const newmis = q.newmis.get();
    newmis.set(.owner, self);
    newmis.set(.movetype, q.MOVETYPE_FLY);
    newmis.set(.solid, q.SOLID_BBOX);
    newmis.set(.effects, q.EF_DIMLIGHT);

    q.setmodel(newmis, S("progs/laser.mdl"));
    q.setsize(newmis, q.vec(0, 0, 0), q.vec(0, 0, 0));

    q.setorigin(newmis, org);

    newmis.set(.velocity, q.scale(vec, 600));
    newmis.set(.angles, q.vectoangles(newmis.get(.velocity)));

    newmis.set(.nextthink, q.time.get() + 5);
    newmis.set(.think, q.func(subs.SUB_Remove));
    newmis.set(.touch, q.func(Laser_Touch));
}

pub fn spikeshooter_use() callconv(.c) void {
    const self = q.me();
    if (q.has(self.get(.spawnflags), SPAWNFLAG_LASER)) {
        q.sound(self, q.CHAN_VOICE, S("enforcer/enfire.wav"), 1, q.ATTN_NORM);
        LaunchLaser(self.get(.origin), self.get(.movedir));
    } else {
        q.sound(self, q.CHAN_VOICE, S("weapons/spike2.wav"), 1, q.ATTN_NORM);
        weapons.launch_spike(self.get(.origin), self.get(.movedir));
        q.newmis.get().set(.velocity, q.scale(self.get(.movedir), 500));
        if (q.has(self.get(.spawnflags), SPAWNFLAG_SUPERSPIKE))
            q.newmis.get().set(.touch, q.func(weapons.superspike_touch));
    }
}

pub fn shooter_think() callconv(.c) void {
    const self = q.me();
    spikeshooter_use();
    self.set(.nextthink, q.time.get() + self.get(q.wait));
    q.newmis.get().set(.velocity, q.scale(self.get(.movedir), 500));
}

/// QUAKED trap_spikeshooter (0 .5 .8) (-8 -8 -8) (8 8 8) superspike laser
/// When triggered, fires a spike in the direction set in QuakeEd.
/// Laser is only for REGISTERED.
pub export fn trap_spikeshooter() void {
    const self = q.me();
    subs.SetMovedir();
    self.set(.use, q.func(spikeshooter_use));
    if (q.has(self.get(.spawnflags), SPAWNFLAG_LASER)) {
        _ = q.precache_model2(S("progs/laser.mdl"));

        _ = q.precache_sound2(S("enforcer/enfire.wav"));
        _ = q.precache_sound2(S("enforcer/enfstop.wav"));
    } else {
        _ = q.precache_sound(S("weapons/spike2.wav"));
    }
}

/// QUAKED trap_shooter (0 .5 .8) (-8 -8 -8) (8 8 8) superspike laser
/// Continuously fires spikes.
/// "wait" time between spike (1.0 default)
/// "nextthink" delay before firing first spike, so multiple shooters can be stagered.
pub export fn trap_shooter() void {
    const self = q.me();
    trap_spikeshooter();

    if (self.get(q.wait) == 0)
        self.set(q.wait, 1);
    self.set(.nextthink, self.get(.nextthink) + self.get(q.wait) + self.get(.ltime));
    self.set(.think, q.func(shooter_think));
}

// ===============================================================================
//
//
// ===============================================================================

/// QUAKED air_bubbles (0 .5 .8) (-8 -8 -8) (8 8 8)
///
/// testing air bubbles
pub export fn air_bubbles() void {
    q.remove(q.me());
}

pub fn make_bubbles() callconv(.c) void {
    const self = q.me();

    const bubble = q.spawn();
    q.setmodel(bubble, S("progs/s_bubble.spr"));
    q.setorigin(bubble, self.get(.origin));
    bubble.set(.movetype, q.MOVETYPE_NOCLIP);
    bubble.set(.solid, q.SOLID_NOT);
    bubble.set(.velocity, q.vec(0, 0, 15));
    bubble.set(.nextthink, q.time.get() + 0.5);
    bubble.set(.think, q.func(bubble_bob));
    bubble.set(.touch, q.func(bubble_remove));
    bubble.set(.classname, S("bubble"));
    bubble.set(.frame, 0);
    bubble.set(q.cnt, 0);
    q.setsize(bubble, q.vec(-8, -8, -8), q.vec(8, 8, 8));
    self.set(.nextthink, q.time.get() + q.random() + 0.5);
    self.set(.think, q.func(make_bubbles));
}

pub fn bubble_split() callconv(.c) void {
    const self = q.me();

    const bubble = q.spawn();
    q.setmodel(bubble, S("progs/s_bubble.spr"));
    q.setorigin(bubble, self.get(.origin));
    bubble.set(.movetype, q.MOVETYPE_NOCLIP);
    bubble.set(.solid, q.SOLID_NOT);
    bubble.set(.velocity, self.get(.velocity));
    bubble.set(.nextthink, q.time.get() + 0.5);
    bubble.set(.think, q.func(bubble_bob));
    bubble.set(.touch, q.func(bubble_remove));
    bubble.set(.classname, S("bubble"));
    bubble.set(.frame, 1);
    bubble.set(q.cnt, 10);
    q.setsize(bubble, q.vec(-8, -8, -8), q.vec(8, 8, 8));
    self.set(.frame, 1);
    self.set(q.cnt, 10);
    if (self.get(.waterlevel) != 3)
        q.remove(self);
}

pub fn bubble_remove() callconv(.c) void {
    const self = q.me();
    if (q.other.get().get(.classname).eql(self.get(.classname))) {
        //              dprint ("bump");
        return;
    }
    q.remove(self);
}

pub fn bubble_bob() callconv(.c) void {
    const self = q.me();

    self.set(q.cnt, self.get(q.cnt) + 1);
    if (self.get(q.cnt) == 4)
        bubble_split();
    if (self.get(q.cnt) == 20)
        q.remove(self);

    const vel = self.get(.velocity);
    var rnd1 = vel[0] + (-10 + (q.random() * 20));
    var rnd2 = vel[1] + (-10 + (q.random() * 20));
    var rnd3 = vel[2] + 10 + q.random() * 10;

    if (rnd1 > 10)
        rnd1 = 5;
    if (rnd1 < -10)
        rnd1 = -5;

    if (rnd2 > 10)
        rnd2 = 5;
    if (rnd2 < -10)
        rnd2 = -5;

    if (rnd3 < 10)
        rnd3 = 15;
    if (rnd3 > 30)
        rnd3 = 25;

    self.set(.velocity, q.vec(rnd1, rnd2, rnd3));

    self.set(.nextthink, q.time.get() + 0.5);
    self.set(.think, q.func(bubble_bob));
}

// ~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>
// ~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~<~>~

/// QUAKED viewthing (0 .5 .8) (-8 -8 -8) (8 8 8)
///
/// Just for the debugging level.  Don't use
pub export fn viewthing() void {
    const self = q.me();
    self.set(.movetype, q.MOVETYPE_NONE);
    self.set(.solid, q.SOLID_NOT);
    _ = q.precache_model(S("progs/player.mdl"));
    q.setmodel(self, S("progs/player.mdl"));
}

// ==============================================================================
//
// SIMPLE BMODELS
//
// ==============================================================================

pub fn func_wall_use() callconv(.c) void {
    // change to alternate textures
    const self = q.me();
    self.set(.frame, 1 - self.get(.frame));
}

/// QUAKED func_wall (0 .5 .8) ?
/// This is just a solid wall if not inhibitted
pub export fn func_wall() void {
    const self = q.me();
    self.set(.angles, q.vec(0, 0, 0));
    self.set(.movetype, q.MOVETYPE_PUSH); // so it doesn't get pushed by anything
    self.set(.solid, q.SOLID_BSP);
    self.set(.use, q.func(func_wall_use));
    q.setmodel(self, self.get(.model));
}

/// QUAKED func_illusionary (0 .5 .8) ?
/// A simple entity that looks solid but lets you walk through it.
pub export fn func_illusionary() void {
    const self = q.me();
    self.set(.angles, q.vec(0, 0, 0));
    self.set(.movetype, q.MOVETYPE_NONE);
    self.set(.solid, q.SOLID_NOT);
    q.setmodel(self, self.get(.model));
    // The original calls `makestatic ();` without an argument, which passes
    // whatever is left in the first parameter slot: `self`, from `setmodel`.
    q.makestatic(self);
}

/// QUAKED func_episodegate (0 .5 .8) ? E1 E2 E3 E4
/// This bmodel will appear if the episode has allready been completed, so
/// players can't reenter it.
pub export fn func_episodegate() void {
    const self = q.me();
    if (!q.has(q.serverflags.get(), self.get(.spawnflags)))
        return; // can still enter episode

    self.set(.angles, q.vec(0, 0, 0));
    self.set(.movetype, q.MOVETYPE_PUSH); // so it doesn't get pushed by anything
    self.set(.solid, q.SOLID_BSP);
    self.set(.use, q.func(func_wall_use));
    q.setmodel(self, self.get(.model));
}

/// QUAKED func_bossgate (0 .5 .8) ?
/// This bmodel appears unless players have all of the episode sigils.
pub export fn func_bossgate() void {
    const self = q.me();
    if (q.band(q.serverflags.get(), 15) == 15)
        return; // all episodes completed
    self.set(.angles, q.vec(0, 0, 0));
    self.set(.movetype, q.MOVETYPE_PUSH); // so it doesn't get pushed by anything
    self.set(.solid, q.SOLID_BSP);
    self.set(.use, q.func(func_wall_use));
    q.setmodel(self, self.get(.model));
}

//============================================================================

/// `precache_sound` and `ambientsound` at `self.origin`, shared by the
/// `ambient_*` entities.
inline fn ambient(comptime sample: []const u8, vol: f32) void {
    _ = q.precache_sound(S(sample));
    q.ambientsound(q.me().get(.origin), S(sample), vol, q.ATTN_STATIC);
}

/// QUAKED ambient_suck_wind (0.3 0.1 0.6) (-10 -10 -8) (10 10 8)
pub export fn ambient_suck_wind() void {
    ambient("ambience/suck1.wav", 1);
}

/// QUAKED ambient_drone (0.3 0.1 0.6) (-10 -10 -8) (10 10 8)
pub export fn ambient_drone() void {
    ambient("ambience/drone6.wav", 0.5);
}

/// QUAKED ambient_flouro_buzz (0.3 0.1 0.6) (-10 -10 -8) (10 10 8)
pub export fn ambient_flouro_buzz() void {
    ambient("ambience/buzz1.wav", 1);
}

/// QUAKED ambient_drip (0.3 0.1 0.6) (-10 -10 -8) (10 10 8)
pub export fn ambient_drip() void {
    ambient("ambience/drip1.wav", 0.5);
}

/// QUAKED ambient_comp_hum (0.3 0.1 0.6) (-10 -10 -8) (10 10 8)
pub export fn ambient_comp_hum() void {
    ambient("ambience/comp1.wav", 1);
}

/// QUAKED ambient_thunder (0.3 0.1 0.6) (-10 -10 -8) (10 10 8)
pub export fn ambient_thunder() void {
    ambient("ambience/thunder1.wav", 0.5);
}

/// QUAKED ambient_light_buzz (0.3 0.1 0.6) (-10 -10 -8) (10 10 8)
pub export fn ambient_light_buzz() void {
    ambient("ambience/fl_hum1.wav", 0.5);
}

/// QUAKED ambient_swamp1 (0.3 0.1 0.6) (-10 -10 -8) (10 10 8)
pub export fn ambient_swamp1() void {
    ambient("ambience/swamp1.wav", 0.5);
}

/// QUAKED ambient_swamp2 (0.3 0.1 0.6) (-10 -10 -8) (10 10 8)
pub export fn ambient_swamp2() void {
    ambient("ambience/swamp2.wav", 0.5);
}

//============================================================================

pub fn noise_think() callconv(.c) void {
    const self = q.me();
    self.set(.nextthink, q.time.get() + 0.5);
    q.sound(self, 1, S("enforcer/enfire.wav"), 1, q.ATTN_NORM);
    q.sound(self, 2, S("enforcer/enfstop.wav"), 1, q.ATTN_NORM);
    q.sound(self, 3, S("enforcer/sight1.wav"), 1, q.ATTN_NORM);
    q.sound(self, 4, S("enforcer/sight2.wav"), 1, q.ATTN_NORM);
    q.sound(self, 5, S("enforcer/sight3.wav"), 1, q.ATTN_NORM);
    q.sound(self, 6, S("enforcer/sight4.wav"), 1, q.ATTN_NORM);
    q.sound(self, 7, S("enforcer/pain1.wav"), 1, q.ATTN_NORM);
}

/// QUAKED misc_noisemaker (1 0.5 0) (-10 -10 -10) (10 10 10)
///
/// For optimzation testing, starts a lot of sounds.
pub export fn misc_noisemaker() void {
    const self = q.me();
    _ = q.precache_sound2(S("enforcer/enfire.wav"));
    _ = q.precache_sound2(S("enforcer/enfstop.wav"));
    _ = q.precache_sound2(S("enforcer/sight1.wav"));
    _ = q.precache_sound2(S("enforcer/sight2.wav"));
    _ = q.precache_sound2(S("enforcer/sight3.wav"));
    _ = q.precache_sound2(S("enforcer/sight4.wav"));
    _ = q.precache_sound2(S("enforcer/pain1.wav"));
    _ = q.precache_sound2(S("enforcer/pain2.wav"));
    _ = q.precache_sound2(S("enforcer/death1.wav"));
    _ = q.precache_sound2(S("enforcer/idle1.wav"));

    self.set(.nextthink, q.time.get() + 0.1 + q.random());
    self.set(.think, q.func(noise_think));
}
