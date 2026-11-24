//! player.qc
//!
//! player functions/definitions
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
const String = q.String;
const Vector = q.Vector;

const client = @import("client.zig");
const items = @import("items.zig");
const misc = @import("misc.zig");
const subs = @import("subs.zig");
const weapons = @import("weapons.zig");

// ==============================================================================
//
// PLAYER
//
// ==============================================================================

// $cd /raid/quake/id1/models/player_4
// $origin 0 -6 24
// $base base
// $skin skin

/// The model's frames (`$frame ...`), numbered in order.
const Frame = enum(u8) {
    //
    // running
    //
    axrun1,
    axrun2,
    axrun3,
    axrun4,
    axrun5,
    axrun6,

    rockrun1,
    rockrun2,
    rockrun3,
    rockrun4,
    rockrun5,
    rockrun6,

    //
    // standing
    //
    stand1,
    stand2,
    stand3,
    stand4,
    stand5,

    axstnd1,
    axstnd2,
    axstnd3,
    axstnd4,
    axstnd5,
    axstnd6,
    axstnd7,
    axstnd8,
    axstnd9,
    axstnd10,
    axstnd11,
    axstnd12,

    //
    // pain
    //
    axpain1,
    axpain2,
    axpain3,
    axpain4,
    axpain5,
    axpain6,

    pain1,
    pain2,
    pain3,
    pain4,
    pain5,
    pain6,

    //
    // death
    //
    axdeth1,
    axdeth2,
    axdeth3,
    axdeth4,
    axdeth5,
    axdeth6,
    axdeth7,
    axdeth8,
    axdeth9,

    deatha1,
    deatha2,
    deatha3,
    deatha4,
    deatha5,
    deatha6,
    deatha7,
    deatha8,
    deatha9,
    deatha10,
    deatha11,

    deathb1,
    deathb2,
    deathb3,
    deathb4,
    deathb5,
    deathb6,
    deathb7,
    deathb8,
    deathb9,

    deathc1,
    deathc2,
    deathc3,
    deathc4,
    deathc5,
    deathc6,
    deathc7,
    deathc8,
    deathc9,
    deathc10,
    deathc11,
    deathc12,
    deathc13,
    deathc14,
    deathc15,

    deathd1,
    deathd2,
    deathd3,
    deathd4,
    deathd5,
    deathd6,
    deathd7,
    deathd8,
    deathd9,

    deathe1,
    deathe2,
    deathe3,
    deathe4,
    deathe5,
    deathe6,
    deathe7,
    deathe8,
    deathe9,

    //
    // attacks
    //
    nailatt1,
    nailatt2,

    light1,
    light2,

    rockatt1,
    rockatt2,
    rockatt3,
    rockatt4,
    rockatt5,
    rockatt6,

    shotatt1,
    shotatt2,
    shotatt3,
    shotatt4,
    shotatt5,
    shotatt6,

    axatt1,
    axatt2,
    axatt3,
    axatt4,
    axatt5,
    axatt6,

    axattb1,
    axattb2,
    axattb3,
    axattb4,
    axattb5,
    axattb6,

    axattc1,
    axattc2,
    axattc3,
    axattc4,
    axattc5,
    axattc6,

    axattd1,
    axattd2,
    axattd3,
    axattd4,
    axattd5,
    axattd6,
};

/// `$name`: a frame number.
inline fn fr(comptime frame: Frame) f32 {
    return @floatFromInt(@backingInt(frame));
}

/// `[$frame, next]`.
inline fn state(comptime frame: Frame, comptime next: anytype) void {
    q.setState(fr(frame), q.func(next));
}

test "frame numbers" {
    const std = @import("std");
    try std.testing.expectEqual(@as(f32, 0), fr(.axrun1));
    try std.testing.expectEqual(@as(f32, 17), fr(.axstnd1));
    try std.testing.expectEqual(@as(f32, 49), fr(.deatha1));
    try std.testing.expectEqual(@as(f32, 120), fr(.axatt1));
}

// ==============================================================================
// PLAYER
// ==============================================================================

pub fn player_stand1() callconv(.c) void {
    state(.axstnd1, player_stand1);
    const self = q.me();
    self.weaponframe = 0;
    if (self.velocity[0] != 0 or self.velocity[1] != 0) {
        self.set(g.walkframe, 0);
        player_run();
        return;
    }

    if (self.weapon == q.IT_AXE) {
        if (self.get(g.walkframe) >= 12)
            self.set(g.walkframe, 0);
        self.frame = fr(.axstnd1) + self.get(g.walkframe);
    } else {
        if (self.get(g.walkframe) >= 5)
            self.set(g.walkframe, 0);
        self.frame = fr(.stand1) + self.get(g.walkframe);
    }
    self.set(g.walkframe, self.get(g.walkframe) + 1);
}

pub fn player_run() callconv(.c) void {
    state(.rockrun1, player_run);
    const self = q.me();
    self.weaponframe = 0;
    if (self.velocity[0] == 0 and self.velocity[1] == 0) {
        self.set(g.walkframe, 0);
        player_stand1();
        return;
    }

    if (self.weapon == q.IT_AXE) {
        if (self.get(g.walkframe) == 6)
            self.set(g.walkframe, 0);
        self.frame = fr(.axrun1) + self.get(g.walkframe);
    } else {
        if (self.get(g.walkframe) == 6)
            self.set(g.walkframe, 0);
        // (`self.frame` here is `$rockrun1`, set by the state prefix.)
        self.frame = self.frame + self.get(g.walkframe);
    }
    self.set(g.walkframe, self.get(g.walkframe) + 1);
}

pub fn muzzleflash() callconv(.c) void {
    const self = q.me();
    q.WriteByte(q.MSG_MULTICAST, q.SVC_MUZZLEFLASH);
    q.WriteEntity(q.MSG_MULTICAST, self);
    q.multicast(self.origin, q.MULTICAST_PVS);
}

inline fn weaponframe(f: f32) void {
    q.me().weaponframe = f;
}

pub fn player_shot1() callconv(.c) void {
    state(.shotatt1, player_shot2);
    weaponframe(1);
    muzzleflash();
}
pub fn player_shot2() callconv(.c) void {
    state(.shotatt2, player_shot3);
    weaponframe(2);
}
pub fn player_shot3() callconv(.c) void {
    state(.shotatt3, player_shot4);
    weaponframe(3);
}
pub fn player_shot4() callconv(.c) void {
    state(.shotatt4, player_shot5);
    weaponframe(4);
}
pub fn player_shot5() callconv(.c) void {
    state(.shotatt5, player_shot6);
    weaponframe(5);
}
pub fn player_shot6() callconv(.c) void {
    state(.shotatt6, player_run);
    weaponframe(6);
}

pub fn player_axe1() callconv(.c) void {
    state(.axatt1, player_axe2);
    weaponframe(1);
}
pub fn player_axe2() callconv(.c) void {
    state(.axatt2, player_axe3);
    weaponframe(2);
}
pub fn player_axe3() callconv(.c) void {
    state(.axatt3, player_axe4);
    weaponframe(3);
    weapons.W_FireAxe();
}
pub fn player_axe4() callconv(.c) void {
    state(.axatt4, player_run);
    weaponframe(4);
}

pub fn player_axeb1() callconv(.c) void {
    state(.axattb1, player_axeb2);
    weaponframe(5);
}
pub fn player_axeb2() callconv(.c) void {
    state(.axattb2, player_axeb3);
    weaponframe(6);
}
pub fn player_axeb3() callconv(.c) void {
    state(.axattb3, player_axeb4);
    weaponframe(7);
    weapons.W_FireAxe();
}
pub fn player_axeb4() callconv(.c) void {
    state(.axattb4, player_run);
    weaponframe(8);
}

pub fn player_axec1() callconv(.c) void {
    state(.axattc1, player_axec2);
    weaponframe(1);
}
pub fn player_axec2() callconv(.c) void {
    state(.axattc2, player_axec3);
    weaponframe(2);
}
pub fn player_axec3() callconv(.c) void {
    state(.axattc3, player_axec4);
    weaponframe(3);
    weapons.W_FireAxe();
}
pub fn player_axec4() callconv(.c) void {
    state(.axattc4, player_run);
    weaponframe(4);
}

pub fn player_axed1() callconv(.c) void {
    state(.axattd1, player_axed2);
    weaponframe(5);
}
pub fn player_axed2() callconv(.c) void {
    state(.axattd2, player_axed3);
    weaponframe(6);
}
pub fn player_axed3() callconv(.c) void {
    state(.axattd3, player_axed4);
    weaponframe(7);
    weapons.W_FireAxe();
}
pub fn player_axed4() callconv(.c) void {
    state(.axattd4, player_run);
    weaponframe(8);
}

//============================================================================

pub fn player_nail1() callconv(.c) void {
    state(.nailatt1, player_nail2);
    const self = q.me();
    muzzleflash();

    if (self.button0 == 0 or client.intermission_running.get() != 0 or self.impulse != 0) {
        player_run();
        return;
    }
    self.weaponframe = self.weaponframe + 1;
    if (self.weaponframe == 9)
        self.weaponframe = 1;
    weapons.SuperDamageSound();
    weapons.W_FireSpikes(4);
    self.set(g.attack_finished, q.time.get() + 0.2);
}
pub fn player_nail2() callconv(.c) void {
    state(.nailatt2, player_nail1);
    const self = q.me();
    muzzleflash();

    if (self.button0 == 0 or client.intermission_running.get() != 0 or self.impulse != 0) {
        player_run();
        return;
    }
    self.weaponframe = self.weaponframe + 1;
    if (self.weaponframe == 9)
        self.weaponframe = 1;
    weapons.SuperDamageSound();
    weapons.W_FireSpikes(-4);
    self.set(g.attack_finished, q.time.get() + 0.2);
}

//============================================================================

pub fn player_light1() callconv(.c) void {
    state(.light1, player_light2);
    const self = q.me();
    muzzleflash();

    if (self.button0 == 0 or client.intermission_running.get() != 0) {
        player_run();
        return;
    }
    self.weaponframe = self.weaponframe + 1;
    if (self.weaponframe == 5)
        self.weaponframe = 1;
    weapons.SuperDamageSound();
    weapons.W_FireLightning();
    self.set(g.attack_finished, q.time.get() + 0.2);
}
pub fn player_light2() callconv(.c) void {
    state(.light2, player_light1);
    const self = q.me();
    muzzleflash();

    if (self.button0 == 0 or client.intermission_running.get() != 0) {
        player_run();
        return;
    }
    self.weaponframe = self.weaponframe + 1;
    if (self.weaponframe == 5)
        self.weaponframe = 1;
    weapons.SuperDamageSound();
    weapons.W_FireLightning();
    self.set(g.attack_finished, q.time.get() + 0.2);
}

//============================================================================

pub fn player_rocket1() callconv(.c) void {
    state(.rockatt1, player_rocket2);
    weaponframe(1);
    muzzleflash();
}
pub fn player_rocket2() callconv(.c) void {
    state(.rockatt2, player_rocket3);
    weaponframe(2);
}
pub fn player_rocket3() callconv(.c) void {
    state(.rockatt3, player_rocket4);
    weaponframe(3);
}
pub fn player_rocket4() callconv(.c) void {
    state(.rockatt4, player_rocket5);
    weaponframe(4);
}
pub fn player_rocket5() callconv(.c) void {
    state(.rockatt5, player_rocket6);
    weaponframe(5);
}
pub fn player_rocket6() callconv(.c) void {
    state(.rockatt6, player_run);
    weaponframe(6);
}

pub fn PainSound() callconv(.c) void {
    const self = q.me();

    if (self.health < 0)
        return;

    if (g.damage_attacker.get().classname.eql(S("teledeath"))) {
        q.sound(self, q.CHAN_VOICE, S("player/teledth1.wav"), 1, q.ATTN_NONE);
        return;
    }

    // water pain sounds
    if (self.watertype == q.CONTENT_WATER and self.waterlevel == 3) {
        DeathBubbles(1);
        if (q.random() > 0.5) {
            q.sound(self, q.CHAN_VOICE, S("player/drown1.wav"), 1, q.ATTN_NORM);
        } else {
            q.sound(self, q.CHAN_VOICE, S("player/drown2.wav"), 1, q.ATTN_NORM);
        }
        return;
    }

    // slime pain sounds
    if (self.watertype == q.CONTENT_SLIME) {
        // FIX ME       put in some steam here
        if (q.random() > 0.5) {
            q.sound(self, q.CHAN_VOICE, S("player/lburn1.wav"), 1, q.ATTN_NORM);
        } else {
            q.sound(self, q.CHAN_VOICE, S("player/lburn2.wav"), 1, q.ATTN_NORM);
        }
        return;
    }

    if (self.watertype == q.CONTENT_LAVA) {
        if (q.random() > 0.5) {
            q.sound(self, q.CHAN_VOICE, S("player/lburn1.wav"), 1, q.ATTN_NORM);
        } else {
            q.sound(self, q.CHAN_VOICE, S("player/lburn2.wav"), 1, q.ATTN_NORM);
        }
        return;
    }

    if (self.get(g.pain_finished) > q.time.get()) {
        self.set(g.axhitme, 0);
        return;
    }
    self.set(g.pain_finished, q.time.get() + 0.5);

    // don't make multiple pain sounds right after each other

    // ax pain sound
    if (self.get(g.axhitme) == 1) {
        self.set(g.axhitme, 0);
        q.sound(self, q.CHAN_VOICE, S("player/axhit1.wav"), 1, q.ATTN_NORM);
        return;
    }

    const rs = q.rint((q.random() * 5) + 1);

    self.noise = S("");
    if (rs == 1) {
        self.noise = S("player/pain1.wav");
    } else if (rs == 2) {
        self.noise = S("player/pain2.wav");
    } else if (rs == 3) {
        self.noise = S("player/pain3.wav");
    } else if (rs == 4) {
        self.noise = S("player/pain4.wav");
    } else if (rs == 5) {
        self.noise = S("player/pain5.wav");
    } else {
        self.noise = S("player/pain6.wav");
    }

    q.sound(self, q.CHAN_VOICE, self.noise, 1, q.ATTN_NORM);
    return;
}

pub fn player_pain1() callconv(.c) void {
    state(.pain1, player_pain2);
    PainSound();
    weaponframe(0);
}
pub fn player_pain2() callconv(.c) void {
    state(.pain2, player_pain3);
}
pub fn player_pain3() callconv(.c) void {
    state(.pain3, player_pain4);
}
pub fn player_pain4() callconv(.c) void {
    state(.pain4, player_pain5);
}
pub fn player_pain5() callconv(.c) void {
    state(.pain5, player_pain6);
}
pub fn player_pain6() callconv(.c) void {
    state(.pain6, player_run);
}

pub fn player_axpain1() callconv(.c) void {
    state(.axpain1, player_axpain2);
    PainSound();
    weaponframe(0);
}
pub fn player_axpain2() callconv(.c) void {
    state(.axpain2, player_axpain3);
}
pub fn player_axpain3() callconv(.c) void {
    state(.axpain3, player_axpain4);
}
pub fn player_axpain4() callconv(.c) void {
    state(.axpain4, player_axpain5);
}
pub fn player_axpain5() callconv(.c) void {
    state(.axpain5, player_axpain6);
}
pub fn player_axpain6() callconv(.c) void {
    state(.axpain6, player_run);
}

pub fn player_pain() callconv(.c) void {
    const self = q.me();
    if (self.weaponframe != 0)
        return;

    if (self.get(g.invisible_finished) > q.time.get())
        return; // eyes don't have pain frames

    if (self.weapon == q.IT_AXE) {
        player_axpain1();
    } else {
        player_pain1();
    }
}

pub fn DeathBubblesSpawn() callconv(.c) void {
    const self = q.me();
    const owner = self.owner;
    if (owner.waterlevel != 3)
        return;
    const bubble = q.spawn();
    q.setmodel(bubble, S("progs/s_bubble.spr"));
    q.setorigin(bubble, owner.origin + q.vec(0, 0, 24));
    bubble.movetype = q.MOVETYPE_NOCLIP;
    bubble.solid = q.SOLID_NOT;
    bubble.velocity = q.vec(0, 0, 15);
    bubble.nextthink = q.time.get() + 0.5;
    bubble.think = q.func(misc.bubble_bob);
    bubble.classname = S("bubble");
    bubble.frame = 0;
    bubble.set(g.cnt, 0);
    q.setsize(bubble, q.vec(-8, -8, -8), q.vec(8, 8, 8));
    self.nextthink = q.time.get() + 0.1;
    self.think = q.func(DeathBubblesSpawn);
    self.set(g.air_finished, self.get(g.air_finished) + 1);
    if (self.get(g.air_finished) >= self.get(g.bubble_count))
        q.remove(self);
}

pub fn DeathBubbles(num_bubbles: f32) callconv(.c) void {
    const self = q.me();

    const bubble_spawner = q.spawn();
    q.setorigin(bubble_spawner, self.origin);
    bubble_spawner.movetype = q.MOVETYPE_NONE;
    bubble_spawner.solid = q.SOLID_NOT;
    bubble_spawner.nextthink = q.time.get() + 0.1;
    bubble_spawner.think = q.func(DeathBubblesSpawn);
    bubble_spawner.set(g.air_finished, 0);
    bubble_spawner.owner = self;
    bubble_spawner.set(g.bubble_count, num_bubbles);
    return;
}

pub fn DeathSound() callconv(.c) void {
    const self = q.me();

    // water death sounds
    if (self.waterlevel == 3) {
        DeathBubbles(5);
        q.sound(self, q.CHAN_VOICE, S("player/h2odeath.wav"), 1, q.ATTN_NONE);
        return;
    }

    const rs = q.rint((q.random() * 4) + 1);
    if (rs == 1)
        self.noise = S("player/death1.wav");
    if (rs == 2)
        self.noise = S("player/death2.wav");
    if (rs == 3)
        self.noise = S("player/death3.wav");
    if (rs == 4)
        self.noise = S("player/death4.wav");
    if (rs == 5)
        self.noise = S("player/death5.wav");

    q.sound(self, q.CHAN_VOICE, self.noise, 1, q.ATTN_NONE);
    return;
}

pub fn PlayerDead() callconv(.c) void {
    const self = q.me();
    self.nextthink = -1;
    // allow respawn after a certain time
    self.deadflag = q.DEAD_DEAD;
}

pub fn VelocityForDamage(dm: f32) callconv(.c) Vector {
    const self = q.me();
    const inflictor = g.damage_inflictor.get();
    var v = q.vec(0, 0, 0);

    if (q.vlen(inflictor.velocity) > 0) {
        v = q.scale(inflictor.velocity, 0.5);
        v = v + q.scale(q.normalize(@as(Vector, self.origin) - inflictor.origin), 25);
        v[2] = 100 + 240 * q.random();
        v[0] = v[0] + (200 * weapons.crandom());
        v[1] = v[1] + (200 * weapons.crandom());
        //dprint ("Velocity gib\n");
    } else {
        v[0] = 100 * weapons.crandom();
        v[1] = 100 * weapons.crandom();
        v[2] = 200 + 100 * q.random();
    }

    //v_x = 100 * crandom();
    //v_y = 100 * crandom();
    //v_z = 200 + 100 * random();

    if (dm > -50) {
        //      dprint ("level 1\n");
        v = q.scale(v, 0.7);
    } else if (dm > -200) {
        //      dprint ("level 3\n");
        v = q.scale(v, 2);
    } else {
        v = q.scale(v, 10);
    }

    return v;
}

pub fn ThrowGib(gibname: String, dm: f32) callconv(.c) void {
    const self = q.me();

    const new = q.spawn();
    new.origin = self.origin;
    q.setmodel(new, gibname);
    q.setsize(new, q.vec(0, 0, 0), q.vec(0, 0, 0));
    new.velocity = VelocityForDamage(dm);
    new.movetype = q.MOVETYPE_BOUNCE;
    new.solid = q.SOLID_NOT;
    new.avelocity = q.vec(q.random() * 600, q.random() * 600, q.random() * 600);
    new.think = q.func(subs.SUB_Remove);
    new.ltime = q.time.get();
    new.nextthink = q.time.get() + 10 + q.random() * 10;
    new.frame = 0;
    new.flags = 0;
}

pub fn ThrowHead(gibname: String, dm: f32) callconv(.c) void {
    const self = q.me();
    q.setmodel(self, gibname);
    self.frame = 0;
    self.nextthink = -1;
    self.movetype = q.MOVETYPE_BOUNCE;
    self.takedamage = q.DAMAGE_NO;
    self.solid = q.SOLID_NOT;
    self.view_ofs = q.vec(0, 0, 8);
    q.setsize(self, q.vec(-16, -16, 0), q.vec(16, 16, 56));
    self.velocity = VelocityForDamage(dm);
    var origin = self.origin;
    origin[2] = origin[2] - 24;
    self.origin = origin;
    self.flags = self.flags - q.band(self.flags, q.FL_ONGROUND);
    self.avelocity = q.scale(q.vec(0, 600, 0), weapons.crandom());
}

pub fn GibPlayer() callconv(.c) void {
    const self = q.me();
    ThrowHead(S("progs/h_player.mdl"), self.health);
    ThrowGib(S("progs/gib1.mdl"), self.health);
    ThrowGib(S("progs/gib2.mdl"), self.health);
    ThrowGib(S("progs/gib3.mdl"), self.health);

    self.deadflag = q.DEAD_DEAD;

    if (g.damage_attacker.get().classname.eql(S("teledeath"))) {
        q.sound(self, q.CHAN_VOICE, S("player/teledth1.wav"), 1, q.ATTN_NONE);
        return;
    }

    if (g.damage_attacker.get().classname.eql(S("teledeath2"))) {
        q.sound(self, q.CHAN_VOICE, S("player/teledth1.wav"), 1, q.ATTN_NONE);
        return;
    }

    if (q.random() < 0.5) {
        q.sound(self, q.CHAN_VOICE, S("player/gib.wav"), 1, q.ATTN_NONE);
    } else {
        q.sound(self, q.CHAN_VOICE, S("player/udeath.wav"), 1, q.ATTN_NONE);
    }
}

pub fn PlayerDie() callconv(.c) void {
    const self = q.me();
    self.items = self.items - q.band(self.items, q.IT_INVISIBILITY);

    if (q.stof(q.infokey(q.world.get(), S("dq"))) != 0) {
        if (self.get(g.super_damage_finished) > 0) {
            items.DropQuad(self.get(g.super_damage_finished) - q.time.get());
            q.bprint(q.PRINT_LOW, self.netname);
            if (g.deathmatch.get() == 4) {
                q.bprint(q.PRINT_LOW, S(" lost an OctaPower with "));
            } else {
                q.bprint(q.PRINT_LOW, S(" lost a quad with "));
            }
            const s = q.ftos(q.rint(self.get(g.super_damage_finished) - q.time.get()));
            q.bprint(q.PRINT_LOW, s);
            q.bprint(q.PRINT_LOW, S(" seconds remaining\n"));
        }
    }

    if (q.stof(q.infokey(q.world.get(), S("dr"))) != 0) {
        if (self.get(g.invisible_finished) > 0) {
            q.bprint(q.PRINT_LOW, self.netname);
            q.bprint(q.PRINT_LOW, S(" lost a ring with "));
            const s = q.ftos(q.rint(self.get(g.invisible_finished) - q.time.get()));
            q.bprint(q.PRINT_LOW, s);
            q.bprint(q.PRINT_LOW, S(" seconds remaining\n"));
            items.DropRing(self.get(g.invisible_finished) - q.time.get());
        }
    }

    self.set(g.invisible_finished, 0); // don't die as eyes
    self.set(g.invincible_finished, 0);
    self.set(g.super_damage_finished, 0);
    self.set(g.radsuit_finished, 0);
    self.modelindex = client.modelindex_player.get(); // don't use eyes

    items.DropBackpack();

    self.weaponmodel = S("");
    self.view_ofs = q.vec(0, 0, -8);
    self.deadflag = q.DEAD_DYING;
    self.solid = q.SOLID_NOT;
    self.flags = self.flags - q.band(self.flags, q.FL_ONGROUND);
    self.movetype = q.MOVETYPE_TOSS;
    if (self.velocity[2] < 10) {
        var v = self.velocity;
        v[2] = v[2] + q.random() * 300;
        self.velocity = v;
    }

    if (self.health < -40) {
        GibPlayer();
        return;
    }

    DeathSound();

    var angles = self.angles;
    angles[0] = 0;
    angles[2] = 0;
    self.angles = angles;

    if (self.weapon == q.IT_AXE) {
        player_die_ax1();
        return;
    }

    var i = q.cvar(S("temp1"));
    if (i == 0)
        i = 1 + q.floor(q.random() * 6);

    if (i == 1) {
        player_diea1();
    } else if (i == 2) {
        player_dieb1();
    } else if (i == 3) {
        player_diec1();
    } else if (i == 4) {
        player_died1();
    } else {
        player_diee1();
    }
}

pub fn set_suicide_frame() callconv(.c) void {
    // used by klill command and diconnect command
    const self = q.me();
    if (self.model.neq(S("progs/player.mdl")))
        return; // allready gibbed
    self.frame = fr(.deatha11);
    self.solid = q.SOLID_NOT;
    self.movetype = q.MOVETYPE_TOSS;
    self.deadflag = q.DEAD_DEAD;
    self.nextthink = -1;
}

pub fn player_diea1() callconv(.c) void {
    state(.deatha1, player_diea2);
}
pub fn player_diea2() callconv(.c) void {
    state(.deatha2, player_diea3);
}
pub fn player_diea3() callconv(.c) void {
    state(.deatha3, player_diea4);
}
pub fn player_diea4() callconv(.c) void {
    state(.deatha4, player_diea5);
}
pub fn player_diea5() callconv(.c) void {
    state(.deatha5, player_diea6);
}
pub fn player_diea6() callconv(.c) void {
    state(.deatha6, player_diea7);
}
pub fn player_diea7() callconv(.c) void {
    state(.deatha7, player_diea8);
}
pub fn player_diea8() callconv(.c) void {
    state(.deatha8, player_diea9);
}
pub fn player_diea9() callconv(.c) void {
    state(.deatha9, player_diea10);
}
pub fn player_diea10() callconv(.c) void {
    state(.deatha10, player_diea11);
}
pub fn player_diea11() callconv(.c) void {
    state(.deatha11, player_diea11);
    PlayerDead();
}

pub fn player_dieb1() callconv(.c) void {
    state(.deathb1, player_dieb2);
}
pub fn player_dieb2() callconv(.c) void {
    state(.deathb2, player_dieb3);
}
pub fn player_dieb3() callconv(.c) void {
    state(.deathb3, player_dieb4);
}
pub fn player_dieb4() callconv(.c) void {
    state(.deathb4, player_dieb5);
}
pub fn player_dieb5() callconv(.c) void {
    state(.deathb5, player_dieb6);
}
pub fn player_dieb6() callconv(.c) void {
    state(.deathb6, player_dieb7);
}
pub fn player_dieb7() callconv(.c) void {
    state(.deathb7, player_dieb8);
}
pub fn player_dieb8() callconv(.c) void {
    state(.deathb8, player_dieb9);
}
pub fn player_dieb9() callconv(.c) void {
    state(.deathb9, player_dieb9);
    PlayerDead();
}

pub fn player_diec1() callconv(.c) void {
    state(.deathc1, player_diec2);
}
pub fn player_diec2() callconv(.c) void {
    state(.deathc2, player_diec3);
}
pub fn player_diec3() callconv(.c) void {
    state(.deathc3, player_diec4);
}
pub fn player_diec4() callconv(.c) void {
    state(.deathc4, player_diec5);
}
pub fn player_diec5() callconv(.c) void {
    state(.deathc5, player_diec6);
}
pub fn player_diec6() callconv(.c) void {
    state(.deathc6, player_diec7);
}
pub fn player_diec7() callconv(.c) void {
    state(.deathc7, player_diec8);
}
pub fn player_diec8() callconv(.c) void {
    state(.deathc8, player_diec9);
}
pub fn player_diec9() callconv(.c) void {
    state(.deathc9, player_diec10);
}
pub fn player_diec10() callconv(.c) void {
    state(.deathc10, player_diec11);
}
pub fn player_diec11() callconv(.c) void {
    state(.deathc11, player_diec12);
}
pub fn player_diec12() callconv(.c) void {
    state(.deathc12, player_diec13);
}
pub fn player_diec13() callconv(.c) void {
    state(.deathc13, player_diec14);
}
pub fn player_diec14() callconv(.c) void {
    state(.deathc14, player_diec15);
}
pub fn player_diec15() callconv(.c) void {
    state(.deathc15, player_diec15);
    PlayerDead();
}

pub fn player_died1() callconv(.c) void {
    state(.deathd1, player_died2);
}
pub fn player_died2() callconv(.c) void {
    state(.deathd2, player_died3);
}
pub fn player_died3() callconv(.c) void {
    state(.deathd3, player_died4);
}
pub fn player_died4() callconv(.c) void {
    state(.deathd4, player_died5);
}
pub fn player_died5() callconv(.c) void {
    state(.deathd5, player_died6);
}
pub fn player_died6() callconv(.c) void {
    state(.deathd6, player_died7);
}
pub fn player_died7() callconv(.c) void {
    state(.deathd7, player_died8);
}
pub fn player_died8() callconv(.c) void {
    state(.deathd8, player_died9);
}
pub fn player_died9() callconv(.c) void {
    state(.deathd9, player_died9);
    PlayerDead();
}

pub fn player_diee1() callconv(.c) void {
    state(.deathe1, player_diee2);
}
pub fn player_diee2() callconv(.c) void {
    state(.deathe2, player_diee3);
}
pub fn player_diee3() callconv(.c) void {
    state(.deathe3, player_diee4);
}
pub fn player_diee4() callconv(.c) void {
    state(.deathe4, player_diee5);
}
pub fn player_diee5() callconv(.c) void {
    state(.deathe5, player_diee6);
}
pub fn player_diee6() callconv(.c) void {
    state(.deathe6, player_diee7);
}
pub fn player_diee7() callconv(.c) void {
    state(.deathe7, player_diee8);
}
pub fn player_diee8() callconv(.c) void {
    state(.deathe8, player_diee9);
}
pub fn player_diee9() callconv(.c) void {
    state(.deathe9, player_diee9);
    PlayerDead();
}

pub fn player_die_ax1() callconv(.c) void {
    state(.axdeth1, player_die_ax2);
}
pub fn player_die_ax2() callconv(.c) void {
    state(.axdeth2, player_die_ax3);
}
pub fn player_die_ax3() callconv(.c) void {
    state(.axdeth3, player_die_ax4);
}
pub fn player_die_ax4() callconv(.c) void {
    state(.axdeth4, player_die_ax5);
}
pub fn player_die_ax5() callconv(.c) void {
    state(.axdeth5, player_die_ax6);
}
pub fn player_die_ax6() callconv(.c) void {
    state(.axdeth6, player_die_ax7);
}
pub fn player_die_ax7() callconv(.c) void {
    state(.axdeth7, player_die_ax8);
}
pub fn player_die_ax8() callconv(.c) void {
    state(.axdeth8, player_die_ax9);
}
pub fn player_die_ax9() callconv(.c) void {
    state(.axdeth9, player_die_ax9);
    PlayerDead();
}
