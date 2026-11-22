//! A small typed QuakeC AST, type inference variables, and a pretty-printer.

use std::fmt::Write as _;

/// A concrete QuakeC type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QcType {
    Void,
    Float,
    Entity,
    String,
    /// A function value whose signature is not statically known (stored as
    /// `void()`; cast at call sites).
    Func,
    Vector,
    /// A `.string` field reference.
    StringField,
}

impl QcType {
    pub fn name(self) -> &'static str {
        match self {
            QcType::Void => "void",
            QcType::Float => "float",
            QcType::Entity => "entity",
            QcType::String => "string",
            QcType::Func => "void()",
            QcType::Vector => "vector",
            QcType::StringField => ".string",
        }
    }
}

/// A possibly-not-yet-inferred type. Only `externref` values use variables,
/// since they may be either entities or strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Known(QcType),
    Var(usize),
}

impl From<QcType> for Ty {
    fn from(t: QcType) -> Self {
        Ty::Known(t)
    }
}

/// Union-find over externref type variables.
#[derive(Default)]
pub struct TypeVars {
    parent: Vec<usize>,
    known: Vec<Option<QcType>>,
    /// Human-readable descriptions of conflicts found during unification.
    pub conflicts: Vec<String>,
}

impl TypeVars {
    pub fn fresh(&mut self) -> Ty {
        let id = self.parent.len();
        self.parent.push(id);
        self.known.push(None);
        Ty::Var(id)
    }

    fn find(&mut self, v: usize) -> usize {
        let p = self.parent[v];
        if p == v {
            return v;
        }
        let r = self.find(p);
        self.parent[v] = r;
        r
    }

    fn set_known(&mut self, root: usize, t: QcType, ctx: &str) {
        match self.known[root] {
            None => self.known[root] = Some(t),
            Some(k) if k == t => {}
            Some(k) => self.conflicts.push(format!(
                "{ctx}: value used as both {} and {}",
                k.name(),
                t.name()
            )),
        }
    }

    pub fn unify(&mut self, a: Ty, b: Ty, ctx: &str) {
        match (a, b) {
            (Ty::Known(x), Ty::Known(y)) => {
                if x != y {
                    self.conflicts.push(format!(
                        "{ctx}: expected {}, found {}",
                        x.name(),
                        y.name()
                    ));
                }
            }
            (Ty::Var(v), Ty::Known(t)) | (Ty::Known(t), Ty::Var(v)) => {
                let r = self.find(v);
                self.set_known(r, t, ctx);
            }
            (Ty::Var(x), Ty::Var(y)) => {
                let (rx, ry) = (self.find(x), self.find(y));
                if rx != ry {
                    self.parent[ry] = rx;
                    if let Some(t) = self.known[ry] {
                        self.set_known(rx, t, ctx);
                    }
                }
            }
        }
    }

    /// Resolve a type; unconstrained externrefs default to `entity`.
    pub fn resolve(&mut self, t: Ty) -> QcType {
        match t {
            Ty::Known(t) => t,
            Ty::Var(v) => {
                let r = self.find(v);
                self.known[r].unwrap_or(QcType::Entity)
            }
        }
    }
}

/// A function signature (parameters and return), possibly with type variables.
#[derive(Clone, Debug, PartialEq)]
pub struct Sig {
    pub params: Vec<Ty>,
    pub ret: Ty,
}

impl Sig {
    /// `void()`, the type of QuakeC function globals and fields.
    pub fn void() -> Self {
        Sig {
            params: vec![],
            ret: Ty::Known(QcType::Void),
        }
    }
}

impl Sig {
    pub fn render(&self, tv: &mut TypeVars) -> String {
        let params: Vec<_> = self.params.iter().map(|&p| tv.resolve(p).name()).collect();
        format!("{}({})", tv.resolve(self.ret).name(), params.join(", "))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    BitAnd,
    BitOr,
}

impl BinOp {
    fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Float(f32),
    Null,
    /// A function-local variable (parameter, local, or temporary).
    Local(String),
    /// A QuakeC global (system global or wasm global).
    Global(String),
    /// A reference to a function by name.
    FuncName(String),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Un(UnOp, Box<Expr>),
    Select(Box<Expr>, Box<Expr>, Box<Expr>),
    /// Cast a function value to a particular signature.
    FnCast(Sig, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    /// A vector literal, `'x y z'`.
    Vector([f32; 3]),
    /// A string literal, `"..."`. Never contains NUL bytes.
    Str(Vec<u8>),
    /// A component (0..3) of a vector local or global, e.g. `v_x`. The inner
    /// expression must be an `Expr::Local` or `Expr::Global`.
    Component(Box<Expr>, u8),
    /// An entity field, `ent.name`. The entity must be an `Expr::Local` or
    /// `Expr::Global`.
    Field(Box<Expr>, String),
    /// A vector assembled from three float components. Only exists on the
    /// lowering's operand stack, so that lane-by-lane construction can be
    /// collapsed (e.g. into `v_forward`); it is always turned into another
    /// expression before being emitted.
    MakeVector(Box<[Expr; 3]>),
}

impl Expr {
    pub fn is_const(&self) -> bool {
        matches!(
            self,
            Expr::Float(_) | Expr::Null | Expr::FuncName(_) | Expr::Vector(_) | Expr::Str(_)
        )
    }

    /// Whether evaluating this expression reads mutable global state (and so
    /// must be evaluated before any intervening call or store).
    pub fn reads_globals(&self) -> bool {
        match self {
            Expr::Global(_) | Expr::Call(..) | Expr::Field(..) => true,
            Expr::MakeVector(parts) => parts.iter().any(Expr::reads_globals),
            Expr::Float(_)
            | Expr::Null
            | Expr::Local(_)
            | Expr::FuncName(_)
            | Expr::Vector(_)
            | Expr::Str(_) => false,
            Expr::Bin(_, a, b) => a.reads_globals() || b.reads_globals(),
            Expr::Un(_, a) | Expr::FnCast(_, a) | Expr::Component(a, _) => a.reads_globals(),
            Expr::Select(a, b, c) => a.reads_globals() || b.reads_globals() || c.reads_globals(),
        }
    }

    pub fn reads_local(&self, name: &str) -> bool {
        match self {
            Expr::Local(n) => n == name,
            Expr::Float(_)
            | Expr::Null
            | Expr::Global(_)
            | Expr::FuncName(_)
            | Expr::Vector(_)
            | Expr::Str(_) => false,
            Expr::Bin(_, a, b) => a.reads_local(name) || b.reads_local(name),
            Expr::Un(_, a) | Expr::FnCast(_, a) | Expr::Component(a, _) | Expr::Field(a, _) => {
                a.reads_local(name)
            }
            Expr::Select(a, b, c) => {
                a.reads_local(name) || b.reads_local(name) || c.reads_local(name)
            }
            Expr::Call(f, args) => f.reads_local(name) || args.iter().any(|a| a.reads_local(name)),
            Expr::MakeVector(parts) => parts.iter().any(|p| p.reads_local(name)),
        }
    }

    fn render(&self, tv: &mut TypeVars, out: &mut String) {
        match self {
            Expr::Float(f) => {
                if *f < 0.0 || (*f == 0.0 && f.is_sign_negative()) {
                    let _ = write!(out, "({f})");
                } else {
                    let _ = write!(out, "{f}");
                }
            }
            Expr::Null => out.push_str("__NULL__"),
            Expr::Str(bytes) => render_str(bytes, out),
            Expr::Local(n) | Expr::Global(n) | Expr::FuncName(n) => out.push_str(n),
            Expr::Bin(op, a, b) => {
                out.push('(');
                a.render(tv, out);
                let _ = write!(out, " {} ", op.symbol());
                b.render(tv, out);
                out.push(')');
            }
            Expr::Un(op, a) => {
                out.push_str(match op {
                    UnOp::Not => "!",
                    UnOp::Neg => "-",
                });
                out.push('(');
                a.render(tv, out);
                out.push(')');
            }
            Expr::Select(c, a, b) => {
                out.push('(');
                c.render(tv, out);
                out.push_str(" ? ");
                a.render(tv, out);
                out.push_str(" : ");
                b.render(tv, out);
                out.push(')');
            }
            Expr::FnCast(sig, f) => {
                let _ = write!(out, "(({})", sig.render(tv));
                f.render(tv, out);
                out.push(')');
            }
            Expr::Vector([x, y, z]) => {
                let _ = write!(out, "'{x} {y} {z}'");
            }
            Expr::Component(v, i) => {
                v.render(tv, out);
                out.push_str(["_x", "_y", "_z"][*i as usize]);
            }
            Expr::Field(e, name) => {
                e.render(tv, out);
                let _ = write!(out, ".{name}");
            }
            Expr::MakeVector(_) => unreachable!("MakeVector must be materialized before rendering"),
            Expr::Call(f, args) => {
                f.render(tv, out);
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    a.render(tv, out);
                }
                out.push(')');
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Assign(Expr, Expr),
    Expr(Expr),
    Return(Option<Expr>),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    Goto(String),
    Label(String),
    Comment(String),
}

impl Stmt {
    fn render(&self, tv: &mut TypeVars, indent: usize, out: &mut String) {
        let pad = "    ".repeat(indent);
        match self {
            Stmt::Assign(l, r) => {
                out.push_str(&pad);
                l.render(tv, out);
                out.push_str(" = ");
                r.render(tv, out);
                out.push_str(";\n");
            }
            Stmt::Expr(e) => {
                out.push_str(&pad);
                e.render(tv, out);
                out.push_str(";\n");
            }
            Stmt::Return(None) => {
                let _ = writeln!(out, "{pad}return;");
            }
            Stmt::Return(Some(e)) => {
                out.push_str(&pad);
                out.push_str("return ");
                e.render(tv, out);
                out.push_str(";\n");
            }
            Stmt::If(c, t, e) => {
                out.push_str(&pad);
                out.push_str("if (");
                c.render(tv, out);
                out.push_str(") {\n");
                for s in t {
                    s.render(tv, indent + 1, out);
                }
                if e.is_empty() {
                    let _ = writeln!(out, "{pad}}}");
                } else {
                    let _ = writeln!(out, "{pad}}} else {{");
                    for s in e {
                        s.render(tv, indent + 1, out);
                    }
                    let _ = writeln!(out, "{pad}}}");
                }
            }
            Stmt::Goto(l) => {
                let _ = writeln!(out, "{pad}goto {l};");
            }
            Stmt::Label(l) => {
                let _ = writeln!(out, "{l}:");
            }
            Stmt::Comment(c) => {
                let _ = writeln!(out, "{pad}// {c}");
            }
        }
    }
}

/// A QuakeC function definition.
pub struct Function {
    pub name: String,
    pub params: Vec<(String, Ty)>,
    pub ret: Ty,
    pub locals: Vec<(String, Ty)>,
    pub body: Vec<Stmt>,
}

impl Function {
    pub fn render_proto(&self, tv: &mut TypeVars) -> String {
        let params: Vec<_> = self
            .params
            .iter()
            .map(|(n, t)| format!("{} {n}", tv.resolve(*t).name()))
            .collect();
        format!(
            "{} {}({})",
            tv.resolve(self.ret).name(),
            self.name,
            params.join(", ")
        )
    }

    pub fn render(&self, tv: &mut TypeVars, out: &mut String) {
        out.push_str(&self.render_proto(tv));
        out.push_str(" {\n");
        for (n, t) in &self.locals {
            let _ = writeln!(out, "    local {} {n};", tv.resolve(*t).name());
        }
        for s in &self.body {
            s.render(tv, 1, out);
        }
        out.push_str("}\n");
    }
}

/// Render a string literal for fteqcc. Printable ASCII is written as is,
/// everything else is escaped. fteqcc's `\x` takes exactly two hex digits.
fn render_str(bytes: &[u8], out: &mut String) {
    out.push('"');
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            b'\r' => out.push_str("\\r"),
            0x20..=0x7e => out.push(b as char),
            _ => {
                let _ = write!(out, "\\x{b:02x}");
            }
        }
    }
    out.push('"');
}
