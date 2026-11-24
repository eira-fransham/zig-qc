//! The globals and entity fields from `qw-qc/defs.qc` that aren't part of the
//! engine's `globalvars_t`/`entvars_t` (those, and the game constants, are in
//! `defs.zig`).

const q = @import("defs.zig");
const Entity = q.Entity;
const Function = q.Function;
const String = q.String;
const Vector = q.Vector;
const userField = q.userField;
const userGlobal = q.userGlobal;

//================================================

//
// globals
//
pub const movedist = userGlobal(f32, "movedist");

pub const string_null = userGlobal(String, "string_null"); // null string, nothing should be held here
pub const empty_float = userGlobal(f32, "empty_float");

pub const activator = userGlobal(Entity, "activator"); // the entity that activated a trigger or brush

pub const damage_attacker = userGlobal(Entity, "damage_attacker"); // set by T_Damage
pub const damage_inflictor = userGlobal(Entity, "damage_inflictor");
pub const framecount = userGlobal(f32, "framecount");

//
// cvars checked each frame
//
pub const teamplay = userGlobal(f32, "teamplay");
pub const timelimit = userGlobal(f32, "timelimit");
pub const fraglimit = userGlobal(f32, "fraglimit");
pub const deathmatch = userGlobal(f32, "deathmatch");
// `float rj = 1;` in the original. Zig globals can't have initial values in
// QuakeC, so `worldspawn` sets it (progs globals are reset on every map load,
// just before `worldspawn` runs).
pub const rj = userGlobal(f32, "rj");

//================================================

//
// world fields (FIXME: make globals)
//
pub const wad = userField(String, "wad");
pub const map = userField(String, "map");
pub const worldtype = userField(f32, "worldtype"); // 0=medieval 1=metal 2=base

//================================================

pub const killtarget = userField(String, "killtarget");

//
// quakeed fields
//
pub const light_lev = userField(f32, "light_lev"); // not used by game, but parsed by light util
pub const style = userField(f32, "style");

//
// monster ai
//
pub const th_stand = userField(?Function, "th_stand");
pub const th_walk = userField(?Function, "th_walk");
pub const th_run = userField(?Function, "th_run");
pub const th_missile = userField(?Function, "th_missile");
pub const th_melee = userField(?Function, "th_melee");
/// `.void(entity attacker, float damage) th_pain`; call with `callAs(ThPain, ...)`.
pub const th_pain = userField(?Function, "th_pain");
pub const ThPain = fn (Entity, f32) callconv(.c) void;
pub const th_die = userField(?Function, "th_die");

pub const oldenemy = userField(Entity, "oldenemy"); // mad at this player before taking damage

pub const speed = userField(f32, "speed");

pub const lefty = userField(f32, "lefty");

pub const search_time = userField(f32, "search_time");
pub const attack_state = userField(f32, "attack_state");

//
// player only fields
//
pub const voided = userField(f32, "voided");
pub const walkframe = userField(f32, "walkframe");

// Zoid Additions
pub const maxspeed = userField(f32, "maxspeed"); // Used to set Maxspeed on a player
pub const gravity = userField(f32, "gravity"); // Gravity Multiplier (0 to 1.0)

pub const attack_finished = userField(f32, "attack_finished");
pub const pain_finished = userField(f32, "pain_finished");

pub const invincible_finished = userField(f32, "invincible_finished");
pub const invisible_finished = userField(f32, "invisible_finished");
pub const super_damage_finished = userField(f32, "super_damage_finished");
pub const radsuit_finished = userField(f32, "radsuit_finished");

pub const invincible_time = userField(f32, "invincible_time");
pub const invincible_sound = userField(f32, "invincible_sound");
pub const invisible_time = userField(f32, "invisible_time");
pub const invisible_sound = userField(f32, "invisible_sound");
pub const super_time = userField(f32, "super_time");
pub const super_sound = userField(f32, "super_sound");
pub const rad_time = userField(f32, "rad_time");
pub const fly_sound = userField(f32, "fly_sound");

pub const axhitme = userField(f32, "axhitme");

pub const show_hostile = userField(f32, "show_hostile"); // set to time+0.2 whenever a client fires a
// weapon or takes damage.  Used to alert
// monsters that otherwise would let the player go
pub const jump_flag = userField(f32, "jump_flag"); // player jump flag
pub const swim_flag = userField(f32, "swim_flag"); // player swimming sound flag
pub const air_finished = userField(f32, "air_finished"); // when time > air_finished, start drowning
pub const bubble_count = userField(f32, "bubble_count"); // keeps track of the number of bubbles
pub const deathtype = userField(String, "deathtype"); // keeps track of how the player died

//
// object stuff
//
pub const mdl = userField(String, "mdl");
pub const mangle = userField(Vector, "mangle"); // angle at start

// `.vector oldorigin` is redeclared here in the original ("only used by
// secret door"); it is the system field `.oldorigin`.

pub const t_length = userField(f32, "t_length");
pub const t_width = userField(f32, "t_width");

//
// doors, etc
//
pub const dest = userField(Vector, "dest");
pub const dest1 = userField(Vector, "dest1");
pub const dest2 = userField(Vector, "dest2");
pub const wait = userField(f32, "wait"); // time from firing to restarting
pub const delay = userField(f32, "delay"); // time from activation to firing
pub const trigger_field = userField(Entity, "trigger_field"); // door's trigger entity
pub const noise4 = userField(String, "noise4");

//
// monsters
//
pub const pausetime = userField(f32, "pausetime");
pub const movetarget = userField(Entity, "movetarget");

//
// doors
//
pub const aflag = userField(f32, "aflag");
pub const dmg = userField(f32, "dmg"); // damage done by door when hit

//
// misc
//
pub const cnt = userField(f32, "cnt"); // misc flag

//
// subs
//
pub const think1 = userField(?Function, "think1");
pub const finaldest = userField(Vector, "finaldest");
pub const finalangle = userField(Vector, "finalangle");

//
// triggers
//
pub const count = userField(f32, "count"); // for counting triggers

//
// plats / doors / buttons
//
pub const lip = userField(f32, "lip");
pub const state = userField(f32, "state");
pub const pos1 = userField(Vector, "pos1");
pub const pos2 = userField(Vector, "pos2"); // top and bottom positions
pub const height = userField(f32, "height");

//
// sounds
//
pub const waitmin = userField(f32, "waitmin");
pub const waitmax = userField(f32, "waitmax");
pub const distance = userField(f32, "distance");
pub const volume = userField(f32, "volume");

// (The function prototypes at the end of `defs.qc` aren't needed in Zig.)
