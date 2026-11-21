//! weapons.qc
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

/// called by worldspawn
pub export fn W_Precache() void {
    _ = q.precache_sound(S("weapons/r_exp3.wav")); // new rocket explosion
    _ = q.precache_sound(S("weapons/rocket1i.wav")); // spike gun
    _ = q.precache_sound(S("weapons/sgun1.wav"));
    _ = q.precache_sound(S("weapons/guncock.wav")); // player shotgun
    _ = q.precache_sound(S("weapons/ric1.wav")); // ricochet (used in c code)
    _ = q.precache_sound(S("weapons/ric2.wav")); // ricochet (used in c code)
    _ = q.precache_sound(S("weapons/ric3.wav")); // ricochet (used in c code)
    _ = q.precache_sound(S("weapons/spike2.wav")); // super spikes
    _ = q.precache_sound(S("weapons/tink1.wav")); // spikes tink (used in c code)
    _ = q.precache_sound(S("weapons/grenade.wav")); // grenade launcher
    _ = q.precache_sound(S("weapons/bounce.wav")); // grenade bounce
    _ = q.precache_sound(S("weapons/shotgn2.wav")); // super shotgun
}

pub export fn crandom() f32 {
    return 2 * (q.random() - 0.5);
}

/// W_FireAxe
pub export fn W_FireAxe() void {
    const self = q.me();

    q.makevectors(self.get(.v_angle));
    const source = self.get(.origin) + q.vec(0, 0, 16);
    q.traceline(source, source + q.scale(q.v_forward.get(), 64), q.FALSE, self);
    if (q.trace_fraction.get() == 1.0)
        return;

    const org = q.trace_endpos.get() - q.scale(q.v_forward.get(), 4);

    const trace_ent = q.trace_ent.get();
    if (trace_ent.get(.takedamage) != 0) {
        trace_ent.set(q.axhitme, 1);
        SpawnBlood(org, 20);
        if (q.deathmatch.get() > 3) {
            combat.T_Damage(trace_ent, self, self, 75);
        } else {
            combat.T_Damage(trace_ent, self, self, 20);
        }
    } else { // hit wall
        q.sound(self, q.CHAN_WEAPON, S("player/axhit2.wav"), 1, q.ATTN_NORM);

        q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
        q.WriteByte(q.MSG_MULTICAST, q.TE_GUNSHOT);
        q.WriteByte(q.MSG_MULTICAST, 3);
        q.WriteCoord(q.MSG_MULTICAST, org[0]);
        q.WriteCoord(q.MSG_MULTICAST, org[1]);
        q.WriteCoord(q.MSG_MULTICAST, org[2]);
        q.multicast(org, q.MULTICAST_PVS);
    }
}

//============================================================================

pub export fn wall_velocity() Vector {
    const self = q.me();
    var vel = q.normalize(self.get(.velocity));
    vel = q.normalize(vel + q.scale(q.v_up.get(), q.random() - 0.5) + q.scale(q.v_right.get(), q.random() - 0.5));
    vel = vel + q.scale(q.trace_plane_normal.get(), 2);
    vel = q.scale(vel, 200);

    return vel;
}

/// SpawnMeatSpray
pub export fn SpawnMeatSpray(org: Vector, vel: Vector) void {
    const self = q.me();
    // (The original also declares a `local vector org;`, shadowing the
    // parameter without ever setting it. The parameter is used here.)

    const missile = q.spawn();
    missile.set(.owner, self);
    missile.set(.movetype, q.MOVETYPE_BOUNCE);
    missile.set(.solid, q.SOLID_NOT);

    q.makevectors(self.get(.angles));

    var v = vel;
    v[2] = v[2] + 250 + 50 * q.random();
    missile.set(.velocity, v);

    missile.set(.avelocity, q.vec(3000, 1000, 2000));

    // set missile duration
    missile.set(.nextthink, q.time.get() + 1);
    missile.set(.think, q.func(subs.SUB_Remove));

    q.setmodel(missile, S("progs/zom_gib.mdl"));
    q.setsize(missile, q.vec(0, 0, 0), q.vec(0, 0, 0));
    q.setorigin(missile, org);
}

/// SpawnBlood
pub export fn SpawnBlood(org: Vector, damage: f32) void {
    _ = damage;
    q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
    q.WriteByte(q.MSG_MULTICAST, q.TE_BLOOD);
    q.WriteByte(q.MSG_MULTICAST, 1);
    q.WriteCoord(q.MSG_MULTICAST, org[0]);
    q.WriteCoord(q.MSG_MULTICAST, org[1]);
    q.WriteCoord(q.MSG_MULTICAST, org[2]);
    q.multicast(org, q.MULTICAST_PVS);
}

/// spawn_touchblood
pub export fn spawn_touchblood(damage: f32) void {
    const vel = q.scale(wall_velocity(), 0.2);
    SpawnBlood(q.me().get(.origin) + q.scale(vel, 0.01), damage);
}

// ==============================================================================
//
// MULTI-DAMAGE
//
// Collects multiple small damages into a single damage
//
// ==============================================================================

pub const multi_ent = q.userGlobal(Entity, "multi_ent");
pub const multi_damage = q.userGlobal(f32, "multi_damage");

pub const blood_org = q.userGlobal(Vector, "blood_org");
pub const blood_count = q.userGlobal(f32, "blood_count");

pub const puff_org = q.userGlobal(Vector, "puff_org");
pub const puff_count = q.userGlobal(f32, "puff_count");

pub export fn ClearMultiDamage() void {
    multi_ent.set(q.world.get());
    multi_damage.set(0);
    blood_count.set(0);
    puff_count.set(0);
}

pub export fn ApplyMultiDamage() void {
    if (multi_ent.get().isNull())
        return;
    const self = q.me();
    combat.T_Damage(multi_ent.get(), self, self, multi_damage.get());
}

pub export fn AddMultiDamage(hit: Entity, damage: f32) void {
    if (hit.isNull())
        return;

    if (hit.neq(multi_ent.get())) {
        ApplyMultiDamage();
        multi_damage.set(damage);
        multi_ent.set(hit);
    } else {
        multi_damage.set(multi_damage.get() + damage);
    }
}

pub export fn Multi_Finish() void {
    if (puff_count.get() != 0) {
        const o = puff_org.get();
        q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
        q.WriteByte(q.MSG_MULTICAST, q.TE_GUNSHOT);
        q.WriteByte(q.MSG_MULTICAST, puff_count.get());
        q.WriteCoord(q.MSG_MULTICAST, o[0]);
        q.WriteCoord(q.MSG_MULTICAST, o[1]);
        q.WriteCoord(q.MSG_MULTICAST, o[2]);
        q.multicast(o, q.MULTICAST_PVS);
    }

    if (blood_count.get() != 0) {
        const o = blood_org.get();
        q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
        q.WriteByte(q.MSG_MULTICAST, q.TE_BLOOD);
        q.WriteByte(q.MSG_MULTICAST, blood_count.get());
        q.WriteCoord(q.MSG_MULTICAST, o[0]);
        q.WriteCoord(q.MSG_MULTICAST, o[1]);
        q.WriteCoord(q.MSG_MULTICAST, o[2]);
        q.multicast(puff_org.get(), q.MULTICAST_PVS);
    }
}

// ==============================================================================
//
// BULLETS
//
// ==============================================================================

/// TraceAttack
pub export fn TraceAttack(damage: f32, dir: Vector) void {
    var vel = q.normalize(dir + q.scale(q.v_up.get(), crandom()) + q.scale(q.v_right.get(), crandom()));
    vel = vel + q.scale(q.trace_plane_normal.get(), 2);
    vel = q.scale(vel, 200);
    // (`vel` is unused in the original.)

    const org = q.trace_endpos.get() - q.scale(dir, 4);

    const trace_ent = q.trace_ent.get();
    if (trace_ent.get(.takedamage) != 0) {
        blood_count.set(blood_count.get() + 1);
        blood_org.set(org);
        AddMultiDamage(trace_ent, damage);
    } else {
        puff_count.set(puff_count.get() + 1);
    }
}

/// FireBullets
///
/// Used by shotgun, super shotgun, and enemy soldier firing
/// Go to the trouble of combining multiple pellets into a single damage call.
pub export fn FireBullets(shotcount_: f32, dir: Vector, spread: Vector) void {
    const self = q.me();
    var shotcount = shotcount_;

    q.makevectors(self.get(.v_angle));

    var src = self.get(.origin) + q.scale(q.v_forward.get(), 10);
    src[2] = self.get(.absmin)[2] + self.get(.size)[2] * 0.7;

    ClearMultiDamage();

    q.traceline(src, src + q.scale(dir, 2048), q.FALSE, self);
    puff_org.set(q.trace_endpos.get() - q.scale(dir, 4));

    while (shotcount > 0) {
        const direction = dir + q.scale(q.v_right.get(), crandom() * spread[0]) + q.scale(q.v_up.get(), crandom() * spread[1]);
        q.traceline(src, src + q.scale(direction, 2048), q.FALSE, self);
        if (q.trace_fraction.get() != 1.0)
            TraceAttack(4, direction);

        shotcount = shotcount - 1;
    }
    ApplyMultiDamage();
    Multi_Finish();
}

/// W_FireShotgun
pub export fn W_FireShotgun() void {
    const self = q.me();

    q.sound(self, q.CHAN_WEAPON, S("weapons/guncock.wav"), 1, q.ATTN_NORM);

    q.msg_entity.set(self);
    q.WriteByte(q.MSG_ONE, q.SVC_SMALLKICK);

    if (q.deathmatch.get() != 4) {
        self.set(.ammo_shells, self.get(.ammo_shells) - 1);
        self.set(.currentammo, self.get(.ammo_shells));
    }

    const dir = q.aim(self, 100000);
    FireBullets(6, dir, q.vec(0.04, 0.04, 0));
}

/// W_FireSuperShotgun
pub export fn W_FireSuperShotgun() void {
    const self = q.me();

    if (self.get(.currentammo) == 1) {
        W_FireShotgun();
        return;
    }

    q.sound(self, q.CHAN_WEAPON, S("weapons/shotgn2.wav"), 1, q.ATTN_NORM);

    q.msg_entity.set(self);
    q.WriteByte(q.MSG_ONE, q.SVC_BIGKICK);

    if (q.deathmatch.get() != 4) {
        self.set(.ammo_shells, self.get(.ammo_shells) - 2);
        self.set(.currentammo, self.get(.ammo_shells));
    }
    const dir = q.aim(self, 100000);
    FireBullets(14, dir, q.vec(0.14, 0.08, 0));
}

// ==============================================================================
//
// ROCKETS
//
// ==============================================================================

pub export fn T_MissileTouch() void {
    const self = q.me();
    const other = q.other.get();

    //  if (deathmatch == 4)
    //  {
    //  if ( ((other.weapon == 32) || (other.weapon == 16)))
    //      {
    //          if (random() < 0.1)
    //          {
    //              if (other != world)
    //              {
    //  //              bprint (PRINT_HIGH, "Got here\n");
    //                  other.deathtype = "blaze";
    //                  T_Damage (other, self, self.owner, 1000 );
    //                  T_RadiusDamage (self, self.owner, 1000, other);
    //              }
    //          }
    //      }
    //  }

    if (other.eql(self.get(.owner)))
        return; // don't explode on owner

    if (self.get(q.voided) != 0) {
        return;
    }
    self.set(q.voided, 1);

    if (q.pointcontents(self.get(.origin)) == q.CONTENT_SKY) {
        q.remove(self);
        return;
    }

    const damg = 100 + q.random() * 20;

    if (other.get(.health) != 0) {
        other.set(q.deathtype, S("rocket"));
        combat.T_Damage(other, self, self.get(.owner), damg);
    }

    // don't do radius damage to the other, because all the damage
    // was done in the impact

    combat.T_RadiusDamage(self, self.get(.owner), 120, other, S("rocket"));

    //  sound (self, CHAN_WEAPON, "weapons/r_exp3.wav", 1, ATTN_NORM);
    self.set(.origin, self.get(.origin) - q.scale(q.normalize(self.get(.velocity)), 8));

    const o = self.get(.origin);
    q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
    q.WriteByte(q.MSG_MULTICAST, q.TE_EXPLOSION);
    q.WriteCoord(q.MSG_MULTICAST, o[0]);
    q.WriteCoord(q.MSG_MULTICAST, o[1]);
    q.WriteCoord(q.MSG_MULTICAST, o[2]);
    q.multicast(o, q.MULTICAST_PHS);

    q.remove(self);
}

/// W_FireRocket
pub export fn W_FireRocket() void {
    const self = q.me();

    if (q.deathmatch.get() != 4) {
        self.set(.ammo_rockets, self.get(.ammo_rockets) - 1);
        self.set(.currentammo, self.get(.ammo_rockets));
    }

    q.sound(self, q.CHAN_WEAPON, S("weapons/sgun1.wav"), 1, q.ATTN_NORM);

    q.msg_entity.set(self);
    q.WriteByte(q.MSG_ONE, q.SVC_SMALLKICK);

    q.newmis.set(q.spawn());
    const newmis = q.newmis.get();
    newmis.set(.owner, self);
    newmis.set(.movetype, q.MOVETYPE_FLYMISSILE);
    newmis.set(.solid, q.SOLID_BBOX);

    // set newmis speed

    q.makevectors(self.get(.v_angle));
    newmis.set(.velocity, q.aim(self, 1000));
    newmis.set(.velocity, q.scale(newmis.get(.velocity), 1000));
    newmis.set(.angles, q.vectoangles(newmis.get(.velocity)));

    newmis.set(.touch, q.func(T_MissileTouch));
    newmis.set(q.voided, 0);

    // set newmis duration
    newmis.set(.nextthink, q.time.get() + 5);
    newmis.set(.think, q.func(subs.SUB_Remove));
    newmis.set(.classname, S("rocket"));

    q.setmodel(newmis, S("progs/missile.mdl"));
    q.setsize(newmis, q.vec(0, 0, 0), q.vec(0, 0, 0));
    q.setorigin(newmis, self.get(.origin) + q.scale(q.v_forward.get(), 8) + q.vec(0, 0, 16));
}

// ===============================================================================
//
// LIGHTNING
//
// ===============================================================================

pub export fn LightningHit(from: Entity, damage: f32) void {
    const e = q.trace_endpos.get();
    q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
    q.WriteByte(q.MSG_MULTICAST, q.TE_LIGHTNINGBLOOD);
    q.WriteCoord(q.MSG_MULTICAST, e[0]);
    q.WriteCoord(q.MSG_MULTICAST, e[1]);
    q.WriteCoord(q.MSG_MULTICAST, e[2]);
    q.multicast(e, q.MULTICAST_PVS);

    combat.T_Damage(q.trace_ent.get(), from, from, damage);
}

/// LightningDamage
pub export fn LightningDamage(p1: Vector, p2: Vector, from: Entity, damage: f32) void {
    const self = q.me();

    var f = p2 - p1;
    _ = q.normalize(f); // (the result is discarded in the original)
    f[0] = 0 - f[1];
    f[1] = f[0];
    f[2] = 0;
    f = q.scale(f, 16);

    q.traceline(p1, p2, q.FALSE, self);
    if (q.trace_ent.get().get(.takedamage) != 0) {
        LightningHit(from, damage);
        if (self.get(.classname).eql(S("player"))) {
            if (q.other.get().get(.classname).eql(S("player"))) {
                const t = q.trace_ent.get();
                var v = t.get(.velocity);
                v[2] = v[2] + 400;
                t.set(.velocity, v);
            }
        }
    }
    const e1 = q.trace_ent.get();

    q.traceline(p1 + f, p2 + f, q.FALSE, self);
    if (q.trace_ent.get().neq(e1) and q.trace_ent.get().get(.takedamage) != 0) {
        LightningHit(from, damage);
    }
    const e2 = q.trace_ent.get();

    q.traceline(p1 - f, p2 - f, q.FALSE, self);
    if (q.trace_ent.get().neq(e1) and q.trace_ent.get().neq(e2) and q.trace_ent.get().get(.takedamage) != 0) {
        LightningHit(from, damage);
    }
}

pub export fn W_FireLightning() void {
    const self = q.me();

    if (self.get(.ammo_cells) < 1) {
        self.set(.weapon, W_BestWeapon());
        W_SetCurrentAmmo();
        return;
    }

    // explode if under water
    if (self.get(.waterlevel) > 1) {
        if (q.deathmatch.get() > 3) {
            if (q.random() <= 0.5) {
                self.set(q.deathtype, S("selfwater"));
                combat.T_Damage(self, self, self.get(.owner), 4000);
            } else {
                const cells = self.get(.ammo_cells);
                self.set(.ammo_cells, 0);
                W_SetCurrentAmmo();
                combat.T_RadiusDamage(self, self, 35 * cells, q.world.get(), S(""));
                return;
            }
        } else {
            const cells = self.get(.ammo_cells);
            self.set(.ammo_cells, 0);
            W_SetCurrentAmmo();
            combat.T_RadiusDamage(self, self, 35 * cells, q.world.get(), S(""));
            return;
        }
    }

    if (self.get(q.t_width) < q.time.get()) {
        q.sound(self, q.CHAN_WEAPON, S("weapons/lhit.wav"), 1, q.ATTN_NORM);
        self.set(q.t_width, q.time.get() + 0.6);
    }
    q.msg_entity.set(self);
    q.WriteByte(q.MSG_ONE, q.SVC_SMALLKICK);

    if (q.deathmatch.get() != 4) {
        self.set(.ammo_cells, self.get(.ammo_cells) - 1);
        self.set(.currentammo, self.get(.ammo_cells));
    }

    const org = self.get(.origin) + q.vec(0, 0, 16);

    q.traceline(org, org + q.scale(q.v_forward.get(), 600), q.TRUE, self);

    const e = q.trace_endpos.get();
    q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
    q.WriteByte(q.MSG_MULTICAST, q.TE_LIGHTNING2);
    q.WriteEntity(q.MSG_MULTICAST, self);
    q.WriteCoord(q.MSG_MULTICAST, org[0]);
    q.WriteCoord(q.MSG_MULTICAST, org[1]);
    q.WriteCoord(q.MSG_MULTICAST, org[2]);
    q.WriteCoord(q.MSG_MULTICAST, e[0]);
    q.WriteCoord(q.MSG_MULTICAST, e[1]);
    q.WriteCoord(q.MSG_MULTICAST, e[2]);
    q.multicast(org, q.MULTICAST_PHS);

    LightningDamage(self.get(.origin), e + q.scale(q.v_forward.get(), 4), self, 30);
}

//=============================================================================

pub export fn GrenadeExplode() void {
    const self = q.me();
    if (self.get(q.voided) != 0) {
        return;
    }
    self.set(q.voided, 1);

    combat.T_RadiusDamage(self, self.get(.owner), 120, q.world.get(), S("grenade"));

    const o = self.get(.origin);
    q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
    q.WriteByte(q.MSG_MULTICAST, q.TE_EXPLOSION);
    q.WriteCoord(q.MSG_MULTICAST, o[0]);
    q.WriteCoord(q.MSG_MULTICAST, o[1]);
    q.WriteCoord(q.MSG_MULTICAST, o[2]);
    q.multicast(o, q.MULTICAST_PHS);

    q.remove(self);
}

pub export fn GrenadeTouch() void {
    const self = q.me();
    const other = q.other.get();
    if (other.eql(self.get(.owner)))
        return; // don't explode on owner
    if (other.get(.takedamage) == q.DAMAGE_AIM) {
        GrenadeExplode();
        return;
    }
    q.sound(self, q.CHAN_WEAPON, S("weapons/bounce.wav"), 1, q.ATTN_NORM); // bounce sound
    if (q.veq(self.get(.velocity), q.vec(0, 0, 0)))
        self.set(.avelocity, q.vec(0, 0, 0));
}

/// W_FireGrenade
pub export fn W_FireGrenade() void {
    const self = q.me();

    if (q.deathmatch.get() != 4) {
        self.set(.ammo_rockets, self.get(.ammo_rockets) - 1);
        self.set(.currentammo, self.get(.ammo_rockets));
    }

    q.sound(self, q.CHAN_WEAPON, S("weapons/grenade.wav"), 1, q.ATTN_NORM);

    q.msg_entity.set(self);
    q.WriteByte(q.MSG_ONE, q.SVC_SMALLKICK);

    q.newmis.set(q.spawn());
    const newmis = q.newmis.get();
    newmis.set(q.voided, 0);
    newmis.set(.owner, self);
    newmis.set(.movetype, q.MOVETYPE_BOUNCE);
    newmis.set(.solid, q.SOLID_BBOX);
    newmis.set(.classname, S("grenade"));

    // set newmis speed

    q.makevectors(self.get(.v_angle));

    if (self.get(.v_angle)[0] != 0) {
        newmis.set(.velocity, q.scale(q.v_forward.get(), 600) + q.scale(q.v_up.get(), 200) + q.scale(q.v_right.get(), crandom() * 10) + q.scale(q.v_up.get(), crandom() * 10));
    } else {
        newmis.set(.velocity, q.aim(self, 10000));
        var v = q.scale(newmis.get(.velocity), 600);
        v[2] = 200;
        newmis.set(.velocity, v);
    }

    newmis.set(.avelocity, q.vec(300, 300, 300));

    newmis.set(.angles, q.vectoangles(newmis.get(.velocity)));

    newmis.set(.touch, q.func(GrenadeTouch));

    // set newmis duration
    if (q.deathmatch.get() == 4) {
        newmis.set(.nextthink, q.time.get() + 2.5);
        self.set(q.attack_finished, q.time.get() + 1.1);
        //      self.health = self.health - 1;
        combat.T_Damage(self, self, self.get(.owner), 10);
    } else {
        newmis.set(.nextthink, q.time.get() + 2.5);
    }

    newmis.set(.think, q.func(GrenadeExplode));

    q.setmodel(newmis, S("progs/grenade.mdl"));
    q.setsize(newmis, q.vec(0, 0, 0), q.vec(0, 0, 0));
    q.setorigin(newmis, self.get(.origin));
}

//=============================================================================

/// launch_spike
///
/// Used for both the player and the ogre
pub export fn launch_spike(org: Vector, dir: Vector) void {
    const self = q.me();
    q.newmis.set(q.spawn());
    const newmis = q.newmis.get();
    newmis.set(q.voided, 0);
    newmis.set(.owner, self);
    newmis.set(.movetype, q.MOVETYPE_FLYMISSILE);
    newmis.set(.solid, q.SOLID_BBOX);

    newmis.set(.angles, q.vectoangles(dir));

    newmis.set(.touch, q.func(spike_touch));
    newmis.set(.classname, S("spike"));
    newmis.set(.think, q.func(subs.SUB_Remove));
    newmis.set(.nextthink, q.time.get() + 6);
    q.setmodel(newmis, S("progs/spike.mdl"));
    q.setsize(newmis, q.VEC_ORIGIN, q.VEC_ORIGIN);
    q.setorigin(newmis, org);

    newmis.set(.velocity, q.scale(dir, 1000));
}

pub export fn W_FireSuperSpikes() void {
    const self = q.me();

    q.sound(self, q.CHAN_WEAPON, S("weapons/spike2.wav"), 1, q.ATTN_NORM);
    self.set(q.attack_finished, q.time.get() + 0.2);
    if (q.deathmatch.get() != 4) {
        self.set(.ammo_nails, self.get(.ammo_nails) - 2);
        self.set(.currentammo, self.get(.ammo_nails));
    }
    const dir = q.aim(self, 1000);
    launch_spike(self.get(.origin) + q.vec(0, 0, 16), dir);
    const newmis = q.newmis.get();
    newmis.set(.touch, q.func(superspike_touch));
    q.setmodel(newmis, S("progs/s_spike.mdl"));
    q.setsize(newmis, q.VEC_ORIGIN, q.VEC_ORIGIN);
    q.msg_entity.set(self);
    q.WriteByte(q.MSG_ONE, q.SVC_SMALLKICK);
}

pub export fn W_FireSpikes(ox: f32) void {
    const self = q.me();

    q.makevectors(self.get(.v_angle));

    if (self.get(.ammo_nails) >= 2 and self.get(.weapon) == q.IT_SUPER_NAILGUN) {
        W_FireSuperSpikes();
        return;
    }

    if (self.get(.ammo_nails) < 1) {
        self.set(.weapon, W_BestWeapon());
        W_SetCurrentAmmo();
        return;
    }

    q.sound(self, q.CHAN_WEAPON, S("weapons/rocket1i.wav"), 1, q.ATTN_NORM);
    self.set(q.attack_finished, q.time.get() + 0.2);
    if (q.deathmatch.get() != 4) {
        self.set(.ammo_nails, self.get(.ammo_nails) - 1);
        self.set(.currentammo, self.get(.ammo_nails));
    }
    const dir = q.aim(self, 1000);
    launch_spike(self.get(.origin) + q.vec(0, 0, 16) + q.scale(q.v_right.get(), ox), dir);

    q.msg_entity.set(self);
    q.WriteByte(q.MSG_ONE, q.SVC_SMALLKICK);
}

pub const hit_z = q.userField(f32, "hit_z");

pub export fn spike_touch() void {
    const self = q.me();
    const other = q.other.get();

    if (other.eql(self.get(.owner)))
        return;

    if (self.get(q.voided) != 0) {
        return;
    }
    self.set(q.voided, 1);

    if (other.get(.solid) == q.SOLID_TRIGGER)
        return; // trigger field, do nothing

    if (q.pointcontents(self.get(.origin)) == q.CONTENT_SKY) {
        q.remove(self);
        return;
    }

    // hit something that bleeds
    if (other.get(.takedamage) != 0) {
        spawn_touchblood(9);
        other.set(q.deathtype, S("nail"));
        combat.T_Damage(other, self, self.get(.owner), 9);
    } else {
        q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
        if (self.get(.classname).eql(S("wizspike"))) {
            q.WriteByte(q.MSG_MULTICAST, q.TE_WIZSPIKE);
        } else if (self.get(.classname).eql(S("knightspike"))) {
            q.WriteByte(q.MSG_MULTICAST, q.TE_KNIGHTSPIKE);
        } else {
            q.WriteByte(q.MSG_MULTICAST, q.TE_SPIKE);
        }
        const o = self.get(.origin);
        q.WriteCoord(q.MSG_MULTICAST, o[0]);
        q.WriteCoord(q.MSG_MULTICAST, o[1]);
        q.WriteCoord(q.MSG_MULTICAST, o[2]);
        q.multicast(o, q.MULTICAST_PHS);
    }

    q.remove(self);
}

pub export fn superspike_touch() void {
    const self = q.me();
    const other = q.other.get();

    if (other.eql(self.get(.owner)))
        return;

    if (self.get(q.voided) != 0) {
        return;
    }
    self.set(q.voided, 1);

    if (other.get(.solid) == q.SOLID_TRIGGER)
        return; // trigger field, do nothing

    if (q.pointcontents(self.get(.origin)) == q.CONTENT_SKY) {
        q.remove(self);
        return;
    }

    // hit something that bleeds
    if (other.get(.takedamage) != 0) {
        spawn_touchblood(18);
        other.set(q.deathtype, S("supernail"));
        combat.T_Damage(other, self, self.get(.owner), 18);
    } else {
        const o = self.get(.origin);
        q.WriteByte(q.MSG_MULTICAST, q.SVC_TEMPENTITY);
        q.WriteByte(q.MSG_MULTICAST, q.TE_SUPERSPIKE);
        q.WriteCoord(q.MSG_MULTICAST, o[0]);
        q.WriteCoord(q.MSG_MULTICAST, o[1]);
        q.WriteCoord(q.MSG_MULTICAST, o[2]);
        q.multicast(o, q.MULTICAST_PHS);
    }

    q.remove(self);
}

// ===============================================================================
//
// PLAYER WEAPON USE
//
// ===============================================================================

pub export fn W_SetCurrentAmmo() void {
    const self = q.me();
    player.player_run(); // get out of any weapon firing states

    self.set(.items, self.get(.items) - q.band(self.get(.items), q.bor(q.bor(q.IT_SHELLS, q.IT_NAILS), q.bor(q.IT_ROCKETS, q.IT_CELLS))));

    const weapon = self.get(.weapon);
    if (weapon == q.IT_AXE) {
        self.set(.currentammo, 0);
        self.set(.weaponmodel, S("progs/v_axe.mdl"));
        self.set(.weaponframe, 0);
    } else if (weapon == q.IT_SHOTGUN) {
        self.set(.currentammo, self.get(.ammo_shells));
        self.set(.weaponmodel, S("progs/v_shot.mdl"));
        self.set(.weaponframe, 0);
        self.set(.items, q.bor(self.get(.items), q.IT_SHELLS));
    } else if (weapon == q.IT_SUPER_SHOTGUN) {
        self.set(.currentammo, self.get(.ammo_shells));
        self.set(.weaponmodel, S("progs/v_shot2.mdl"));
        self.set(.weaponframe, 0);
        self.set(.items, q.bor(self.get(.items), q.IT_SHELLS));
    } else if (weapon == q.IT_NAILGUN) {
        self.set(.currentammo, self.get(.ammo_nails));
        self.set(.weaponmodel, S("progs/v_nail.mdl"));
        self.set(.weaponframe, 0);
        self.set(.items, q.bor(self.get(.items), q.IT_NAILS));
    } else if (weapon == q.IT_SUPER_NAILGUN) {
        self.set(.currentammo, self.get(.ammo_nails));
        self.set(.weaponmodel, S("progs/v_nail2.mdl"));
        self.set(.weaponframe, 0);
        self.set(.items, q.bor(self.get(.items), q.IT_NAILS));
    } else if (weapon == q.IT_GRENADE_LAUNCHER) {
        self.set(.currentammo, self.get(.ammo_rockets));
        self.set(.weaponmodel, S("progs/v_rock.mdl"));
        self.set(.weaponframe, 0);
        self.set(.items, q.bor(self.get(.items), q.IT_ROCKETS));
    } else if (weapon == q.IT_ROCKET_LAUNCHER) {
        self.set(.currentammo, self.get(.ammo_rockets));
        self.set(.weaponmodel, S("progs/v_rock2.mdl"));
        self.set(.weaponframe, 0);
        self.set(.items, q.bor(self.get(.items), q.IT_ROCKETS));
    } else if (weapon == q.IT_LIGHTNING) {
        self.set(.currentammo, self.get(.ammo_cells));
        self.set(.weaponmodel, S("progs/v_light.mdl"));
        self.set(.weaponframe, 0);
        self.set(.items, q.bor(self.get(.items), q.IT_CELLS));
    } else {
        self.set(.currentammo, 0);
        self.set(.weaponmodel, S(""));
        self.set(.weaponframe, 0);
    }
}

pub export fn W_BestWeapon() f32 {
    const self = q.me();

    const it = self.get(.items);

    if (self.get(.waterlevel) <= 1 and self.get(.ammo_cells) >= 1 and q.has(it, q.IT_LIGHTNING)) {
        return q.IT_LIGHTNING;
    } else if (self.get(.ammo_nails) >= 2 and q.has(it, q.IT_SUPER_NAILGUN)) {
        return q.IT_SUPER_NAILGUN;
    } else if (self.get(.ammo_shells) >= 2 and q.has(it, q.IT_SUPER_SHOTGUN)) {
        return q.IT_SUPER_SHOTGUN;
    } else if (self.get(.ammo_nails) >= 1 and q.has(it, q.IT_NAILGUN)) {
        return q.IT_NAILGUN;
    } else if (self.get(.ammo_shells) >= 1 and q.has(it, q.IT_SHOTGUN)) {
        return q.IT_SHOTGUN;
    }

    //  if(self.ammo_rockets >= 1 && (it & IT_ROCKET_LAUNCHER) )
    //      return IT_ROCKET_LAUNCHER;
    //  else if(self.ammo_rockets >= 1 && (it & IT_GRENADE_LAUNCHER) )
    //      return IT_GRENADE_LAUNCHER;

    return q.IT_AXE;
}

pub export fn W_CheckNoAmmo() f32 {
    const self = q.me();
    if (self.get(.currentammo) > 0)
        return q.TRUE;

    if (self.get(.weapon) == q.IT_AXE)
        return q.TRUE;

    self.set(.weapon, W_BestWeapon());

    W_SetCurrentAmmo();

    // drop the weapon down
    return q.FALSE;
}

/// W_Attack
///
/// An attack impulse can be triggered now
pub export fn W_Attack() void {
    const self = q.me();

    if (W_CheckNoAmmo() == 0)
        return;

    q.makevectors(self.get(.v_angle)); // calculate forward angle for velocity
    self.set(q.show_hostile, q.time.get() + 1); // wake monsters up

    const weapon = self.get(.weapon);
    if (weapon == q.IT_AXE) {
        self.set(q.attack_finished, q.time.get() + 0.5);
        q.sound(self, q.CHAN_WEAPON, S("weapons/ax1.wav"), 1, q.ATTN_NORM);
        const r = q.random();
        if (r < 0.25) {
            player.player_axe1();
        } else if (r < 0.5) {
            player.player_axeb1();
        } else if (r < 0.75) {
            player.player_axec1();
        } else {
            player.player_axed1();
        }
    } else if (weapon == q.IT_SHOTGUN) {
        player.player_shot1();
        self.set(q.attack_finished, q.time.get() + 0.5);
        W_FireShotgun();
    } else if (weapon == q.IT_SUPER_SHOTGUN) {
        player.player_shot1();
        self.set(q.attack_finished, q.time.get() + 0.7);
        W_FireSuperShotgun();
    } else if (weapon == q.IT_NAILGUN) {
        player.player_nail1();
    } else if (weapon == q.IT_SUPER_NAILGUN) {
        player.player_nail1();
    } else if (weapon == q.IT_GRENADE_LAUNCHER) {
        player.player_rocket1();
        self.set(q.attack_finished, q.time.get() + 0.6);
        W_FireGrenade();
    } else if (weapon == q.IT_ROCKET_LAUNCHER) {
        player.player_rocket1();
        self.set(q.attack_finished, q.time.get() + 0.8);
        W_FireRocket();
    } else if (weapon == q.IT_LIGHTNING) {
        self.set(q.attack_finished, q.time.get() + 0.1);
        q.sound(self, q.CHAN_AUTO, S("weapons/lstart.wav"), 1, q.ATTN_NORM);
        player.player_light1();
    }
}

/// W_ChangeWeapon
pub export fn W_ChangeWeapon() void {
    const self = q.me();
    var am: f32 = 0;
    var fl: f32 = 0;

    const impulse = self.get(.impulse);
    if (impulse == 1) {
        fl = q.IT_AXE;
    } else if (impulse == 2) {
        fl = q.IT_SHOTGUN;
        if (self.get(.ammo_shells) < 1)
            am = 1;
    } else if (impulse == 3) {
        fl = q.IT_SUPER_SHOTGUN;
        if (self.get(.ammo_shells) < 2)
            am = 1;
    } else if (impulse == 4) {
        fl = q.IT_NAILGUN;
        if (self.get(.ammo_nails) < 1)
            am = 1;
    } else if (impulse == 5) {
        fl = q.IT_SUPER_NAILGUN;
        if (self.get(.ammo_nails) < 2)
            am = 1;
    } else if (impulse == 6) {
        fl = q.IT_GRENADE_LAUNCHER;
        if (self.get(.ammo_rockets) < 1)
            am = 1;
    } else if (impulse == 7) {
        fl = q.IT_ROCKET_LAUNCHER;
        if (self.get(.ammo_rockets) < 1)
            am = 1;
    } else if (impulse == 8) {
        fl = q.IT_LIGHTNING;
        if (self.get(.ammo_cells) < 1)
            am = 1;
    }

    self.set(.impulse, 0);

    if (!q.has(self.get(.items), fl)) { // don't have the weapon or the ammo
        q.sprint(self, q.PRINT_HIGH, S("no weapon.\n"));
        return;
    }

    if (am != 0) { // don't have the ammo
        q.sprint(self, q.PRINT_HIGH, S("not enough ammo.\n"));
        return;
    }

    //
    // set weapon, set ammo
    //
    self.set(.weapon, fl);
    W_SetCurrentAmmo();
}

/// The original disables `CheatCommand` with an unconditional `return;`
/// (`//      if (deathmatch || coop)`).
const cheats_enabled = false;

/// CheatCommand
pub export fn CheatCommand() void {
    if (cheats_enabled) {
        const self = q.me();
        self.set(.ammo_rockets, 100);
        self.set(.ammo_nails, 200);
        self.set(.ammo_shells, 100);
        self.set(.items, q.bor(self.get(.items), q.IT_AXE + q.IT_SHOTGUN + q.IT_SUPER_SHOTGUN + q.IT_NAILGUN +
            q.IT_SUPER_NAILGUN + q.IT_GRENADE_LAUNCHER + q.IT_ROCKET_LAUNCHER + q.IT_KEY1 + q.IT_KEY2));

        self.set(.ammo_cells, 200);
        self.set(.items, q.bor(self.get(.items), q.IT_LIGHTNING));

        self.set(.weapon, q.IT_ROCKET_LAUNCHER);
        self.set(.impulse, 0);
        W_SetCurrentAmmo();
    }
}

/// CycleWeaponCommand
///
/// Go to the next weapon with ammo
pub export fn CycleWeaponCommand() void {
    const self = q.me();

    self.set(.impulse, 0);

    while (true) {
        var am: f32 = 0;

        const weapon = self.get(.weapon);
        if (weapon == q.IT_LIGHTNING) {
            self.set(.weapon, q.IT_AXE);
        } else if (weapon == q.IT_AXE) {
            self.set(.weapon, q.IT_SHOTGUN);
            if (self.get(.ammo_shells) < 1)
                am = 1;
        } else if (weapon == q.IT_SHOTGUN) {
            self.set(.weapon, q.IT_SUPER_SHOTGUN);
            if (self.get(.ammo_shells) < 2)
                am = 1;
        } else if (weapon == q.IT_SUPER_SHOTGUN) {
            self.set(.weapon, q.IT_NAILGUN);
            if (self.get(.ammo_nails) < 1)
                am = 1;
        } else if (weapon == q.IT_NAILGUN) {
            self.set(.weapon, q.IT_SUPER_NAILGUN);
            if (self.get(.ammo_nails) < 2)
                am = 1;
        } else if (weapon == q.IT_SUPER_NAILGUN) {
            self.set(.weapon, q.IT_GRENADE_LAUNCHER);
            if (self.get(.ammo_rockets) < 1)
                am = 1;
        } else if (weapon == q.IT_GRENADE_LAUNCHER) {
            self.set(.weapon, q.IT_ROCKET_LAUNCHER);
            if (self.get(.ammo_rockets) < 1)
                am = 1;
        } else if (weapon == q.IT_ROCKET_LAUNCHER) {
            self.set(.weapon, q.IT_LIGHTNING);
            if (self.get(.ammo_cells) < 1)
                am = 1;
        }

        if (q.has(self.get(.items), self.get(.weapon)) and am == 0) {
            W_SetCurrentAmmo();
            return;
        }
    }
}

/// CycleWeaponReverseCommand
///
/// Go to the prev weapon with ammo
pub export fn CycleWeaponReverseCommand() void {
    const self = q.me();

    const it = self.get(.items);
    self.set(.impulse, 0);

    while (true) {
        var am: f32 = 0;

        const weapon = self.get(.weapon);
        if (weapon == q.IT_LIGHTNING) {
            self.set(.weapon, q.IT_ROCKET_LAUNCHER);
            if (self.get(.ammo_rockets) < 1)
                am = 1;
        } else if (weapon == q.IT_ROCKET_LAUNCHER) {
            self.set(.weapon, q.IT_GRENADE_LAUNCHER);
            if (self.get(.ammo_rockets) < 1)
                am = 1;
        } else if (weapon == q.IT_GRENADE_LAUNCHER) {
            self.set(.weapon, q.IT_SUPER_NAILGUN);
            if (self.get(.ammo_nails) < 2)
                am = 1;
        } else if (weapon == q.IT_SUPER_NAILGUN) {
            self.set(.weapon, q.IT_NAILGUN);
            if (self.get(.ammo_nails) < 1)
                am = 1;
        } else if (weapon == q.IT_NAILGUN) {
            self.set(.weapon, q.IT_SUPER_SHOTGUN);
            if (self.get(.ammo_shells) < 2)
                am = 1;
        } else if (weapon == q.IT_SUPER_SHOTGUN) {
            self.set(.weapon, q.IT_SHOTGUN);
            if (self.get(.ammo_shells) < 1)
                am = 1;
        } else if (weapon == q.IT_SHOTGUN) {
            self.set(.weapon, q.IT_AXE);
        } else if (weapon == q.IT_AXE) {
            self.set(.weapon, q.IT_LIGHTNING);
            if (self.get(.ammo_cells) < 1)
                am = 1;
        }

        if (q.has(it, self.get(.weapon)) and am == 0) {
            W_SetCurrentAmmo();
            return;
        }
    }
}

/// ServerflagsCommand
///
/// Just for development
pub export fn ServerflagsCommand() void {
    q.serverflags.set(q.serverflags.get() * 2 + 1);
}

/// ImpulseCommands
pub export fn ImpulseCommands() void {
    const self = q.me();
    if (self.get(.impulse) >= 1 and self.get(.impulse) <= 8)
        W_ChangeWeapon();

    if (self.get(.impulse) == 9)
        CheatCommand();
    if (self.get(.impulse) == 10)
        CycleWeaponCommand();
    if (self.get(.impulse) == 11)
        ServerflagsCommand();
    if (self.get(.impulse) == 12)
        CycleWeaponReverseCommand();

    self.set(.impulse, 0);
}

/// W_WeaponFrame
///
/// Called every frame so impulse events can be handled as well as possible
pub export fn W_WeaponFrame() void {
    const self = q.me();
    if (q.time.get() < self.get(q.attack_finished))
        return;

    ImpulseCommands();

    // check for attack
    if (self.get(.button0) != 0) {
        SuperDamageSound();
        W_Attack();
    }
}

/// SuperDamageSound
///
/// Plays sound if needed
pub export fn SuperDamageSound() void {
    const self = q.me();
    if (self.get(q.super_damage_finished) > q.time.get()) {
        if (self.get(q.super_sound) < q.time.get()) {
            self.set(q.super_sound, q.time.get() + 1);
            q.sound(self, q.CHAN_BODY, S("items/damage3.wav"), 1, q.ATTN_NORM);
        }
    }
    return;
}
