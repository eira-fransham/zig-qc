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
