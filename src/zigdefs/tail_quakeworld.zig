// ============================================================================
//
// VARS NOT REFERENCED BY C CODE
//
// (The rest of `qw-qc/defs.qc`: game constants, globals and fields.)
//
// ============================================================================

pub const FALSE: f32 = 0;
pub const TRUE: f32 = 1;

// edict.flags
pub const FL_FLY: f32 = 1;
pub const FL_SWIM: f32 = 2;
pub const FL_CLIENT: f32 = 8; // set for all client edicts
pub const FL_INWATER: f32 = 16; // for enter / leave water splash
pub const FL_MONSTER: f32 = 32;
pub const FL_GODMODE: f32 = 64; // player cheat
pub const FL_NOTARGET: f32 = 128; // player cheat
pub const FL_ITEM: f32 = 256; // extra wide size for bonus items
pub const FL_ONGROUND: f32 = 512; // standing on something
pub const FL_PARTIALGROUND: f32 = 1024; // not all corners are valid
pub const FL_WATERJUMP: f32 = 2048; // player jumping out of water
pub const FL_JUMPRELEASED: f32 = 4096; // for jump debouncing

// edict.movetype values
pub const MOVETYPE_NONE: f32 = 0; // never moves
pub const MOVETYPE_WALK: f32 = 3; // players only
pub const MOVETYPE_STEP: f32 = 4; // discrete, not real time unless fall
pub const MOVETYPE_FLY: f32 = 5;
pub const MOVETYPE_TOSS: f32 = 6; // gravity
pub const MOVETYPE_PUSH: f32 = 7; // no clip to world, push and crush
pub const MOVETYPE_NOCLIP: f32 = 8;
pub const MOVETYPE_FLYMISSILE: f32 = 9; // fly with extra size against monsters
pub const MOVETYPE_BOUNCE: f32 = 10;
pub const MOVETYPE_BOUNCEMISSILE: f32 = 11; // bounce with extra size

// edict.solid values
pub const SOLID_NOT: f32 = 0; // no interaction with other objects
pub const SOLID_TRIGGER: f32 = 1; // touch on edge, but not blocking
pub const SOLID_BBOX: f32 = 2; // touch on edge, block
pub const SOLID_SLIDEBOX: f32 = 3; // touch on edge, but not an onground
pub const SOLID_BSP: f32 = 4; // bsp clip, touch on edge, block

// range values
pub const RANGE_MELEE: f32 = 0;
pub const RANGE_NEAR: f32 = 1;
pub const RANGE_MID: f32 = 2;
pub const RANGE_FAR: f32 = 3;

// deadflag values
pub const DEAD_NO: f32 = 0;
pub const DEAD_DYING: f32 = 1;
pub const DEAD_DEAD: f32 = 2;
pub const DEAD_RESPAWNABLE: f32 = 3;

// takedamage values
pub const DAMAGE_NO: f32 = 0;
pub const DAMAGE_YES: f32 = 1;
pub const DAMAGE_AIM: f32 = 2;

// items
pub const IT_AXE: f32 = 4096;
pub const IT_SHOTGUN: f32 = 1;
pub const IT_SUPER_SHOTGUN: f32 = 2;
pub const IT_NAILGUN: f32 = 4;
pub const IT_SUPER_NAILGUN: f32 = 8;
pub const IT_GRENADE_LAUNCHER: f32 = 16;
pub const IT_ROCKET_LAUNCHER: f32 = 32;
pub const IT_LIGHTNING: f32 = 64;
pub const IT_EXTRA_WEAPON: f32 = 128;

pub const IT_SHELLS: f32 = 256;
pub const IT_NAILS: f32 = 512;
pub const IT_ROCKETS: f32 = 1024;
pub const IT_CELLS: f32 = 2048;

pub const IT_ARMOR1: f32 = 8192;
pub const IT_ARMOR2: f32 = 16384;
pub const IT_ARMOR3: f32 = 32768;
pub const IT_SUPERHEALTH: f32 = 65536;

pub const IT_KEY1: f32 = 131072;
pub const IT_KEY2: f32 = 262144;

pub const IT_INVISIBILITY: f32 = 524288;
pub const IT_INVULNERABILITY: f32 = 1048576;
pub const IT_SUIT: f32 = 2097152;
pub const IT_QUAD: f32 = 4194304;

// point content values
pub const CONTENT_EMPTY: f32 = -1;
pub const CONTENT_SOLID: f32 = -2;
pub const CONTENT_WATER: f32 = -3;
pub const CONTENT_SLIME: f32 = -4;
pub const CONTENT_LAVA: f32 = -5;
pub const CONTENT_SKY: f32 = -6;

pub const STATE_TOP: f32 = 0;
pub const STATE_BOTTOM: f32 = 1;
pub const STATE_UP: f32 = 2;
pub const STATE_DOWN: f32 = 3;

pub const VEC_ORIGIN: Vector = .{ 0, 0, 0 };
pub const VEC_HULL_MIN: Vector = .{ -16, -16, -24 };
pub const VEC_HULL_MAX: Vector = .{ 16, 16, 32 };

pub const VEC_HULL2_MIN: Vector = .{ -32, -32, -24 };
pub const VEC_HULL2_MAX: Vector = .{ 32, 32, 64 };

// protocol bytes
pub const SVC_TEMPENTITY: f32 = 23;
pub const SVC_KILLEDMONSTER: f32 = 27;
pub const SVC_FOUNDSECRET: f32 = 28;
pub const SVC_INTERMISSION: f32 = 30;
pub const SVC_FINALE: f32 = 31;
pub const SVC_CDTRACK: f32 = 32;
pub const SVC_SELLSCREEN: f32 = 33;
pub const SVC_SMALLKICK: f32 = 34;
pub const SVC_BIGKICK: f32 = 35;
pub const SVC_MUZZLEFLASH: f32 = 39;

pub const TE_SPIKE: f32 = 0;
pub const TE_SUPERSPIKE: f32 = 1;
pub const TE_GUNSHOT: f32 = 2;
pub const TE_EXPLOSION: f32 = 3;
pub const TE_TAREXPLOSION: f32 = 4;
pub const TE_LIGHTNING1: f32 = 5;
pub const TE_LIGHTNING2: f32 = 6;
pub const TE_WIZSPIKE: f32 = 7;
pub const TE_KNIGHTSPIKE: f32 = 8;
pub const TE_LIGHTNING3: f32 = 9;
pub const TE_LAVASPLASH: f32 = 10;
pub const TE_TELEPORT: f32 = 11;
pub const TE_BLOOD: f32 = 12;
pub const TE_LIGHTNINGBLOOD: f32 = 13;

// sound channels
// channel 0 never willingly overrides
// other channels (1-7) allways override a playing sound on that channel
pub const CHAN_AUTO: f32 = 0;
pub const CHAN_WEAPON: f32 = 1;
pub const CHAN_VOICE: f32 = 2;
pub const CHAN_ITEM: f32 = 3;
pub const CHAN_BODY: f32 = 4;
pub const CHAN_NO_PHS_ADD: f32 = 8; // ie: CHAN_BODY+CHAN_NO_PHS_ADD

pub const ATTN_NONE: f32 = 0;
pub const ATTN_NORM: f32 = 1;
pub const ATTN_IDLE: f32 = 2;
pub const ATTN_STATIC: f32 = 3;

// update types
pub const UPDATE_GENERAL: f32 = 0;
pub const UPDATE_STATIC: f32 = 1;
pub const UPDATE_BINARY: f32 = 2;
pub const UPDATE_TEMP: f32 = 3;

// entity effects
pub const EF_BRIGHTLIGHT: f32 = 4;
pub const EF_DIMLIGHT: f32 = 8;
pub const EF_FLAG1: f32 = 16;
pub const EF_FLAG2: f32 = 32;
// GLQuakeWorld Stuff
pub const EF_BLUE: f32 = 64; // Blue Globe effect for Quad
pub const EF_RED: f32 = 128; // Red Globe effect for Pentagram

// messages
pub const MSG_BROADCAST: f32 = 0; // unreliable to all
pub const MSG_ONE: f32 = 1; // reliable to one (msg_entity)
pub const MSG_ALL: f32 = 2; // reliable to all
pub const MSG_INIT: f32 = 3; // write to the init string
pub const MSG_MULTICAST: f32 = 4; // for multicast() call

// message levels
pub const PRINT_LOW: f32 = 0; // pickup messages
pub const PRINT_MEDIUM: f32 = 1; // death messages
pub const PRINT_HIGH: f32 = 2; // critical messages
pub const PRINT_CHAT: f32 = 3; // also goes to chat console

// multicast sets
pub const MULTICAST_ALL: f32 = 0; // every client
pub const MULTICAST_PHS: f32 = 1; // within hearing
pub const MULTICAST_PVS: f32 = 2; // within sight
pub const MULTICAST_ALL_R: f32 = 3; // every client, reliable
pub const MULTICAST_PHS_R: f32 = 4; // within hearing, reliable
pub const MULTICAST_PVS_R: f32 = 5; // within sight, reliable

// values of the `attack_state` field
pub const AS_STRAIGHT: f32 = 1;
pub const AS_SLIDING: f32 = 2;
pub const AS_MELEE: f32 = 3;
pub const AS_MISSILE: f32 = 4;

// (The non-engine globals and fields of `defs.qc` belong to the project.)

