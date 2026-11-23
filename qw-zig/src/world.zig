//! world.qc
//!
//! main/world setup functions
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

const weapons = @import("weapons.zig");

/// `precache_file` each of `files`.
inline fn precacheFiles(comptime files: []const []const u8) void {
    inline for (files) |f| _ = q.precache_file(S(f));
}

/// `precache_file2` each of `files`.
inline fn precacheFiles2(comptime files: []const []const u8) void {
    inline for (files) |f| _ = q.precache_file2(S(f));
}

pub export fn main() void {
    q.dprint(S("main function\n"));

    // these are just commands the the prog compiler to copy these files

    precacheFiles(&.{ "progs.dat", "gfx.wad", "quake.rc", "default.cfg", "end1.bin" });
    precacheFiles2(&.{"end2.bin"});

    precacheFiles(&.{ "demo1.dem", "demo2.dem", "demo3.dem" });

    //
    // these are all of the lumps from the cached.ls files
    //
    precacheFiles(&.{ "gfx/palette.lmp", "gfx/colormap.lmp" });

    precacheFiles2(&.{"gfx/pop.lmp"});

    precacheFiles(&.{
        "gfx/complete.lmp", "gfx/inter.lmp",

        "gfx/ranking.lmp",  "gfx/vidmodes.lmp",
        "gfx/finale.lmp",   "gfx/conback.lmp",
        "gfx/qplaque.lmp",

        "gfx/menudot1.lmp", "gfx/menudot2.lmp",
        "gfx/menudot3.lmp", "gfx/menudot4.lmp",
        "gfx/menudot5.lmp", "gfx/menudot6.lmp",

        "gfx/menuplyr.lmp", "gfx/bigbox.lmp",
        "gfx/dim_modm.lmp", "gfx/dim_drct.lmp",
        "gfx/dim_ipx.lmp",  "gfx/dim_tcp.lmp",
        "gfx/dim_mult.lmp", "gfx/mainmenu.lmp",

        "gfx/box_tl.lmp",   "gfx/box_tm.lmp",
        "gfx/box_tr.lmp",

        "gfx/box_ml.lmp",   "gfx/box_mm.lmp",
        "gfx/box_mm2.lmp",  "gfx/box_mr.lmp",

        "gfx/box_bl.lmp",   "gfx/box_bm.lmp",
        "gfx/box_br.lmp",

        "gfx/sp_menu.lmp",  "gfx/ttl_sgl.lmp",
        "gfx/ttl_main.lmp", "gfx/ttl_cstm.lmp",

        "gfx/mp_menu.lmp",

        "gfx/netmen1.lmp",  "gfx/netmen2.lmp",
        "gfx/netmen3.lmp",  "gfx/netmen4.lmp",
        "gfx/netmen5.lmp",

        "gfx/sell.lmp",

        "gfx/help0.lmp",    "gfx/help1.lmp",
        "gfx/help2.lmp",    "gfx/help3.lmp",
        "gfx/help4.lmp",    "gfx/help5.lmp",

        "gfx/pause.lmp",    "gfx/loading.lmp",

        "gfx/p_option.lmp", "gfx/p_load.lmp",
        "gfx/p_save.lmp",   "gfx/p_multi.lmp",
    });

    // sounds loaded by C code
    _ = q.precache_sound(S("misc/menu1.wav"));
    _ = q.precache_sound(S("misc/menu2.wav"));
    _ = q.precache_sound(S("misc/menu3.wav"));

    _ = q.precache_sound(S("ambience/water1.wav"));
    _ = q.precache_sound(S("ambience/wind2.wav"));

    // shareware
    precacheFiles(&.{"maps/start.bsp"});

    precacheFiles(&.{
        "maps/e1m1.bsp", "maps/e1m2.bsp", "maps/e1m3.bsp", "maps/e1m4.bsp",
        "maps/e1m5.bsp", "maps/e1m6.bsp", "maps/e1m7.bsp", "maps/e1m8.bsp",
    });

    // registered
    precacheFiles2(&.{"gfx/pop.lmp"});

    precacheFiles2(&.{
        "maps/e2m1.bsp", "maps/e2m2.bsp", "maps/e2m3.bsp", "maps/e2m4.bsp",
        "maps/e2m5.bsp", "maps/e2m6.bsp", "maps/e2m7.bsp",

        "maps/e3m1.bsp", "maps/e3m2.bsp", "maps/e3m3.bsp", "maps/e3m4.bsp",
        "maps/e3m5.bsp", "maps/e3m6.bsp", "maps/e3m7.bsp",

        "maps/e4m1.bsp", "maps/e4m2.bsp", "maps/e4m3.bsp", "maps/e4m4.bsp",
        "maps/e4m5.bsp", "maps/e4m6.bsp", "maps/e4m7.bsp", "maps/e4m8.bsp",

        "maps/end.bsp",

        "maps/dm1.bsp",  "maps/dm2.bsp",  "maps/dm3.bsp",  "maps/dm4.bsp",
        "maps/dm5.bsp",  "maps/dm6.bsp",
    });
}

pub const lastspawn = q.userGlobal(Entity, "lastspawn");

//=======================
/// QUAKED worldspawn (0 0 0) ?
/// Only used for the world entity.
/// Set message to the level name.
/// Set sounds to the cd track to play.
///
/// World Types:
/// 0: medieval
/// 1: metal
/// 2: base
//=======================
pub export fn worldspawn() void {
    const self = q.me();

    // `float rj = 1;` in defs.qc (see `defs.rj`).
    q.rj.set(1);

    lastspawn.set(q.world.get());
    InitBodyQue();

    // custom map attributes

    if (self.model.eql(S("maps/e1m8.bsp"))) {
        q.cvar_set(S("sv_gravity"), S("100"));
    } else {
        q.cvar_set(S("sv_gravity"), S("800"));
    }

    // the area based ambient sounds MUST be the first precache_sounds

    // player precaches
    weapons.W_Precache(); // get weapon precaches

    // sounds used from C physics code
    _ = q.precache_sound(S("demon/dland2.wav")); // landing thud
    _ = q.precache_sound(S("misc/h2ohit1.wav")); // landing splash

    // setup precaches allways needed
    _ = q.precache_sound(S("items/itembk2.wav")); // item respawn sound
    _ = q.precache_sound(S("player/plyrjmp8.wav")); // player jump
    _ = q.precache_sound(S("player/land.wav")); // player landing
    _ = q.precache_sound(S("player/land2.wav")); // player hurt landing
    _ = q.precache_sound(S("player/drown1.wav")); // drowning pain
    _ = q.precache_sound(S("player/drown2.wav")); // drowning pain
    _ = q.precache_sound(S("player/gasp1.wav")); // gasping for air
    _ = q.precache_sound(S("player/gasp2.wav")); // taking breath
    _ = q.precache_sound(S("player/h2odeath.wav")); // drowning death

    _ = q.precache_sound(S("misc/talk.wav")); // talk
    _ = q.precache_sound(S("player/teledth1.wav")); // telefrag
    _ = q.precache_sound(S("misc/r_tele1.wav")); // teleport sounds
    _ = q.precache_sound(S("misc/r_tele2.wav"));
    _ = q.precache_sound(S("misc/r_tele3.wav"));
    _ = q.precache_sound(S("misc/r_tele4.wav"));
    _ = q.precache_sound(S("misc/r_tele5.wav"));
    _ = q.precache_sound(S("weapons/lock4.wav")); // ammo pick up
    _ = q.precache_sound(S("weapons/pkup.wav")); // weapon up
    _ = q.precache_sound(S("items/armor1.wav")); // armor up
    _ = q.precache_sound(S("weapons/lhit.wav")); //lightning
    _ = q.precache_sound(S("weapons/lstart.wav")); //lightning start
    _ = q.precache_sound(S("items/damage3.wav"));

    _ = q.precache_sound(S("misc/power.wav")); //lightning for boss

    // player gib sounds
    _ = q.precache_sound(S("player/gib.wav")); // player gib sound
    _ = q.precache_sound(S("player/udeath.wav")); // player gib sound
    _ = q.precache_sound(S("player/tornoff2.wav")); // gib sound

    // player pain sounds

    _ = q.precache_sound(S("player/pain1.wav"));
    _ = q.precache_sound(S("player/pain2.wav"));
    _ = q.precache_sound(S("player/pain3.wav"));
    _ = q.precache_sound(S("player/pain4.wav"));
    _ = q.precache_sound(S("player/pain5.wav"));
    _ = q.precache_sound(S("player/pain6.wav"));

    // player death sounds
    _ = q.precache_sound(S("player/death1.wav"));
    _ = q.precache_sound(S("player/death2.wav"));
    _ = q.precache_sound(S("player/death3.wav"));
    _ = q.precache_sound(S("player/death4.wav"));
    _ = q.precache_sound(S("player/death5.wav"));

    _ = q.precache_sound(S("boss1/sight1.wav"));

    // ax sounds
    _ = q.precache_sound(S("weapons/ax1.wav")); // ax swoosh
    _ = q.precache_sound(S("player/axhit1.wav")); // ax hit meat
    _ = q.precache_sound(S("player/axhit2.wav")); // ax hit world

    _ = q.precache_sound(S("player/h2ojump.wav")); // player jumping into water
    _ = q.precache_sound(S("player/slimbrn2.wav")); // player enter slime
    _ = q.precache_sound(S("player/inh2o.wav")); // player enter water
    _ = q.precache_sound(S("player/inlava.wav")); // player enter lava
    _ = q.precache_sound(S("misc/outwater.wav")); // leaving water sound

    _ = q.precache_sound(S("player/lburn1.wav")); // lava burn
    _ = q.precache_sound(S("player/lburn2.wav")); // lava burn

    _ = q.precache_sound(S("misc/water1.wav")); // swimming
    _ = q.precache_sound(S("misc/water2.wav")); // swimming

    // Invulnerability sounds
    _ = q.precache_sound(S("items/protect.wav"));
    _ = q.precache_sound(S("items/protect2.wav"));
    _ = q.precache_sound(S("items/protect3.wav"));

    _ = q.precache_model(S("progs/player.mdl"));
    _ = q.precache_model(S("progs/eyes.mdl"));
    _ = q.precache_model(S("progs/h_player.mdl"));
    _ = q.precache_model(S("progs/gib1.mdl"));
    _ = q.precache_model(S("progs/gib2.mdl"));
    _ = q.precache_model(S("progs/gib3.mdl"));

    _ = q.precache_model(S("progs/s_bubble.spr")); // drowning bubbles
    _ = q.precache_model(S("progs/s_explod.spr")); // sprite explosion

    _ = q.precache_model(S("progs/v_axe.mdl"));
    _ = q.precache_model(S("progs/v_shot.mdl"));
    _ = q.precache_model(S("progs/v_nail.mdl"));
    _ = q.precache_model(S("progs/v_rock.mdl"));
    _ = q.precache_model(S("progs/v_shot2.mdl"));
    _ = q.precache_model(S("progs/v_nail2.mdl"));
    _ = q.precache_model(S("progs/v_rock2.mdl"));

    _ = q.precache_model(S("progs/bolt.mdl")); // for lightning gun
    _ = q.precache_model(S("progs/bolt2.mdl")); // for lightning gun
    _ = q.precache_model(S("progs/bolt3.mdl")); // for boss shock
    _ = q.precache_model(S("progs/lavaball.mdl")); // for testing

    _ = q.precache_model(S("progs/missile.mdl"));
    _ = q.precache_model(S("progs/grenade.mdl"));
    _ = q.precache_model(S("progs/spike.mdl"));
    _ = q.precache_model(S("progs/s_spike.mdl"));

    _ = q.precache_model(S("progs/backpack.mdl"));

    _ = q.precache_model(S("progs/zom_gib.mdl"));

    _ = q.precache_model(S("progs/v_light.mdl"));

    //
    // Setup light animation tables. 'a' is total darkness, 'z' is maxbright.
    //

    // 0 normal
    q.lightstyle(0, S("m"));

    // 1 FLICKER (first variety)
    q.lightstyle(1, S("mmnmmommommnonmmonqnmmo"));

    // 2 SLOW STRONG PULSE
    q.lightstyle(2, S("abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcba"));

    // 3 CANDLE (first variety)
    q.lightstyle(3, S("mmmmmaaaaammmmmaaaaaabcdefgabcdefg"));

    // 4 FAST STROBE
    q.lightstyle(4, S("mamamamamama"));

    // 5 GENTLE PULSE 1
    q.lightstyle(5, S("jklmnopqrstuvwxyzyxwvutsrqponmlkj"));

    // 6 FLICKER (second variety)
    q.lightstyle(6, S("nmonqnmomnmomomno"));

    // 7 CANDLE (second variety)
    q.lightstyle(7, S("mmmaaaabcdefgmmmmaaaammmaamm"));

    // 8 CANDLE (third variety)
    q.lightstyle(8, S("mmmaaammmaaammmabcdefaaaammmmabcdefmmmaaaa"));

    // 9 SLOW STROBE (fourth variety)
    q.lightstyle(9, S("aaaaaaaazzzzzzzz"));

    // 10 FLUORESCENT FLICKER
    q.lightstyle(10, S("mmamammmmammamamaaamammma"));

    // 11 SLOW PULSE NOT FADE TO BLACK
    q.lightstyle(11, S("abcdefghijklmnopqrrqponmlkjihgfedcba"));

    // styles 32-62 are assigned by the light program for switchable lights

    // 63 testing
    q.lightstyle(63, S("a"));
}

pub export fn StartFrame() void {
    q.timelimit.set(q.cvar(S("timelimit")) * 60);
    q.fraglimit.set(q.cvar(S("fraglimit")));
    q.teamplay.set(q.cvar(S("teamplay")));
    q.deathmatch.set(q.cvar(S("deathmatch")));

    q.framecount.set(q.framecount.get() + 1);
}

// ==============================================================================
//
// BODY QUE
//
// ==============================================================================

pub const bodyque_head = q.userGlobal(Entity, "bodyque_head");

pub export fn bodyque() void {
    // just here so spawn functions don't complain after the world
    // creates bodyques
}

pub fn InitBodyQue() callconv(.c) void {
    const head = q.spawn();
    bodyque_head.set(head);
    head.classname = S("bodyque");
    head.owner = q.spawn();
    head.owner.classname = S("bodyque");
    head.owner.owner = q.spawn();
    head.owner.owner.classname = S("bodyque");
    head.owner.owner.owner = q.spawn();
    head.owner.owner.owner.classname = S("bodyque");
    head.owner.owner.owner.owner = head;
}

/// make a body que entry for the given ent so the ent can be
/// respawned elsewhere
pub fn CopyToBodyQue(ent: Entity) callconv(.c) void {
    const head = bodyque_head.get();
    head.angles = ent.angles;
    head.model = ent.model;
    head.modelindex = ent.modelindex;
    head.frame = ent.frame;
    head.colormap = ent.colormap;
    head.movetype = ent.movetype;
    head.velocity = ent.velocity;
    head.flags = 0;
    q.setorigin(head, ent.origin);
    q.setsize(head, ent.mins, ent.maxs);
    bodyque_head.set(head.owner);
}
