//! The vanilla Quake system globals and fields.
//!
//! Wasm linear memory is treated as the QuakeC globals area: byte address
//! `slot * 4` corresponds to global slot `slot`. Slots 0..28 are reserved
//! (`OFS_NULL`, `OFS_RETURN`, `OFS_PARM0..7`), and the system globals start at
//! 28. The prelude we emit declares these in exactly this order so that
//! fteqcc places them at the expected offsets (and computes the expected CRC).

use crate::qc::QcType;

/// Type of a system global, as declared in the prelude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysTy {
    Float,
    Entity,
    String,
    Vector,
    /// `void()` function global.
    Func,
}

impl SysTy {
    fn decl(self) -> &'static str {
        match self {
            SysTy::Float => "float",
            SysTy::Entity => "entity",
            SysTy::String => "string",
            SysTy::Vector => "vector",
            SysTy::Func => "void()",
        }
    }
}

/// First slot of the system globals.
pub const SYS_GLOBALS_START: u32 = 28;

/// System globals in declaration order (vanilla NetQuake `defs.qc`).
pub const SYS_GLOBALS: &[(&str, SysTy)] = &[
    ("self", SysTy::Entity),
    ("other", SysTy::Entity),
    ("world", SysTy::Entity),
    ("time", SysTy::Float),
    ("frametime", SysTy::Float),
    ("force_retouch", SysTy::Float),
    ("mapname", SysTy::String),
    ("deathmatch", SysTy::Float),
    ("coop", SysTy::Float),
    ("teamplay", SysTy::Float),
    ("serverflags", SysTy::Float),
    ("total_secrets", SysTy::Float),
    ("total_monsters", SysTy::Float),
    ("found_secrets", SysTy::Float),
    ("killed_monsters", SysTy::Float),
    ("parm1", SysTy::Float),
    ("parm2", SysTy::Float),
    ("parm3", SysTy::Float),
    ("parm4", SysTy::Float),
    ("parm5", SysTy::Float),
    ("parm6", SysTy::Float),
    ("parm7", SysTy::Float),
    ("parm8", SysTy::Float),
    ("parm9", SysTy::Float),
    ("parm10", SysTy::Float),
    ("parm11", SysTy::Float),
    ("parm12", SysTy::Float),
    ("parm13", SysTy::Float),
    ("parm14", SysTy::Float),
    ("parm15", SysTy::Float),
    ("parm16", SysTy::Float),
    ("v_forward", SysTy::Vector),
    ("v_up", SysTy::Vector),
    ("v_right", SysTy::Vector),
    ("trace_allsolid", SysTy::Float),
    ("trace_startsolid", SysTy::Float),
    ("trace_fraction", SysTy::Float),
    ("trace_endpos", SysTy::Vector),
    ("trace_plane_normal", SysTy::Vector),
    ("trace_plane_dist", SysTy::Float),
    ("trace_ent", SysTy::Entity),
    ("trace_inopen", SysTy::Float),
    ("trace_inwater", SysTy::Float),
    ("msg_entity", SysTy::Entity),
    ("main", SysTy::Func),
    ("StartFrame", SysTy::Func),
    ("PlayerPreThink", SysTy::Func),
    ("PlayerPostThink", SysTy::Func),
    ("ClientKill", SysTy::Func),
    ("ClientConnect", SysTy::Func),
    ("PutClientInServer", SysTy::Func),
    ("ClientDisconnect", SysTy::Func),
    ("SetNewParms", SysTy::Func),
    ("SetChangeParms", SysTy::Func),
];

/// System entity fields in declaration order (vanilla NetQuake `defs.qc`).
pub const SYS_FIELDS: &[(&str, &str)] = &[
    ("float", "modelindex"),
    ("vector", "absmin"),
    ("vector", "absmax"),
    ("float", "ltime"),
    ("float", "movetype"),
    ("float", "solid"),
    ("vector", "origin"),
    ("vector", "oldorigin"),
    ("vector", "velocity"),
    ("vector", "angles"),
    ("vector", "avelocity"),
    ("vector", "punchangle"),
    ("string", "classname"),
    ("string", "model"),
    ("float", "frame"),
    ("float", "skin"),
    ("float", "effects"),
    ("vector", "mins"),
    ("vector", "maxs"),
    ("vector", "size"),
    ("void()", "touch"),
    ("void()", "use"),
    ("void()", "think"),
    ("void()", "blocked"),
    ("float", "nextthink"),
    ("entity", "groundentity"),
    ("float", "health"),
    ("float", "frags"),
    ("float", "weapon"),
    ("string", "weaponmodel"),
    ("float", "weaponframe"),
    ("float", "currentammo"),
    ("float", "ammo_shells"),
    ("float", "ammo_nails"),
    ("float", "ammo_rockets"),
    ("float", "ammo_cells"),
    ("float", "items"),
    ("float", "takedamage"),
    ("entity", "chain"),
    ("float", "deadflag"),
    ("vector", "view_ofs"),
    ("float", "button0"),
    ("float", "button1"),
    ("float", "button2"),
    ("float", "impulse"),
    ("float", "fixangle"),
    ("vector", "v_angle"),
    ("float", "idealpitch"),
    ("string", "netname"),
    ("entity", "enemy"),
    ("float", "flags"),
    ("float", "colormap"),
    ("float", "team"),
    ("float", "max_health"),
    ("float", "teleport_time"),
    ("float", "armortype"),
    ("float", "armorvalue"),
    ("float", "waterlevel"),
    ("float", "watertype"),
    ("float", "ideal_yaw"),
    ("float", "yaw_speed"),
    ("entity", "aiment"),
    ("entity", "goalentity"),
    ("float", "spawnflags"),
    ("string", "target"),
    ("string", "targetname"),
    ("float", "dmg_take"),
    ("float", "dmg_save"),
    ("entity", "dmg_inflictor"),
    ("entity", "owner"),
    ("vector", "movedir"),
    ("string", "message"),
    ("float", "sounds"),
    ("string", "noise"),
    ("string", "noise1"),
    ("string", "noise2"),
    ("string", "noise3"),
];

pub fn is_vector_global(name: &str) -> bool {
    SYS_GLOBALS.iter().any(|&(n, t)| n == name && t == SysTy::Vector)
}

pub fn is_vector_field(name: &str) -> bool {
    SYS_FIELDS.iter().any(|&(t, n)| n == name && t == "vector")
}

/// Look up a system field by its QuakeC field offset. A float access to an
/// offset inside a vector field refers to that component (e.g. `origin_y`).
pub fn field_at(offset: u32, ty: QcType) -> Option<String> {
    let mut start = 0;
    for &(fty, name) in SYS_FIELDS {
        let fty = match fty {
            "float" => QcType::Float,
            "vector" => QcType::Vector,
            "string" => QcType::String,
            "entity" => QcType::Entity,
            "void()" => QcType::Func,
            other => unreachable!("unknown field type {other}"),
        };
        let len = if fty == QcType::Vector { 3 } else { 1 };
        if (start..start + len).contains(&offset) {
            let i = (offset - start) as usize;
            return match (fty, ty) {
                _ if fty == ty && i == 0 => Some(name.to_string()),
                (QcType::Vector, QcType::Float) => Some(format!("{name}_{}", ["x", "y", "z"][i])),
                _ => None,
            };
        }
        start += len;
    }
    None
}

/// A single 32-bit global slot that wasm code can address.
#[derive(Clone, Debug)]
pub struct GlobalSlot {
    /// QuakeC expression naming this slot (e.g. `v_up_x`).
    pub name: String,
    pub ty: QcType,
    /// Whether this is a `void()` function global from the system defs.
    pub is_sys_func: bool,
}

/// Maps slot index → addressable global.
pub struct GlobalMap {
    slots: Vec<Option<GlobalSlot>>,
}

impl GlobalMap {
    pub fn vanilla() -> Self {
        let mut slots = vec![None; SYS_GLOBALS_START as usize];
        for &(name, ty) in SYS_GLOBALS {
            match ty {
                SysTy::Vector => {
                    for c in ["x", "y", "z"] {
                        slots.push(Some(GlobalSlot {
                            name: format!("{name}_{c}"),
                            ty: QcType::Float,
                            is_sys_func: false,
                        }));
                    }
                }
                _ => slots.push(Some(GlobalSlot {
                    name: name.to_string(),
                    ty: match ty {
                        SysTy::Float => QcType::Float,
                        SysTy::Entity => QcType::Entity,
                        SysTy::String => QcType::String,
                        SysTy::Func => QcType::Func,
                        SysTy::Vector => unreachable!(),
                    },
                    is_sys_func: ty == SysTy::Func,
                })),
            }
        }
        Self { slots }
    }

    /// Look up the global at a byte address.
    pub fn by_addr(&self, addr: u64) -> Option<&GlobalSlot> {
        if !addr.is_multiple_of(4) {
            return None;
        }
        self.slots.get((addr / 4) as usize)?.as_ref()
    }
}

/// Emit the system defs prelude.
///
/// `defined_func_proto` returns a prototype for system function globals that
/// the module itself defines (via an export of the same name); those are
/// declared as function prototypes instead of as `var void()`.
pub fn prelude(defined_func_proto: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    out.push_str("// ---- system globals (must match vanilla defs.qc layout) ----\n");
    for &(name, ty) in SYS_GLOBALS {
        if ty == SysTy::Func {
            match defined_func_proto(name) {
                Some(proto) => out.push_str(&format!("{proto};\n")),
                None => out.push_str(&format!("var void() {name};\n")),
            }
        } else {
            out.push_str(&format!("{} {name};\n", ty.decl()));
        }
    }
    out.push_str("void end_sys_globals;\n\n// ---- system fields ----\n");
    for &(ty, name) in SYS_FIELDS {
        out.push_str(&format!(".{ty} {name};\n"));
    }
    out.push_str("void end_sys_fields;\n");
    out
}

/// A vanilla NetQuake builtin: name, number, return type and parameter types.
pub struct Builtin {
    pub name: &'static str,
    pub number: u32,
    pub ret: QcType,
    pub params: &'static [QcType],
}

macro_rules! builtins {
    ($($num:literal $name:literal : $ret:ident ( $($p:ident),* );)*) => {
        &[$(Builtin {
            name: $name,
            number: $num,
            ret: QcType::$ret,
            params: &[$(QcType::$p),*],
        }),*]
    };
}

/// Vanilla NetQuake builtins (from `defs.qc`), excluding removed slots.
pub const BUILTINS: &[Builtin] = builtins! {
    1 "makevectors": Void(Vector);
    2 "setorigin": Void(Entity, Vector);
    3 "setmodel": Void(Entity, String);
    4 "setsize": Void(Entity, Vector, Vector);
    6 "break": Void();
    7 "random": Float();
    8 "sound": Void(Entity, Float, String, Float, Float);
    9 "normalize": Vector(Vector);
    10 "error": Void(String);
    11 "objerror": Void(String);
    12 "vlen": Float(Vector);
    13 "vectoyaw": Float(Vector);
    14 "spawn": Entity();
    15 "remove": Void(Entity);
    16 "traceline": Void(Vector, Vector, Float, Entity);
    17 "checkclient": Entity();
    18 "find": Entity(Entity, StringField, String);
    19 "precache_sound": String(String);
    20 "precache_model": String(String);
    21 "stuffcmd": Void(Entity, String);
    22 "findradius": Entity(Vector, Float);
    23 "bprint": Void(String);
    24 "sprint": Void(Entity, String);
    25 "dprint": Void(String);
    26 "ftos": String(Float);
    27 "vtos": String(Vector);
    28 "coredump": Void();
    29 "traceon": Void();
    30 "traceoff": Void();
    31 "eprint": Void(Entity);
    32 "walkmove": Float(Float, Float);
    34 "droptofloor": Float();
    35 "lightstyle": Void(Float, String);
    36 "rint": Float(Float);
    37 "floor": Float(Float);
    38 "ceil": Float(Float);
    40 "checkbottom": Float(Entity);
    41 "pointcontents": Float(Vector);
    43 "fabs": Float(Float);
    44 "aim": Vector(Entity, Float);
    45 "cvar": Float(String);
    46 "localcmd": Void(String);
    47 "nextent": Entity(Entity);
    48 "particle": Void(Vector, Vector, Float, Float);
    49 "ChangeYaw": Void();
    51 "vectoangles": Vector(Vector);
    52 "WriteByte": Void(Float, Float);
    53 "WriteChar": Void(Float, Float);
    54 "WriteShort": Void(Float, Float);
    55 "WriteLong": Void(Float, Float);
    56 "WriteCoord": Void(Float, Float);
    57 "WriteAngle": Void(Float, Float);
    58 "WriteString": Void(Float, String);
    59 "WriteEntity": Void(Float, Entity);
    67 "movetogoal": Void(Float);
    68 "precache_file": String(String);
    69 "makestatic": Void(Entity);
    70 "changelevel": Void(String);
    72 "cvar_set": Void(String, String);
    73 "centerprint": Void(Entity, String);
    74 "ambientsound": Void(Vector, String, Float, Float);
    75 "precache_model2": String(String);
    76 "precache_sound2": String(String);
    77 "precache_file2": String(String);
    78 "setspawnparms": Void(Entity);
};

pub fn builtin(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}
