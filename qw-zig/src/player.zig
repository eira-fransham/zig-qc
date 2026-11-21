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
    axrun1, axrun2, axrun3, axrun4, axrun5, axrun6,

    rockrun1, rockrun2, rockrun3, rockrun4, rockrun5, rockrun6,

    //
    // standing
    //
    stand1, stand2, stand3, stand4, stand5,

    axstnd1, axstnd2, axstnd3, axstnd4, axstnd5, axstnd6,
    axstnd7, axstnd8, axstnd9, axstnd10, axstnd11, axstnd12,

    //
    // pain
    //
    axpain1, axpain2, axpain3, axpain4, axpain5, axpain6,

    pain1, pain2, pain3, pain4, pain5, pain6,

    //
    // death
    //
    axdeth1, axdeth2, axdeth3, axdeth4, axdeth5, axdeth6,
    axdeth7, axdeth8, axdeth9,

    deatha1, deatha2, deatha3, deatha4, deatha5, deatha6, deatha7, deatha8,
    deatha9, deatha10, deatha11,

    deathb1, deathb2, deathb3, deathb4, deathb5, deathb6, deathb7, deathb8,
    deathb9,

    deathc1, deathc2, deathc3, deathc4, deathc5, deathc6, deathc7, deathc8,
    deathc9, deathc10, deathc11, deathc12, deathc13, deathc14, deathc15,

    deathd1, deathd2, deathd3, deathd4, deathd5, deathd6, deathd7,
    deathd8, deathd9,

    deathe1, deathe2, deathe3, deathe4, deathe5, deathe6, deathe7,
    deathe8, deathe9,

    //
    // attacks
    //
    nailatt1, nailatt2,

    light1, light2,

    rockatt1, rockatt2, rockatt3, rockatt4, rockatt5, rockatt6,

    shotatt1, shotatt2, shotatt3, shotatt4, shotatt5, shotatt6,

    axatt1, axatt2, axatt3, axatt4, axatt5, axatt6,

    axattb1, axattb2, axattb3, axattb4, axattb5, axattb6,

    axattc1, axattc2, axattc3, axattc4, axattc5, axattc6,

    axattd1, axattd2, axattd3, axattd4, axattd5, axattd6,
};

/// `$name`: a frame number.
inline fn fr(comptime frame: Frame) f32 {
    return @floatFromInt(@intFromEnum(frame));
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

pub export fn player_stand1() void {
    state(.axstnd1, player_stand1);
    const self = q.me();
    self.set(.weaponframe, 0);
    if (self.get(.velocity)[0] != 0 or self.get(.velocity)[1] != 0) {
        self.set(q.walkframe, 0);
        player_run();
        return;
    }

    if (self.get(.weapon) == q.IT_AXE) {
        if (self.get(q.walkframe) >= 12)
            self.set(q.walkframe, 0);
        self.set(.frame, fr(.axstnd1) + self.get(q.walkframe));
    } else {
        if (self.get(q.walkframe) >= 5)
            self.set(q.walkframe, 0);
        self.set(.frame, fr(.stand1) + self.get(q.walkframe));
    }
    self.set(q.walkframe, self.get(q.walkframe) + 1);
}

pub export fn player_run() void {
    state(.rockrun1, player_run);
    const self = q.me();
    self.set(.weaponframe, 0);
    if (self.get(.velocity)[0] == 0 and self.get(.velocity)[1] == 0) {
        self.set(q.walkframe, 0);
        player_stand1();
        return;
    }

    if (self.get(.weapon) == q.IT_AXE) {
        if (self.get(q.walkframe) == 6)
            self.set(q.walkframe, 0);
        self.set(.frame, fr(.axrun1) + self.get(q.walkframe));
    } else {
        if (self.get(q.walkframe) == 6)
            self.set(q.walkframe, 0);
        // (`self.frame` here is `$rockrun1`, set by the state prefix.)
        self.set(.frame, self.get(.frame) + self.get(q.walkframe));
    }
    self.set(q.walkframe, self.get(q.walkframe) + 1);
}

pub export fn muzzleflash() void {
    const self = q.me();
    q.WriteByte(q.MSG_MULTICAST, q.SVC_MUZZLEFLASH);
    q.WriteEntity(q.MSG_MULTICAST, self);
    q.multicast(self.get(.origin), q.MULTICAST_PVS);
}

inline fn weaponframe(f: f32) void {
    q.me().set(.weaponframe, f);
}

pub export fn player_shot1() void {
    state(.shotatt1, player_shot2);
    weaponframe(1);
    muzzleflash();
}
pub export fn player_shot2() void {
    state(.shotatt2, player_shot3);
    weaponframe(2);
}
pub export fn player_shot3() void {
    state(.shotatt3, player_shot4);
    weaponframe(3);
}
pub export fn player_shot4() void {
    state(.shotatt4, player_shot5);
    weaponframe(4);
}
pub export fn player_shot5() void {
    state(.shotatt5, player_shot6);
    weaponframe(5);
}
pub export fn player_shot6() void {
    state(.shotatt6, player_run);
    weaponframe(6);
}

pub export fn player_axe1() void {
    state(.axatt1, player_axe2);
    weaponframe(1);
}
pub export fn player_axe2() void {
    state(.axatt2, player_axe3);
    weaponframe(2);
}
pub export fn player_axe3() void {
    state(.axatt3, player_axe4);
    weaponframe(3);
    weapons.W_FireAxe();
}
pub export fn player_axe4() void {
    state(.axatt4, player_run);
    weaponframe(4);
}

pub export fn player_axeb1() void {
    state(.axattb1, player_axeb2);
    weaponframe(5);
}
pub export fn player_axeb2() void {
    state(.axattb2, player_axeb3);
    weaponframe(6);
}
pub export fn player_axeb3() void {
    state(.axattb3, player_axeb4);
    weaponframe(7);
    weapons.W_FireAxe();
}
pub export fn player_axeb4() void {
    state(.axattb4, player_run);
    weaponframe(8);
}

pub export fn player_axec1() void {
    state(.axattc1, player_axec2);
    weaponframe(1);
}
pub export fn player_axec2() void {
    state(.axattc2, player_axec3);
    weaponframe(2);
}
pub export fn player_axec3() void {
    state(.axattc3, player_axec4);
    weaponframe(3);
    weapons.W_FireAxe();
}
pub export fn player_axec4() void {
    state(.axattc4, player_run);
    weaponframe(4);
}

pub export fn player_axed1() void {
    state(.axattd1, player_axed2);
    weaponframe(5);
}
pub export fn player_axed2() void {
    state(.axattd2, player_axed3);
    weaponframe(6);
}
pub export fn player_axed3() void {
    state(.axattd3, player_axed4);
    weaponframe(7);
    weapons.W_FireAxe();
}
pub export fn player_axed4() void {
    state(.axattd4, player_run);
    weaponframe(8);
}

//============================================================================

pub export fn player_nail1() void {
    state(.nailatt1, player_nail2);
    const self = q.me();
    muzzleflash();

    if (self.get(.button0) == 0 or client.intermission_running.get() != 0 or self.get(.impulse) != 0) {
        player_run();
        return;
    }
    self.set(.weaponframe, self.get(.weaponframe) + 1);
    if (self.get(.weaponframe) == 9)
        self.set(.weaponframe, 1);
    weapons.SuperDamageSound();
    weapons.W_FireSpikes(4);
    self.set(q.attack_finished, q.time.get() + 0.2);
}
pub export fn player_nail2() void {
    state(.nailatt2, player_nail1);
    const self = q.me();
    muzzleflash();

    if (self.get(.button0) == 0 or client.intermission_running.get() != 0 or self.get(.impulse) != 0) {
        player_run();
        return;
    }
    self.set(.weaponframe, self.get(.weaponframe) + 1);
    if (self.get(.weaponframe) == 9)
        self.set(.weaponframe, 1);
    weapons.SuperDamageSound();
    weapons.W_FireSpikes(-4);
    self.set(q.attack_finished, q.time.get() + 0.2);
}

//============================================================================

pub export fn player_light1() void {
    state(.light1, player_light2);
    const self = q.me();
    muzzleflash();

    if (self.get(.button0) == 0 or client.intermission_running.get() != 0) {
        player_run();
        return;
    }
    self.set(.weaponframe, self.get(.weaponframe) + 1);
    if (self.get(.weaponframe) == 5)
        self.set(.weaponframe, 1);
    weapons.SuperDamageSound();
    weapons.W_FireLightning();
    self.set(q.attack_finished, q.time.get() + 0.2);
}
pub export fn player_light2() void {
    state(.light2, player_light1);
    const self = q.me();
    muzzleflash();

    if (self.get(.button0) == 0 or client.intermission_running.get() != 0) {
        player_run();
        return;
    }
    self.set(.weaponframe, self.get(.weaponframe) + 1);
    if (self.get(.weaponframe) == 5)
        self.set(.weaponframe, 1);
    weapons.SuperDamageSound();
    weapons.W_FireLightning();
    self.set(q.attack_finished, q.time.get() + 0.2);
}

//============================================================================

pub export fn player_rocket1() void {
    state(.rockatt1, player_rocket2);
    weaponframe(1);
    muzzleflash();
}
pub export fn player_rocket2() void {
    state(.rockatt2, player_rocket3);
    weaponframe(2);
}
pub export fn player_rocket3() void {
    state(.rockatt3, player_rocket4);
    weaponframe(3);
}
pub export fn player_rocket4() void {
    state(.rockatt4, player_rocket5);
    weaponframe(4);
}
pub export fn player_rocket5() void {
    state(.rockatt5, player_rocket6);
    weaponframe(5);
}
pub export fn player_rocket6() void {
    state(.rockatt6, player_run);
    weaponframe(6);
}

pub export fn PainSound() void {
    const self = q.me();

    if (self.get(.health) < 0)
        return;

    if (q.damage_attacker.get().get(.classname).eql(S("teledeath"))) {
        q.sound(self, q.CHAN_VOICE, S("player/teledth1.wav"), 1, q.ATTN_NONE);
        return;
    }

    // water pain sounds
    if (self.get(.watertype) == q.CONTENT_WATER and self.get(.waterlevel) == 3) {
        DeathBubbles(1);
        if (q.random() > 0.5) {
            q.sound(self, q.CHAN_VOICE, S("player/drown1.wav"), 1, q.ATTN_NORM);
        } else {
            q.sound(self, q.CHAN_VOICE, S("player/drown2.wav"), 1, q.ATTN_NORM);
        }
        return;
    }

    // slime pain sounds
    if (self.get(.watertype) == q.CONTENT_SLIME) {
        // FIX ME       put in some steam here
        if (q.random() > 0.5) {
            q.sound(self, q.CHAN_VOICE, S("player/lburn1.wav"), 1, q.ATTN_NORM);
        } else {
            q.sound(self, q.CHAN_VOICE, S("player/lburn2.wav"), 1, q.ATTN_NORM);
        }
        return;
    }

    if (self.get(.watertype) == q.CONTENT_LAVA) {
        if (q.random() > 0.5) {
            q.sound(self, q.CHAN_VOICE, S("player/lburn1.wav"), 1, q.ATTN_NORM);
        } else {
            q.sound(self, q.CHAN_VOICE, S("player/lburn2.wav"), 1, q.ATTN_NORM);
        }
        return;
    }

    if (self.get(q.pain_finished) > q.time.get()) {
        self.set(q.axhitme, 0);
        return;
    }
    self.set(q.pain_finished, q.time.get() + 0.5);

    // don't make multiple pain sounds right after each other

    // ax pain sound
    if (self.get(q.axhitme) == 1) {
        self.set(q.axhitme, 0);
        q.sound(self, q.CHAN_VOICE, S("player/axhit1.wav"), 1, q.ATTN_NORM);
        return;
    }

    const rs = q.rint((q.random() * 5) + 1);

    self.set(.noise, S(""));
    if (rs == 1) {
        self.set(.noise, S("player/pain1.wav"));
    } else if (rs == 2) {
        self.set(.noise, S("player/pain2.wav"));
    } else if (rs == 3) {
        self.set(.noise, S("player/pain3.wav"));
    } else if (rs == 4) {
        self.set(.noise, S("player/pain4.wav"));
    } else if (rs == 5) {
        self.set(.noise, S("player/pain5.wav"));
    } else {
        self.set(.noise, S("player/pain6.wav"));
    }

    q.sound(self, q.CHAN_VOICE, self.get(.noise), 1, q.ATTN_NORM);
    return;
}

pub export fn player_pain1() void {
    state(.pain1, player_pain2);
    PainSound();
    weaponframe(0);
}
pub export fn player_pain2() void {
    state(.pain2, player_pain3);
}
pub export fn player_pain3() void {
    state(.pain3, player_pain4);
}
pub export fn player_pain4() void {
    state(.pain4, player_pain5);
}
pub export fn player_pain5() void {
    state(.pain5, player_pain6);
}
pub export fn player_pain6() void {
    state(.pain6, player_run);
}

pub export fn player_axpain1() void {
    state(.axpain1, player_axpain2);
    PainSound();
    weaponframe(0);
}
pub export fn player_axpain2() void {
    state(.axpain2, player_axpain3);
}
pub export fn player_axpain3() void {
    state(.axpain3, player_axpain4);
}
pub export fn player_axpain4() void {
    state(.axpain4, player_axpain5);
}
pub export fn player_axpain5() void {
    state(.axpain5, player_axpain6);
}
pub export fn player_axpain6() void {
    state(.axpain6, player_run);
}

pub export fn player_pain() void {
    const self = q.me();
    if (self.get(.weaponframe) != 0)
        return;

    if (self.get(q.invisible_finished) > q.time.get())
        return; // eyes don't have pain frames

    if (self.get(.weapon) == q.IT_AXE) {
        player_axpain1();
    } else {
        player_pain1();
    }
}

pub export fn DeathBubblesSpawn() void {
    const self = q.me();
    const owner = self.get(.owner);
    if (owner.get(.waterlevel) != 3)
        return;
    const bubble = q.spawn();
    q.setmodel(bubble, S("progs/s_bubble.spr"));
    q.setorigin(bubble, owner.get(.origin) + q.vec(0, 0, 24));
    bubble.set(.movetype, q.MOVETYPE_NOCLIP);
    bubble.set(.solid, q.SOLID_NOT);
    bubble.set(.velocity, q.vec(0, 0, 15));
    bubble.set(.nextthink, q.time.get() + 0.5);
    bubble.set(.think, q.func(misc.bubble_bob));
    bubble.set(.classname, S("bubble"));
    bubble.set(.frame, 0);
    bubble.set(q.cnt, 0);
    q.setsize(bubble, q.vec(-8, -8, -8), q.vec(8, 8, 8));
    self.set(.nextthink, q.time.get() + 0.1);
    self.set(.think, q.func(DeathBubblesSpawn));
    self.set(q.air_finished, self.get(q.air_finished) + 1);
    if (self.get(q.air_finished) >= self.get(q.bubble_count))
        q.remove(self);
}

pub export fn DeathBubbles(num_bubbles: f32) void {
    const self = q.me();

    const bubble_spawner = q.spawn();
    q.setorigin(bubble_spawner, self.get(.origin));
    bubble_spawner.set(.movetype, q.MOVETYPE_NONE);
    bubble_spawner.set(.solid, q.SOLID_NOT);
    bubble_spawner.set(.nextthink, q.time.get() + 0.1);
    bubble_spawner.set(.think, q.func(DeathBubblesSpawn));
    bubble_spawner.set(q.air_finished, 0);
    bubble_spawner.set(.owner, self);
    bubble_spawner.set(q.bubble_count, num_bubbles);
    return;
}

pub export fn DeathSound() void {
    const self = q.me();

    // water death sounds
    if (self.get(.waterlevel) == 3) {
        DeathBubbles(5);
        q.sound(self, q.CHAN_VOICE, S("player/h2odeath.wav"), 1, q.ATTN_NONE);
        return;
    }

    const rs = q.rint((q.random() * 4) + 1);
    if (rs == 1)
        self.set(.noise, S("player/death1.wav"));
    if (rs == 2)
        self.set(.noise, S("player/death2.wav"));
    if (rs == 3)
        self.set(.noise, S("player/death3.wav"));
    if (rs == 4)
        self.set(.noise, S("player/death4.wav"));
    if (rs == 5)
        self.set(.noise, S("player/death5.wav"));

    q.sound(self, q.CHAN_VOICE, self.get(.noise), 1, q.ATTN_NONE);
    return;
}

pub export fn PlayerDead() void {
    const self = q.me();
    self.set(.nextthink, -1);
    // allow respawn after a certain time
    self.set(.deadflag, q.DEAD_DEAD);
}

pub export fn VelocityForDamage(dm: f32) Vector {
    const self = q.me();
    const inflictor = q.damage_inflictor.get();
    var v = q.vec(0, 0, 0);

    if (q.vlen(inflictor.get(.velocity)) > 0) {
        v = q.scale(inflictor.get(.velocity), 0.5);
        v = v + q.scale(q.normalize(self.get(.origin) - inflictor.get(.origin)), 25);
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

pub export fn ThrowGib(gibname: String, dm: f32) void {
    const self = q.me();

    const new = q.spawn();
    new.set(.origin, self.get(.origin));
    q.setmodel(new, gibname);
    q.setsize(new, q.vec(0, 0, 0), q.vec(0, 0, 0));
    new.set(.velocity, VelocityForDamage(dm));
    new.set(.movetype, q.MOVETYPE_BOUNCE);
    new.set(.solid, q.SOLID_NOT);
    new.set(.avelocity, q.vec(q.random() * 600, q.random() * 600, q.random() * 600));
    new.set(.think, q.func(subs.SUB_Remove));
    new.set(.ltime, q.time.get());
    new.set(.nextthink, q.time.get() + 10 + q.random() * 10);
    new.set(.frame, 0);
    new.set(.flags, 0);
}

pub export fn ThrowHead(gibname: String, dm: f32) void {
    const self = q.me();
    q.setmodel(self, gibname);
    self.set(.frame, 0);
    self.set(.nextthink, -1);
    self.set(.movetype, q.MOVETYPE_BOUNCE);
    self.set(.takedamage, q.DAMAGE_NO);
    self.set(.solid, q.SOLID_NOT);
    self.set(.view_ofs, q.vec(0, 0, 8));
    q.setsize(self, q.vec(-16, -16, 0), q.vec(16, 16, 56));
    self.set(.velocity, VelocityForDamage(dm));
    var origin = self.get(.origin);
    origin[2] = origin[2] - 24;
    self.set(.origin, origin);
    self.set(.flags, self.get(.flags) - q.band(self.get(.flags), q.FL_ONGROUND));
    self.set(.avelocity, q.scale(q.vec(0, 600, 0), weapons.crandom()));
}

pub export fn GibPlayer() void {
    const self = q.me();
    ThrowHead(S("progs/h_player.mdl"), self.get(.health));
    ThrowGib(S("progs/gib1.mdl"), self.get(.health));
    ThrowGib(S("progs/gib2.mdl"), self.get(.health));
    ThrowGib(S("progs/gib3.mdl"), self.get(.health));

    self.set(.deadflag, q.DEAD_DEAD);

    if (q.damage_attacker.get().get(.classname).eql(S("teledeath"))) {
        q.sound(self, q.CHAN_VOICE, S("player/teledth1.wav"), 1, q.ATTN_NONE);
        return;
    }

    if (q.damage_attacker.get().get(.classname).eql(S("teledeath2"))) {
        q.sound(self, q.CHAN_VOICE, S("player/teledth1.wav"), 1, q.ATTN_NONE);
        return;
    }

    if (q.random() < 0.5) {
        q.sound(self, q.CHAN_VOICE, S("player/gib.wav"), 1, q.ATTN_NONE);
    } else {
        q.sound(self, q.CHAN_VOICE, S("player/udeath.wav"), 1, q.ATTN_NONE);
    }
}

pub export fn PlayerDie() void {
    const self = q.me();
    self.set(.items, self.get(.items) - q.band(self.get(.items), q.IT_INVISIBILITY));

    if (q.stof(q.infokey(q.world.get(), S("dq"))) != 0) {
        if (self.get(q.super_damage_finished) > 0) {
            items.DropQuad(self.get(q.super_damage_finished) - q.time.get());
            q.bprint(q.PRINT_LOW, self.get(.netname));
            if (q.deathmatch.get() == 4) {
                q.bprint(q.PRINT_LOW, S(" lost an OctaPower with "));
            } else {
                q.bprint(q.PRINT_LOW, S(" lost a quad with "));
            }
            const s = q.ftos(q.rint(self.get(q.super_damage_finished) - q.time.get()));
            q.bprint(q.PRINT_LOW, s);
            q.bprint(q.PRINT_LOW, S(" seconds remaining\n"));
        }
    }

    if (q.stof(q.infokey(q.world.get(), S("dr"))) != 0) {
        if (self.get(q.invisible_finished) > 0) {
            q.bprint(q.PRINT_LOW, self.get(.netname));
            q.bprint(q.PRINT_LOW, S(" lost a ring with "));
            const s = q.ftos(q.rint(self.get(q.invisible_finished) - q.time.get()));
            q.bprint(q.PRINT_LOW, s);
            q.bprint(q.PRINT_LOW, S(" seconds remaining\n"));
            items.DropRing(self.get(q.invisible_finished) - q.time.get());
        }
    }

    self.set(q.invisible_finished, 0); // don't die as eyes
    self.set(q.invincible_finished, 0);
    self.set(q.super_damage_finished, 0);
    self.set(q.radsuit_finished, 0);
    self.set(.modelindex, client.modelindex_player.get()); // don't use eyes

    items.DropBackpack();

    self.set(.weaponmodel, S(""));
    self.set(.view_ofs, q.vec(0, 0, -8));
    self.set(.deadflag, q.DEAD_DYING);
    self.set(.solid, q.SOLID_NOT);
    self.set(.flags, self.get(.flags) - q.band(self.get(.flags), q.FL_ONGROUND));
    self.set(.movetype, q.MOVETYPE_TOSS);
    if (self.get(.velocity)[2] < 10) {
        var v = self.get(.velocity);
        v[2] = v[2] + q.random() * 300;
        self.set(.velocity, v);
    }

    if (self.get(.health) < -40) {
        GibPlayer();
        return;
    }

    DeathSound();

    var angles = self.get(.angles);
    angles[0] = 0;
    angles[2] = 0;
    self.set(.angles, angles);

    if (self.get(.weapon) == q.IT_AXE) {
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

pub export fn set_suicide_frame() void {
    // used by klill command and diconnect command
    const self = q.me();
    if (self.get(.model).neq(S("progs/player.mdl")))
        return; // allready gibbed
    self.set(.frame, fr(.deatha11));
    self.set(.solid, q.SOLID_NOT);
    self.set(.movetype, q.MOVETYPE_TOSS);
    self.set(.deadflag, q.DEAD_DEAD);
    self.set(.nextthink, -1);
}

pub export fn player_diea1() void {
    state(.deatha1, player_diea2);
}
pub export fn player_diea2() void {
    state(.deatha2, player_diea3);
}
pub export fn player_diea3() void {
    state(.deatha3, player_diea4);
}
pub export fn player_diea4() void {
    state(.deatha4, player_diea5);
}
pub export fn player_diea5() void {
    state(.deatha5, player_diea6);
}
pub export fn player_diea6() void {
    state(.deatha6, player_diea7);
}
pub export fn player_diea7() void {
    state(.deatha7, player_diea8);
}
pub export fn player_diea8() void {
    state(.deatha8, player_diea9);
}
pub export fn player_diea9() void {
    state(.deatha9, player_diea10);
}
pub export fn player_diea10() void {
    state(.deatha10, player_diea11);
}
pub export fn player_diea11() void {
    state(.deatha11, player_diea11);
    PlayerDead();
}

pub export fn player_dieb1() void {
    state(.deathb1, player_dieb2);
}
pub export fn player_dieb2() void {
    state(.deathb2, player_dieb3);
}
pub export fn player_dieb3() void {
    state(.deathb3, player_dieb4);
}
pub export fn player_dieb4() void {
    state(.deathb4, player_dieb5);
}
pub export fn player_dieb5() void {
    state(.deathb5, player_dieb6);
}
pub export fn player_dieb6() void {
    state(.deathb6, player_dieb7);
}
pub export fn player_dieb7() void {
    state(.deathb7, player_dieb8);
}
pub export fn player_dieb8() void {
    state(.deathb8, player_dieb9);
}
pub export fn player_dieb9() void {
    state(.deathb9, player_dieb9);
    PlayerDead();
}

pub export fn player_diec1() void {
    state(.deathc1, player_diec2);
}
pub export fn player_diec2() void {
    state(.deathc2, player_diec3);
}
pub export fn player_diec3() void {
    state(.deathc3, player_diec4);
}
pub export fn player_diec4() void {
    state(.deathc4, player_diec5);
}
pub export fn player_diec5() void {
    state(.deathc5, player_diec6);
}
pub export fn player_diec6() void {
    state(.deathc6, player_diec7);
}
pub export fn player_diec7() void {
    state(.deathc7, player_diec8);
}
pub export fn player_diec8() void {
    state(.deathc8, player_diec9);
}
pub export fn player_diec9() void {
    state(.deathc9, player_diec10);
}
pub export fn player_diec10() void {
    state(.deathc10, player_diec11);
}
pub export fn player_diec11() void {
    state(.deathc11, player_diec12);
}
pub export fn player_diec12() void {
    state(.deathc12, player_diec13);
}
pub export fn player_diec13() void {
    state(.deathc13, player_diec14);
}
pub export fn player_diec14() void {
    state(.deathc14, player_diec15);
}
pub export fn player_diec15() void {
    state(.deathc15, player_diec15);
    PlayerDead();
}

pub export fn player_died1() void {
    state(.deathd1, player_died2);
}
pub export fn player_died2() void {
    state(.deathd2, player_died3);
}
pub export fn player_died3() void {
    state(.deathd3, player_died4);
}
pub export fn player_died4() void {
    state(.deathd4, player_died5);
}
pub export fn player_died5() void {
    state(.deathd5, player_died6);
}
pub export fn player_died6() void {
    state(.deathd6, player_died7);
}
pub export fn player_died7() void {
    state(.deathd7, player_died8);
}
pub export fn player_died8() void {
    state(.deathd8, player_died9);
}
pub export fn player_died9() void {
    state(.deathd9, player_died9);
    PlayerDead();
}

pub export fn player_diee1() void {
    state(.deathe1, player_diee2);
}
pub export fn player_diee2() void {
    state(.deathe2, player_diee3);
}
pub export fn player_diee3() void {
    state(.deathe3, player_diee4);
}
pub export fn player_diee4() void {
    state(.deathe4, player_diee5);
}
pub export fn player_diee5() void {
    state(.deathe5, player_diee6);
}
pub export fn player_diee6() void {
    state(.deathe6, player_diee7);
}
pub export fn player_diee7() void {
    state(.deathe7, player_diee8);
}
pub export fn player_diee8() void {
    state(.deathe8, player_diee9);
}
pub export fn player_diee9() void {
    state(.deathe9, player_diee9);
    PlayerDead();
}

pub export fn player_die_ax1() void {
    state(.axdeth1, player_die_ax2);
}
pub export fn player_die_ax2() void {
    state(.axdeth2, player_die_ax3);
}
pub export fn player_die_ax3() void {
    state(.axdeth3, player_die_ax4);
}
pub export fn player_die_ax4() void {
    state(.axdeth4, player_die_ax5);
}
pub export fn player_die_ax5() void {
    state(.axdeth5, player_die_ax6);
}
pub export fn player_die_ax6() void {
    state(.axdeth6, player_die_ax7);
}
pub export fn player_die_ax7() void {
    state(.axdeth7, player_die_ax8);
}
pub export fn player_die_ax8() void {
    state(.axdeth8, player_die_ax9);
}
pub export fn player_die_ax9() void {
    state(.axdeth9, player_die_ax9);
    PlayerDead();
}
