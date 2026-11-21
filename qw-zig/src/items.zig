//! items.qc
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

const subs = @import("subs.zig");
const weapons = @import("weapons.zig");

// ALL LIGHTS SHOULD BE 0 1 0 IN COLOR ALL OTHER ITEMS SHOULD
// BE .8 .3 .4 IN COLOR

pub fn SUB_regen() callconv(.c) void {
    const self = q.me();
    self.set(.model, self.get(q.mdl)); // restore original model
    self.set(.solid, q.SOLID_TRIGGER); // allow it to be touched again
    q.sound(self, q.CHAN_VOICE, S("items/itembk2.wav"), 1, q.ATTN_NORM); // play respawn sound
    q.setorigin(self, self.get(.origin));
}

/// QUAKED noclass (0 0 0) (-8 -8 -8) (8 8 8)
/// prints a warning message when spawned
pub export fn noclass() void {
    const self = q.me();
    q.dprint(S("noclass spawned at"));
    q.dprint(q.vtos(self.get(.origin)));
    q.dprint(S("\n"));
    q.remove(self);
}

pub fn q_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (other.get(.classname).neq(S("player")))
        return;
    if (other.get(.health) <= 0)
        return;

    self.set(q.mdl, self.get(.model));

    q.sound(other, q.CHAN_VOICE, self.get(.noise), 1, q.ATTN_NORM);
    q.stuffcmd(other, S("bf\n"));
    self.set(.solid, q.SOLID_NOT);
    other.set(.items, q.bor(other.get(.items), q.IT_QUAD));
    self.set(.model, q.string_null.get());
    if (q.deathmatch.get() == 4) {
        other.set(.armortype, 0);
        other.set(.armorvalue, 0 * 0.01);
        other.set(.ammo_cells, 0);
    }

    // do the apropriate action
    other.set(q.super_time, 1);
    other.set(q.super_damage_finished, self.get(q.cnt));

    const s = q.ftos(q.rint(other.get(q.super_damage_finished) - q.time.get()));

    q.bprint(q.PRINT_LOW, other.get(.netname));
    if (q.deathmatch.get() == 4) {
        q.bprint(q.PRINT_LOW, S(" recovered an OctaPower with "));
    } else {
        q.bprint(q.PRINT_LOW, S(" recovered a Quad with "));
    }
    q.bprint(q.PRINT_LOW, s);
    q.bprint(q.PRINT_LOW, S(" seconds remaining!\n"));

    q.activator.set(other);
    subs.SUB_UseTargets(); // fire all targets / killtargets
}

pub fn DropQuad(timeleft: f32) callconv(.c) void {
    const self = q.me();
    const item = q.spawn();
    item.set(.origin, self.get(.origin));

    // (The original sets the components in the order z, x, y.)
    const vz: f32 = 300;
    const vx = -100 + (q.random() * 200);
    const vy = -100 + (q.random() * 200);
    item.set(.velocity, q.vec(vx, vy, vz));

    item.set(.flags, q.FL_ITEM);
    item.set(.solid, q.SOLID_TRIGGER);
    item.set(.movetype, q.MOVETYPE_TOSS);
    item.set(.noise, S("items/damage.wav"));
    q.setmodel(item, S("progs/quaddama.mdl"));
    q.setsize(item, q.vec(-16, -16, -24), q.vec(16, 16, 32));
    item.set(q.cnt, q.time.get() + timeleft);
    item.set(.touch, q.func(q_touch));
    item.set(.nextthink, q.time.get() + timeleft); // remove it with the time left on it
    item.set(.think, q.func(subs.SUB_Remove));
}

pub fn r_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (other.get(.classname).neq(S("player")))
        return;
    if (other.get(.health) <= 0)
        return;

    self.set(q.mdl, self.get(.model));

    q.sound(other, q.CHAN_VOICE, self.get(.noise), 1, q.ATTN_NORM);
    q.stuffcmd(other, S("bf\n"));
    self.set(.solid, q.SOLID_NOT);
    other.set(.items, q.bor(other.get(.items), q.IT_INVISIBILITY));
    self.set(.model, q.string_null.get());

    // do the apropriate action
    other.set(q.invisible_time, 1);
    other.set(q.invisible_finished, self.get(q.cnt));
    const s = q.ftos(q.rint(other.get(q.invisible_finished) - q.time.get()));
    q.bprint(q.PRINT_LOW, other.get(.netname));
    q.bprint(q.PRINT_LOW, S(" recovered a Ring with "));
    q.bprint(q.PRINT_LOW, s);
    q.bprint(q.PRINT_LOW, S(" seconds remaining!\n"));

    q.activator.set(other);
    subs.SUB_UseTargets(); // fire all targets / killtargets
}

pub fn DropRing(timeleft: f32) callconv(.c) void {
    const self = q.me();
    const item = q.spawn();
    item.set(.origin, self.get(.origin));

    const vz: f32 = 300;
    const vx = -100 + (q.random() * 200);
    const vy = -100 + (q.random() * 200);
    item.set(.velocity, q.vec(vx, vy, vz));

    item.set(.flags, q.FL_ITEM);
    item.set(.solid, q.SOLID_TRIGGER);
    item.set(.movetype, q.MOVETYPE_TOSS);
    item.set(.noise, S("items/inv1.wav"));
    q.setmodel(item, S("progs/invisibl.mdl"));
    q.setsize(item, q.vec(-16, -16, -24), q.vec(16, 16, 32));
    item.set(q.cnt, q.time.get() + timeleft);
    item.set(.touch, q.func(r_touch));
    item.set(.nextthink, q.time.get() + timeleft); // remove after 30 seconds
    item.set(.think, q.func(subs.SUB_Remove));
}

/// PlaceItem
///
/// plants the object on the floor
pub fn PlaceItem() callconv(.c) void {
    const self = q.me();

    self.set(q.mdl, self.get(.model)); // so it can be restored on respawn
    self.set(.flags, q.FL_ITEM); // make extra wide
    self.set(.solid, q.SOLID_TRIGGER);
    self.set(.movetype, q.MOVETYPE_TOSS);
    self.set(.velocity, q.vec(0, 0, 0));
    var o = self.get(.origin);
    o[2] = o[2] + 6;
    self.set(.origin, o);
    // (`oldz = self.origin_z;` is unused in the original.)
    if (q.droptofloor() == 0) {
        q.dprint(S("Bonus item fell out of level at "));
        q.dprint(q.vtos(self.get(.origin)));
        q.dprint(S("\n"));
        q.remove(self);
        return;
    }
}

/// StartItem
///
/// Sets the clipping size and plants the object on the floor
pub fn StartItem() callconv(.c) void {
    const self = q.me();
    self.set(.nextthink, q.time.get() + 0.2); // items start after other solids
    self.set(.think, q.func(PlaceItem));
}

// =========================================================================
//
// HEALTH BOX
//
// =========================================================================

/// T_Heal: add health to an entity, limiting health to max_health
/// "ignore" will ignore max_health limit
pub fn T_Heal(e: Entity, healamount_: f32, ignore: f32) callconv(.c) f32 {
    const other = q.other.get();
    if (e.get(.health) <= 0)
        return 0;
    if (ignore == 0 and e.get(.health) >= other.get(.max_health))
        return 0;
    const amount = q.ceil(healamount_);

    e.set(.health, e.get(.health) + amount);
    if (ignore == 0 and e.get(.health) >= other.get(.max_health))
        e.set(.health, other.get(.max_health));

    if (e.get(.health) > 250)
        e.set(.health, 250);
    return 1;
}

// QUAKED item_health (.3 .3 1) (0 0 0) (32 32 32) rotten megahealth
// Health box. Normally gives 25 points.
// Rotten box heals 5-10 points,
// megahealth will add 100 health, then
// rot you down to your maximum health limit,
// one point per second.

pub const H_ROTTEN: f32 = 1;
pub const H_MEGA: f32 = 2;
pub const healamount = q.userField(f32, "healamount");
pub const healtype = q.userField(f32, "healtype");

pub export fn item_health() void {
    const self = q.me();
    self.set(.touch, q.func(health_touch));

    if (q.has(self.get(.spawnflags), H_ROTTEN)) {
        _ = q.precache_model(S("maps/b_bh10.bsp"));

        _ = q.precache_sound(S("items/r_item1.wav"));
        q.setmodel(self, S("maps/b_bh10.bsp"));
        self.set(.noise, S("items/r_item1.wav"));
        self.set(healamount, 15);
        self.set(healtype, 0);
    } else if (q.has(self.get(.spawnflags), H_MEGA)) {
        _ = q.precache_model(S("maps/b_bh100.bsp"));
        _ = q.precache_sound(S("items/r_item2.wav"));
        q.setmodel(self, S("maps/b_bh100.bsp"));
        self.set(.noise, S("items/r_item2.wav"));
        self.set(healamount, 100);
        self.set(healtype, 2);
    } else {
        _ = q.precache_model(S("maps/b_bh25.bsp"));
        _ = q.precache_sound(S("items/health1.wav"));
        q.setmodel(self, S("maps/b_bh25.bsp"));
        self.set(.noise, S("items/health1.wav"));
        self.set(healamount, 25);
        self.set(healtype, 1);
    }
    q.setsize(self, q.vec(0, 0, 0), q.vec(32, 32, 56));
    StartItem();
}

pub fn health_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (q.deathmatch.get() == 4)
        if (other.get(q.invincible_time) > 0)
            return;

    if (other.get(.classname).neq(S("player")))
        return;

    if (self.get(healtype) == 2) { // Megahealth?  Ignore max_health...
        if (other.get(.health) >= 250)
            return;
        if (T_Heal(other, self.get(healamount), 1) == 0)
            return;
    } else {
        if (T_Heal(other, self.get(healamount), 0) == 0)
            return;
    }

    q.sprint(other, q.PRINT_LOW, S("You receive "));
    const s = q.ftos(self.get(healamount));
    q.sprint(other, q.PRINT_LOW, s);
    q.sprint(other, q.PRINT_LOW, S(" health\n"));

    // health touch sound
    q.sound(other, q.CHAN_ITEM, self.get(.noise), 1, q.ATTN_NORM);

    q.stuffcmd(other, S("bf\n"));

    self.set(.model, q.string_null.get());
    self.set(.solid, q.SOLID_NOT);

    // Megahealth = rot down the player's super health
    if (self.get(healtype) == 2) {
        other.set(.items, q.bor(other.get(.items), q.IT_SUPERHEALTH));
        if (q.deathmatch.get() != 4) {
            self.set(.nextthink, q.time.get() + 5);
            self.set(.think, q.func(item_megahealth_rot));
        }
        self.set(.owner, other);
    } else {
        if (q.deathmatch.get() != 2) { // deathmatch 2 is the silly old rules
            self.set(.nextthink, q.time.get() + 20);
            self.set(.think, q.func(SUB_regen));
        }
    }

    q.activator.set(other);
    subs.SUB_UseTargets(); // fire all targets / killtargets
}

pub fn item_megahealth_rot() callconv(.c) void {
    const self = q.me();
    q.other.set(self.get(.owner));
    const other = q.other.get();

    if (other.get(.health) > other.get(.max_health)) {
        other.set(.health, other.get(.health) - 1);
        self.set(.nextthink, q.time.get() + 1);
        return;
    }

    // it is possible for a player to die and respawn between rots, so don't
    // just blindly subtract the flag off
    other.set(.items, other.get(.items) - q.band(other.get(.items), q.IT_SUPERHEALTH));

    if (q.deathmatch.get() != 2) { // deathmatch 2 is silly old rules
        self.set(.nextthink, q.time.get() + 20);
        self.set(.think, q.func(SUB_regen));
    }
}

// ===============================================================================
//
// ARMOR
//
// ===============================================================================

pub fn armor_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    var armortype: f32 = 0;
    var value: f32 = 0;
    var bit: f32 = 0;

    if (other.get(.health) <= 0)
        return;
    if (other.get(.classname).neq(S("player")))
        return;

    if (q.deathmatch.get() == 4)
        if (other.get(q.invincible_time) > 0)
            return;

    if (self.get(.classname).eql(S("item_armor1"))) {
        armortype = 0.3;
        value = 100;
        bit = q.IT_ARMOR1;
    }
    if (self.get(.classname).eql(S("item_armor2"))) {
        armortype = 0.6;
        value = 150;
        bit = q.IT_ARMOR2;
    }
    if (self.get(.classname).eql(S("item_armorInv"))) {
        armortype = 0.8;
        value = 200;
        bit = q.IT_ARMOR3;
    }
    if (other.get(.armortype) * other.get(.armorvalue) >= armortype * value)
        return;

    other.set(.armortype, armortype);
    other.set(.armorvalue, value);
    other.set(.items, other.get(.items) - q.band(other.get(.items), q.bor(q.bor(q.IT_ARMOR1, q.IT_ARMOR2), q.IT_ARMOR3)) + bit);

    self.set(.solid, q.SOLID_NOT);
    self.set(.model, q.string_null.get());
    if (q.deathmatch.get() != 2)
        self.set(.nextthink, q.time.get() + 20);
    self.set(.think, q.func(SUB_regen));

    q.sprint(other, q.PRINT_LOW, S("You got armor\n"));
    // armor touch sound
    q.sound(other, q.CHAN_ITEM, S("items/armor1.wav"), 1, q.ATTN_NORM);
    q.stuffcmd(other, S("bf\n"));

    q.activator.set(other);
    subs.SUB_UseTargets(); // fire all targets / killtargets
}

fn armorItem(skin: f32) void {
    const self = q.me();
    self.set(.touch, q.func(armor_touch));
    _ = q.precache_model(S("progs/armor.mdl"));
    q.setmodel(self, S("progs/armor.mdl"));
    self.set(.skin, skin);
    q.setsize(self, q.vec(-16, -16, 0), q.vec(16, 16, 56));
    StartItem();
}

/// QUAKED item_armor1 (0 .5 .8) (-16 -16 0) (16 16 32)
pub export fn item_armor1() void {
    armorItem(0);
}

/// QUAKED item_armor2 (0 .5 .8) (-16 -16 0) (16 16 32)
pub export fn item_armor2() void {
    armorItem(1);
}

/// QUAKED item_armorInv (0 .5 .8) (-16 -16 0) (16 16 32)
pub fn item_armorInv() callconv(.c) void {
    armorItem(2);
}

// ===============================================================================
//
// WEAPONS
//
// ===============================================================================

pub fn bound_other_ammo() callconv(.c) void {
    const other = q.other.get();
    if (other.get(.ammo_shells) > 100)
        other.set(.ammo_shells, 100);
    if (other.get(.ammo_nails) > 200)
        other.set(.ammo_nails, 200);
    if (other.get(.ammo_rockets) > 100)
        other.set(.ammo_rockets, 100);
    if (other.get(.ammo_cells) > 100)
        other.set(.ammo_cells, 100);
}

pub fn RankForWeapon(w: f32) callconv(.c) f32 {
    if (w == q.IT_LIGHTNING)
        return 1;
    if (w == q.IT_ROCKET_LAUNCHER)
        return 2;
    if (w == q.IT_SUPER_NAILGUN)
        return 3;
    if (w == q.IT_GRENADE_LAUNCHER)
        return 4;
    if (w == q.IT_SUPER_SHOTGUN)
        return 5;
    if (w == q.IT_NAILGUN)
        return 6;
    return 7;
}

pub fn WeaponCode(w: f32) callconv(.c) f32 {
    if (w == q.IT_SUPER_SHOTGUN)
        return 3;
    if (w == q.IT_NAILGUN)
        return 4;
    if (w == q.IT_SUPER_NAILGUN)
        return 5;
    if (w == q.IT_GRENADE_LAUNCHER)
        return 6;
    if (w == q.IT_ROCKET_LAUNCHER)
        return 7;
    if (w == q.IT_LIGHTNING)
        return 8;
    return 1;
}

/// Deathmatch_Weapon
///
/// Deathmatch weapon change rules for picking up a weapon
///
/// .float          ammo_shells, ammo_nails, ammo_rockets, ammo_cells;
pub fn Deathmatch_Weapon(old: f32, new: f32) callconv(.c) void {
    _ = old;
    const self = q.me();

    // change self.weapon if desired
    const oldrank = RankForWeapon(self.get(.weapon));
    const newrank = RankForWeapon(new);
    if (newrank < oldrank)
        self.set(.weapon, new);
}

/// weapon_touch
pub fn weapon_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();
    var new: f32 = 0;
    // For client weapon_switch
    var w_switch: f32 = 0;

    if (!q.has(other.get(.flags), q.FL_CLIENT))
        return;

    if (q.stof(q.infokey(other, S("w_switch"))) == 0) {
        w_switch = 8;
    } else {
        w_switch = q.stof(q.infokey(other, S("w_switch")));
    }

    // if the player was using his best weapon, change up to the new one if better
    var stemp = q.self.get();
    q.self.set(other);
    _ = weapons.W_BestWeapon(); // (`best` is unused in the original)
    q.self.set(stemp);

    const leave = q.deathmatch.get() == 2 or q.deathmatch.get() == 3 or q.deathmatch.get() == 5;

    // (`hadammo` is set in each case but never used in the original.)
    if (self.get(.classname).eql(S("weapon_nailgun"))) {
        if (leave and q.has(other.get(.items), q.IT_NAILGUN))
            return;
        new = q.IT_NAILGUN;
        other.set(.ammo_nails, other.get(.ammo_nails) + 30);
    } else if (self.get(.classname).eql(S("weapon_supernailgun"))) {
        if (leave and q.has(other.get(.items), q.IT_SUPER_NAILGUN))
            return;
        new = q.IT_SUPER_NAILGUN;
        other.set(.ammo_nails, other.get(.ammo_nails) + 30);
    } else if (self.get(.classname).eql(S("weapon_supershotgun"))) {
        if (leave and q.has(other.get(.items), q.IT_SUPER_SHOTGUN))
            return;
        new = q.IT_SUPER_SHOTGUN;
        other.set(.ammo_shells, other.get(.ammo_shells) + 5);
    } else if (self.get(.classname).eql(S("weapon_rocketlauncher"))) {
        if (leave and q.has(other.get(.items), q.IT_ROCKET_LAUNCHER))
            return;
        new = q.IT_ROCKET_LAUNCHER;
        other.set(.ammo_rockets, other.get(.ammo_rockets) + 5);
    } else if (self.get(.classname).eql(S("weapon_grenadelauncher"))) {
        if (leave and q.has(other.get(.items), q.IT_GRENADE_LAUNCHER))
            return;
        new = q.IT_GRENADE_LAUNCHER;
        other.set(.ammo_rockets, other.get(.ammo_rockets) + 5);
    } else if (self.get(.classname).eql(S("weapon_lightning"))) {
        if (leave and q.has(other.get(.items), q.IT_LIGHTNING))
            return;
        new = q.IT_LIGHTNING;
        other.set(.ammo_cells, other.get(.ammo_cells) + 15);
    } else {
        q.objerror(S("weapon_touch: unknown classname"));
    }

    q.sprint(other, q.PRINT_LOW, S("You got the "));
    q.sprint(other, q.PRINT_LOW, self.get(.netname));
    q.sprint(other, q.PRINT_LOW, S("\n"));
    // weapon touch sound
    q.sound(other, q.CHAN_ITEM, S("weapons/pkup.wav"), 1, q.ATTN_NORM);
    q.stuffcmd(other, S("bf\n"));

    bound_other_ammo();

    // change to the weapon
    const old = other.get(.items);
    other.set(.items, q.bor(other.get(.items), new));

    stemp = q.self.get();
    q.self.set(other);

    if (WeaponCode(new) <= w_switch) {
        if (q.has(other.get(.flags), q.FL_INWATER)) {
            if (new != q.IT_LIGHTNING) {
                Deathmatch_Weapon(old, new);
            }
        } else {
            Deathmatch_Weapon(old, new);
        }
    }

    weapons.W_SetCurrentAmmo();

    q.self.set(stemp);

    if (leave)
        return;

    // (Always true in the original.)
    if (q.deathmatch.get() != 3 or q.deathmatch.get() != 5) {
        // remove it in single player, or setup for respawning in deathmatch
        self.set(.model, q.string_null.get());
        self.set(.solid, q.SOLID_NOT);
        if (q.deathmatch.get() != 2)
            self.set(.nextthink, q.time.get() + 30);
        self.set(.think, q.func(SUB_regen));
    }
    q.activator.set(other);
    subs.SUB_UseTargets(); // fire all targets / killtargets
}

fn weaponItem(model: q.String, weapon: f32, netname: q.String) void {
    if (q.deathmatch.get() <= 3) {
        const self = q.me();
        _ = q.precache_model(model);
        q.setmodel(self, model);
        self.set(.weapon, weapon);
        self.set(.netname, netname);
        self.set(.touch, q.func(weapon_touch));
        q.setsize(self, q.vec(-16, -16, 0), q.vec(16, 16, 56));
        StartItem();
    }
}

/// QUAKED weapon_supershotgun (0 .5 .8) (-16 -16 0) (16 16 32)
pub export fn weapon_supershotgun() void {
    weaponItem(S("progs/g_shot.mdl"), q.IT_SUPER_SHOTGUN, S("Double-barrelled Shotgun"));
}

/// QUAKED weapon_nailgun (0 .5 .8) (-16 -16 0) (16 16 32)
pub export fn weapon_nailgun() void {
    weaponItem(S("progs/g_nail.mdl"), q.IT_NAILGUN, S("nailgun"));
}

/// QUAKED weapon_supernailgun (0 .5 .8) (-16 -16 0) (16 16 32)
pub export fn weapon_supernailgun() void {
    weaponItem(S("progs/g_nail2.mdl"), q.IT_SUPER_NAILGUN, S("Super Nailgun"));
}

/// QUAKED weapon_grenadelauncher (0 .5 .8) (-16 -16 0) (16 16 32)
pub export fn weapon_grenadelauncher() void {
    weaponItem(S("progs/g_rock.mdl"), 3, S("Grenade Launcher"));
}

/// QUAKED weapon_rocketlauncher (0 .5 .8) (-16 -16 0) (16 16 32)
pub export fn weapon_rocketlauncher() void {
    weaponItem(S("progs/g_rock2.mdl"), 3, S("Rocket Launcher"));
}

/// QUAKED weapon_lightning (0 .5 .8) (-16 -16 0) (16 16 32)
pub export fn weapon_lightning() void {
    weaponItem(S("progs/g_light.mdl"), 3, S("Thunderbolt"));
}

// ===============================================================================
//
// AMMO
//
// ===============================================================================

pub fn ammo_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (other.get(.classname).neq(S("player")))
        return;
    if (other.get(.health) <= 0)
        return;

    // if the player was using his best weapon, change up to the new one if better
    var stemp = q.self.get();
    q.self.set(other);
    const best = weapons.W_BestWeapon();
    q.self.set(stemp);

    // shotgun
    if (self.get(.weapon) == 1) {
        if (other.get(.ammo_shells) >= 100)
            return;
        other.set(.ammo_shells, other.get(.ammo_shells) + self.get(q.aflag));
    }

    // spikes
    if (self.get(.weapon) == 2) {
        if (other.get(.ammo_nails) >= 200)
            return;
        other.set(.ammo_nails, other.get(.ammo_nails) + self.get(q.aflag));
    }

    //      rockets
    if (self.get(.weapon) == 3) {
        if (other.get(.ammo_rockets) >= 100)
            return;
        other.set(.ammo_rockets, other.get(.ammo_rockets) + self.get(q.aflag));
    }

    //      cells
    if (self.get(.weapon) == 4) {
        if (other.get(.ammo_cells) >= 100)
            return;
        other.set(.ammo_cells, other.get(.ammo_cells) + self.get(q.aflag));
    }

    bound_other_ammo();

    q.sprint(other, q.PRINT_LOW, S("You got the "));
    q.sprint(other, q.PRINT_LOW, self.get(.netname));
    q.sprint(other, q.PRINT_LOW, S("\n"));
    // ammo touch sound
    q.sound(other, q.CHAN_ITEM, S("weapons/lock4.wav"), 1, q.ATTN_NORM);
    q.stuffcmd(other, S("bf\n"));

    // change to a better weapon if appropriate

    if (other.get(.weapon) == best) {
        stemp = q.self.get();
        q.self.set(other);
        other.set(.weapon, weapons.W_BestWeapon());
        weapons.W_SetCurrentAmmo();
        q.self.set(stemp);
    }

    // if changed current ammo, update it
    stemp = q.self.get();
    q.self.set(other);
    weapons.W_SetCurrentAmmo();
    q.self.set(stemp);

    // remove it in single player, or setup for respawning in deathmatch
    self.set(.model, q.string_null.get());
    self.set(.solid, q.SOLID_NOT);
    if (q.deathmatch.get() != 2)
        self.set(.nextthink, q.time.get() + 30);

    // Xian -- If playing in DM 3.0 mode, halve the time ammo respawns

    if (q.deathmatch.get() == 3 or q.deathmatch.get() == 5)
        self.set(.nextthink, q.time.get() + 15);

    self.set(.think, q.func(SUB_regen));

    q.activator.set(other);
    subs.SUB_UseTargets(); // fire all targets / killtargets
}

pub const WEAPON_BIG2: f32 = 1;

fn ammoItem(big: bool, big_model: q.String, big_amount: f32, model: q.String, amount: f32, weapon: f32, netname: q.String) void {
    const self = q.me();
    if (q.deathmatch.get() == 4)
        return;

    self.set(.touch, q.func(ammo_touch));

    if (big) {
        _ = q.precache_model(big_model);
        q.setmodel(self, big_model);
        self.set(q.aflag, big_amount);
    } else {
        _ = q.precache_model(model);
        q.setmodel(self, model);
        self.set(q.aflag, amount);
    }
    self.set(.weapon, weapon);
    self.set(.netname, netname);
    q.setsize(self, q.vec(0, 0, 0), q.vec(32, 32, 56));
    StartItem();
}

/// QUAKED item_shells (0 .5 .8) (0 0 0) (32 32 32) big
pub export fn item_shells() void {
    ammoItem(q.has(q.me().get(.spawnflags), WEAPON_BIG2), S("maps/b_shell1.bsp"), 40, S("maps/b_shell0.bsp"), 20, 1, S("shells"));
}

/// QUAKED item_spikes (0 .5 .8) (0 0 0) (32 32 32) big
pub export fn item_spikes() void {
    ammoItem(q.has(q.me().get(.spawnflags), WEAPON_BIG2), S("maps/b_nail1.bsp"), 50, S("maps/b_nail0.bsp"), 25, 2, S("nails"));
}

/// QUAKED item_rockets (0 .5 .8) (0 0 0) (32 32 32) big
pub export fn item_rockets() void {
    ammoItem(q.has(q.me().get(.spawnflags), WEAPON_BIG2), S("maps/b_rock1.bsp"), 10, S("maps/b_rock0.bsp"), 5, 3, S("rockets"));
}

/// QUAKED item_cells (0 .5 .8) (0 0 0) (32 32 32) big
pub export fn item_cells() void {
    ammoItem(q.has(q.me().get(.spawnflags), WEAPON_BIG2), S("maps/b_batt1.bsp"), 12, S("maps/b_batt0.bsp"), 6, 4, S("cells"));
}

// QUAKED item_weapon (0 .5 .8) (0 0 0) (32 32 32) shotgun rocket spikes big
// DO NOT USE THIS!!!! IT WILL BE REMOVED!

pub const WEAPON_SHOTGUN: f32 = 1;
pub const WEAPON_ROCKET: f32 = 2;
pub const WEAPON_SPIKES: f32 = 4;
pub const WEAPON_BIG: f32 = 8;

pub export fn item_weapon() void {
    const self = q.me();
    self.set(.touch, q.func(ammo_touch));

    if (q.has(self.get(.spawnflags), WEAPON_SHOTGUN)) {
        if (q.has(self.get(.spawnflags), WEAPON_BIG)) {
            _ = q.precache_model(S("maps/b_shell1.bsp"));
            q.setmodel(self, S("maps/b_shell1.bsp"));
            self.set(q.aflag, 40);
        } else {
            _ = q.precache_model(S("maps/b_shell0.bsp"));
            q.setmodel(self, S("maps/b_shell0.bsp"));
            self.set(q.aflag, 20);
        }
        self.set(.weapon, 1);
        self.set(.netname, S("shells"));
    }

    if (q.has(self.get(.spawnflags), WEAPON_SPIKES)) {
        if (q.has(self.get(.spawnflags), WEAPON_BIG)) {
            _ = q.precache_model(S("maps/b_nail1.bsp"));
            q.setmodel(self, S("maps/b_nail1.bsp"));
            self.set(q.aflag, 40);
        } else {
            _ = q.precache_model(S("maps/b_nail0.bsp"));
            q.setmodel(self, S("maps/b_nail0.bsp"));
            self.set(q.aflag, 20);
        }
        self.set(.weapon, 2);
        self.set(.netname, S("spikes"));
    }

    if (q.has(self.get(.spawnflags), WEAPON_ROCKET)) {
        if (q.has(self.get(.spawnflags), WEAPON_BIG)) {
            _ = q.precache_model(S("maps/b_rock1.bsp"));
            q.setmodel(self, S("maps/b_rock1.bsp"));
            self.set(q.aflag, 10);
        } else {
            _ = q.precache_model(S("maps/b_rock0.bsp"));
            q.setmodel(self, S("maps/b_rock0.bsp"));
            self.set(q.aflag, 5);
        }
        self.set(.weapon, 3);
        self.set(.netname, S("rockets"));
    }

    q.setsize(self, q.vec(0, 0, 0), q.vec(32, 32, 56));
    StartItem();
}

// ===============================================================================
//
// KEYS
//
// ===============================================================================

pub fn key_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (other.get(.classname).neq(S("player")))
        return;
    if (other.get(.health) <= 0)
        return;
    if (q.has(other.get(.items), self.get(.items)))
        return;

    q.sprint(other, q.PRINT_LOW, S("You got the "));
    q.sprint(other, q.PRINT_LOW, self.get(.netname));
    q.sprint(other, q.PRINT_LOW, S("\n"));

    q.sound(other, q.CHAN_ITEM, self.get(.noise), 1, q.ATTN_NORM);
    q.stuffcmd(other, S("bf\n"));
    other.set(.items, q.bor(other.get(.items), self.get(.items)));

    self.set(.solid, q.SOLID_NOT);
    self.set(.model, q.string_null.get());

    q.activator.set(other);
    subs.SUB_UseTargets(); // fire all targets / killtargets
}

pub fn key_setsounds() callconv(.c) void {
    const self = q.me();
    const worldtype = q.world.get().get(q.worldtype);
    if (worldtype == 0) {
        _ = q.precache_sound(S("misc/medkey.wav"));
        self.set(.noise, S("misc/medkey.wav"));
    }
    if (worldtype == 1) {
        _ = q.precache_sound(S("misc/runekey.wav"));
        self.set(.noise, S("misc/runekey.wav"));
    }
    if (worldtype == 2) {
        _ = q.precache_sound2(S("misc/basekey.wav"));
        self.set(.noise, S("misc/basekey.wav"));
    }
}

/// QUAKED item_key1 (0 .5 .8) (-16 -16 -24) (16 16 32)
/// SILVER key
/// In order for keys to work
/// you MUST set your maps
/// worldtype to one of the
/// following:
/// 0: medieval
/// 1: metal
/// 2: base
pub export fn item_key1() void {
    const self = q.me();
    const worldtype = q.world.get().get(q.worldtype);
    if (worldtype == 0) {
        _ = q.precache_model(S("progs/w_s_key.mdl"));
        q.setmodel(self, S("progs/w_s_key.mdl"));
        self.set(.netname, S("silver key"));
    } else if (worldtype == 1) {
        _ = q.precache_model(S("progs/m_s_key.mdl"));
        q.setmodel(self, S("progs/m_s_key.mdl"));
        self.set(.netname, S("silver runekey"));
    } else if (worldtype == 2) {
        _ = q.precache_model2(S("progs/b_s_key.mdl"));
        q.setmodel(self, S("progs/b_s_key.mdl"));
        self.set(.netname, S("silver keycard"));
    }
    key_setsounds();
    self.set(.touch, q.func(key_touch));
    self.set(.items, q.IT_KEY1);
    q.setsize(self, q.vec(-16, -16, -24), q.vec(16, 16, 32));
    StartItem();
}

/// QUAKED item_key2 (0 .5 .8) (-16 -16 -24) (16 16 32)
/// GOLD key
/// In order for keys to work
/// you MUST set your maps
/// worldtype to one of the
/// following:
/// 0: medieval
/// 1: metal
/// 2: base
pub export fn item_key2() void {
    const self = q.me();
    const worldtype = q.world.get().get(q.worldtype);
    if (worldtype == 0) {
        _ = q.precache_model(S("progs/w_g_key.mdl"));
        q.setmodel(self, S("progs/w_g_key.mdl"));
        self.set(.netname, S("gold key"));
    }
    if (worldtype == 1) {
        _ = q.precache_model(S("progs/m_g_key.mdl"));
        q.setmodel(self, S("progs/m_g_key.mdl"));
        self.set(.netname, S("gold runekey"));
    }
    if (worldtype == 2) {
        _ = q.precache_model2(S("progs/b_g_key.mdl"));
        q.setmodel(self, S("progs/b_g_key.mdl"));
        self.set(.netname, S("gold keycard"));
    }
    key_setsounds();
    self.set(.touch, q.func(key_touch));
    self.set(.items, q.IT_KEY2);
    q.setsize(self, q.vec(-16, -16, -24), q.vec(16, 16, 32));
    StartItem();
}

// ===============================================================================
//
// END OF LEVEL RUNES
//
// ===============================================================================

pub fn sigil_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (other.get(.classname).neq(S("player")))
        return;
    if (other.get(.health) <= 0)
        return;

    q.centerprint(other, S("You got the rune!"));

    q.sound(other, q.CHAN_ITEM, self.get(.noise), 1, q.ATTN_NORM);
    q.stuffcmd(other, S("bf\n"));
    self.set(.solid, q.SOLID_NOT);
    self.set(.model, q.string_null.get());
    q.serverflags.set(q.bor(q.serverflags.get(), q.band(self.get(.spawnflags), 15)));
    self.set(.classname, S("")); // so rune doors won't find it

    q.activator.set(other);
    subs.SUB_UseTargets(); // fire all targets / killtargets
}

/// QUAKED item_sigil (0 .5 .8) (-16 -16 -24) (16 16 32) E1 E2 E3 E4
/// End of level sigil, pick up to end episode and return to jrstart.
pub export fn item_sigil() void {
    const self = q.me();
    if (self.get(.spawnflags) == 0)
        q.objerror(S("no spawnflags"));

    _ = q.precache_sound(S("misc/runekey.wav"));
    self.set(.noise, S("misc/runekey.wav"));

    if (q.has(self.get(.spawnflags), 1)) {
        _ = q.precache_model(S("progs/end1.mdl"));
        q.setmodel(self, S("progs/end1.mdl"));
    }
    if (q.has(self.get(.spawnflags), 2)) {
        _ = q.precache_model2(S("progs/end2.mdl"));
        q.setmodel(self, S("progs/end2.mdl"));
    }
    if (q.has(self.get(.spawnflags), 4)) {
        _ = q.precache_model2(S("progs/end3.mdl"));
        q.setmodel(self, S("progs/end3.mdl"));
    }
    if (q.has(self.get(.spawnflags), 8)) {
        _ = q.precache_model2(S("progs/end4.mdl"));
        q.setmodel(self, S("progs/end4.mdl"));
    }

    self.set(.touch, q.func(sigil_touch));
    q.setsize(self, q.vec(-16, -16, -24), q.vec(16, 16, 32));
    StartItem();
}

// ===============================================================================
//
// POWERUPS
//
// ===============================================================================

pub fn powerup_touch() callconv(.c) void {
    const self = q.me();
    const other = q.other.get();

    if (other.get(.classname).neq(S("player")))
        return;
    if (other.get(.health) <= 0)
        return;

    q.sprint(other, q.PRINT_LOW, S("You got the "));
    q.sprint(other, q.PRINT_LOW, self.get(.netname));
    q.sprint(other, q.PRINT_LOW, S("\n"));

    self.set(q.mdl, self.get(.model));

    if (self.get(.classname).eql(S("item_artifact_invulnerability")) or
        self.get(.classname).eql(S("item_artifact_invisibility")))
    {
        self.set(.nextthink, q.time.get() + 60 * 5);
    } else {
        self.set(.nextthink, q.time.get() + 60);
    }

    self.set(.think, q.func(SUB_regen));

    q.sound(other, q.CHAN_VOICE, self.get(.noise), 1, q.ATTN_NORM);
    q.stuffcmd(other, S("bf\n"));
    self.set(.solid, q.SOLID_NOT);
    other.set(.items, q.bor(other.get(.items), self.get(.items)));
    self.set(.model, q.string_null.get());

    // do the apropriate action
    if (self.get(.classname).eql(S("item_artifact_envirosuit"))) {
        other.set(q.rad_time, 1);
        other.set(q.radsuit_finished, q.time.get() + 30);
    }

    if (self.get(.classname).eql(S("item_artifact_invulnerability"))) {
        other.set(q.invincible_time, 1);
        other.set(q.invincible_finished, q.time.get() + 30);
    }

    if (self.get(.classname).eql(S("item_artifact_invisibility"))) {
        other.set(q.invisible_time, 1);
        other.set(q.invisible_finished, q.time.get() + 30);
    }

    if (self.get(.classname).eql(S("item_artifact_super_damage"))) {
        if (q.deathmatch.get() == 4) {
            other.set(.armortype, 0);
            other.set(.armorvalue, 0 * 0.01);
            other.set(.ammo_cells, 0);
        }
        other.set(q.super_time, 1);
        other.set(q.super_damage_finished, q.time.get() + 30);
    }

    q.activator.set(other);
    subs.SUB_UseTargets(); // fire all targets / killtargets
}

/// QUAKED item_artifact_invulnerability (0 .5 .8) (-16 -16 -24) (16 16 32)
/// Player is invulnerable for 30 seconds
pub export fn item_artifact_invulnerability() void {
    const self = q.me();
    self.set(.touch, q.func(powerup_touch));

    _ = q.precache_model(S("progs/invulner.mdl"));
    _ = q.precache_sound(S("items/protect.wav"));
    _ = q.precache_sound(S("items/protect2.wav"));
    _ = q.precache_sound(S("items/protect3.wav"));
    self.set(.noise, S("items/protect.wav"));
    q.setmodel(self, S("progs/invulner.mdl"));
    self.set(.netname, S("Pentagram of Protection"));
    self.set(.effects, q.bor(self.get(.effects), q.EF_RED));
    self.set(.items, q.IT_INVULNERABILITY);
    q.setsize(self, q.vec(-16, -16, -24), q.vec(16, 16, 32));
    StartItem();
}

/// QUAKED item_artifact_envirosuit (0 .5 .8) (-16 -16 -24) (16 16 32)
/// Player takes no damage from water or slime for 30 seconds
pub export fn item_artifact_envirosuit() void {
    const self = q.me();
    self.set(.touch, q.func(powerup_touch));

    _ = q.precache_model(S("progs/suit.mdl"));
    _ = q.precache_sound(S("items/suit.wav"));
    _ = q.precache_sound(S("items/suit2.wav"));
    self.set(.noise, S("items/suit.wav"));
    q.setmodel(self, S("progs/suit.mdl"));
    self.set(.netname, S("Biosuit"));
    self.set(.items, q.IT_SUIT);
    q.setsize(self, q.vec(-16, -16, -24), q.vec(16, 16, 32));
    StartItem();
}

/// QUAKED item_artifact_invisibility (0 .5 .8) (-16 -16 -24) (16 16 32)
/// Player is invisible for 30 seconds
pub export fn item_artifact_invisibility() void {
    const self = q.me();
    self.set(.touch, q.func(powerup_touch));

    _ = q.precache_model(S("progs/invisibl.mdl"));
    _ = q.precache_sound(S("items/inv1.wav"));
    _ = q.precache_sound(S("items/inv2.wav"));
    _ = q.precache_sound(S("items/inv3.wav"));
    self.set(.noise, S("items/inv1.wav"));
    q.setmodel(self, S("progs/invisibl.mdl"));
    self.set(.netname, S("Ring of Shadows"));
    self.set(.items, q.IT_INVISIBILITY);
    q.setsize(self, q.vec(-16, -16, -24), q.vec(16, 16, 32));
    StartItem();
}

/// QUAKED item_artifact_super_damage (0 .5 .8) (-16 -16 -24) (16 16 32)
/// The next attack from the player will do 4x damage
pub export fn item_artifact_super_damage() void {
    const self = q.me();
    self.set(.touch, q.func(powerup_touch));

    _ = q.precache_model(S("progs/quaddama.mdl"));
    _ = q.precache_sound(S("items/damage.wav"));
    _ = q.precache_sound(S("items/damage2.wav"));
    _ = q.precache_sound(S("items/damage3.wav"));
    self.set(.noise, S("items/damage.wav"));
    q.setmodel(self, S("progs/quaddama.mdl"));
    if (q.deathmatch.get() == 4) {
        self.set(.netname, S("OctaPower"));
    } else {
        self.set(.netname, S("Quad Damage"));
    }
    self.set(.items, q.IT_QUAD);
    self.set(.effects, q.bor(self.get(.effects), q.EF_BLUE));
    q.setsize(self, q.vec(-16, -16, -24), q.vec(16, 16, 32));
    StartItem();
}

// ===============================================================================
//
// PLAYER BACKPACKS
//
// ===============================================================================

pub fn BackpackTouch() callconv(.c) void {
    var self = q.me();
    const other = q.other.get();
    var b_switch: f32 = 0;

    if (q.deathmatch.get() == 4)
        if (other.get(q.invincible_time) > 0)
            return;

    if (q.stof(q.infokey(other, S("b_switch"))) == 0) {
        b_switch = 8;
    } else {
        b_switch = q.stof(q.infokey(other, S("b_switch")));
    }

    if (other.get(.classname).neq(S("player")))
        return;
    if (other.get(.health) <= 0)
        return;

    var acount: f32 = 0;

    q.sprint(other, q.PRINT_LOW, S("You get "));

    if (q.deathmatch.get() == 4) {
        other.set(.health, other.get(.health) + 10);
        q.sprint(other, q.PRINT_LOW, S("10 additional health\n"));
        if (other.get(.health) > 250 and other.get(.health) < 300) {
            q.sound(other, q.CHAN_ITEM, S("items/protect3.wav"), 1, q.ATTN_NORM);
        } else {
            q.sound(other, q.CHAN_ITEM, S("weapons/lock4.wav"), 1, q.ATTN_NORM);
        }
        q.stuffcmd(other, S("bf\n"));
        q.remove(self);

        if (other.get(.health) > 299) {
            if (other.get(q.invincible_time) != 1) {
                other.set(q.invincible_time, 1);
                other.set(q.invincible_finished, q.time.get() + 30);
                other.set(.items, q.bor(other.get(.items), q.IT_INVULNERABILITY));

                other.set(q.super_time, 1);
                other.set(q.super_damage_finished, q.time.get() + 30);
                other.set(.items, q.bor(other.get(.items), q.IT_QUAD));

                other.set(.ammo_cells, 0);

                q.sound(other, q.CHAN_VOICE, S("boss1/sight1.wav"), 1, q.ATTN_NORM);
                q.stuffcmd(other, S("bf\n"));
                q.bprint(q.PRINT_HIGH, other.get(.netname));
                q.bprint(q.PRINT_HIGH, S(" attains bonus powers!!!\n"));
            }
        }
        q.self.set(other);
        return;
    }
    if (self.get(.items) != 0) {
        if (q.band(other.get(.items), self.get(.items)) == 0) {
            acount = 1;
            q.sprint(other, q.PRINT_LOW, S("the "));
            q.sprint(other, q.PRINT_LOW, self.get(.netname));
        }
    }

    // if the player was using his best weapon, change up to the new one if better
    const stemp = q.self.get();
    q.self.set(other);
    _ = weapons.W_BestWeapon(); // (`best` is unused in the original)
    q.self.set(stemp);

    // change weapons
    other.set(.ammo_shells, other.get(.ammo_shells) + self.get(.ammo_shells));
    other.set(.ammo_nails, other.get(.ammo_nails) + self.get(.ammo_nails));
    other.set(.ammo_rockets, other.get(.ammo_rockets) + self.get(.ammo_rockets));
    other.set(.ammo_cells, other.get(.ammo_cells) + self.get(.ammo_cells));

    var new = self.get(.items);
    if (new == 0)
        new = other.get(.weapon);
    const old = other.get(.items);
    other.set(.items, q.bor(other.get(.items), self.get(.items)));

    bound_other_ammo();

    if (self.get(.ammo_shells) != 0) {
        if (acount != 0)
            q.sprint(other, q.PRINT_LOW, S(", "));
        acount = 1;
        const s = q.ftos(self.get(.ammo_shells));
        q.sprint(other, q.PRINT_LOW, s);
        q.sprint(other, q.PRINT_LOW, S(" shells"));
    }
    if (self.get(.ammo_nails) != 0) {
        if (acount != 0)
            q.sprint(other, q.PRINT_LOW, S(", "));
        acount = 1;
        const s = q.ftos(self.get(.ammo_nails));
        q.sprint(other, q.PRINT_LOW, s);
        q.sprint(other, q.PRINT_LOW, S(" nails"));
    }
    if (self.get(.ammo_rockets) != 0) {
        if (acount != 0)
            q.sprint(other, q.PRINT_LOW, S(", "));
        acount = 1;
        const s = q.ftos(self.get(.ammo_rockets));
        q.sprint(other, q.PRINT_LOW, s);
        q.sprint(other, q.PRINT_LOW, S(" rockets"));
    }
    if (self.get(.ammo_cells) != 0) {
        if (acount != 0)
            q.sprint(other, q.PRINT_LOW, S(", "));
        acount = 1;
        const s = q.ftos(self.get(.ammo_cells));
        q.sprint(other, q.PRINT_LOW, s);
        q.sprint(other, q.PRINT_LOW, S(" cells"));
    }

    if ((q.deathmatch.get() == 3 or q.deathmatch.get() == 5) and (WeaponCode(new) == 6 or WeaponCode(new) == 7) and other.get(.ammo_rockets) < 5)
        other.set(.ammo_rockets, 5);

    q.sprint(other, q.PRINT_LOW, S("\n"));
    // backpack touch sound
    q.sound(other, q.CHAN_ITEM, S("weapons/lock4.wav"), 1, q.ATTN_NORM);
    q.stuffcmd(other, S("bf\n"));

    q.remove(self);
    q.self.set(other);
    self = other;

    // change to the weapon

    if (WeaponCode(new) <= b_switch) {
        if (q.has(self.get(.flags), q.FL_INWATER)) {
            if (new != q.IT_LIGHTNING) {
                Deathmatch_Weapon(old, new);
            }
        } else {
            Deathmatch_Weapon(old, new);
        }
    }

    weapons.W_SetCurrentAmmo();
}

/// DropBackpack
pub fn DropBackpack() callconv(.c) void {
    const self = q.me();

    if (self.get(.ammo_shells) + self.get(.ammo_nails) + self.get(.ammo_rockets) + self.get(.ammo_cells) == 0)
        return; // nothing in it

    const item = q.spawn();
    item.set(.origin, self.get(.origin) - q.vec(0, 0, 24));

    item.set(.items, self.get(.weapon));
    const items = item.get(.items);
    if (items == q.IT_AXE) {
        item.set(.netname, S("Axe"));
    } else if (items == q.IT_SHOTGUN) {
        item.set(.netname, S("Shotgun"));
    } else if (items == q.IT_SUPER_SHOTGUN) {
        item.set(.netname, S("Double-barrelled Shotgun"));
    } else if (items == q.IT_NAILGUN) {
        item.set(.netname, S("Nailgun"));
    } else if (items == q.IT_SUPER_NAILGUN) {
        item.set(.netname, S("Super Nailgun"));
    } else if (items == q.IT_GRENADE_LAUNCHER) {
        item.set(.netname, S("Grenade Launcher"));
    } else if (items == q.IT_ROCKET_LAUNCHER) {
        item.set(.netname, S("Rocket Launcher"));
    } else if (items == q.IT_LIGHTNING) {
        item.set(.netname, S("Thunderbolt"));
    } else {
        item.set(.netname, S(""));
    }

    item.set(.ammo_shells, self.get(.ammo_shells));
    item.set(.ammo_nails, self.get(.ammo_nails));
    item.set(.ammo_rockets, self.get(.ammo_rockets));
    item.set(.ammo_cells, self.get(.ammo_cells));

    const vz: f32 = 300;
    const vx = -100 + (q.random() * 200);
    const vy = -100 + (q.random() * 200);
    item.set(.velocity, q.vec(vx, vy, vz));

    item.set(.flags, q.FL_ITEM);
    item.set(.solid, q.SOLID_TRIGGER);
    item.set(.movetype, q.MOVETYPE_TOSS);
    q.setmodel(item, S("progs/backpack.mdl"));
    q.setsize(item, q.vec(-16, -16, 0), q.vec(16, 16, 56));
    item.set(.touch, q.func(BackpackTouch));

    item.set(.nextthink, q.time.get() + 120); // remove after 2 minutes
    item.set(.think, q.func(subs.SUB_Remove));
}
