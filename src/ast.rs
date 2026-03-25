//! AST for .shed files.
//! Intentionally flat — no expression trees, no precedence, no ambiguity.

/// Structured parse error carrying the 1-based source line number.
/// `line == 0` means EOF or a location that cannot be expressed as a line.
#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

impl ParseError {
    #[inline]
    pub fn at(line: usize, msg: impl Into<String>) -> Self {
        Self {
            line,
            msg: msg.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.line == 0 {
            f.write_str(&self.msg)
        } else {
            write!(f, "line {}: {}", self.line, self.msg)
        }
    }
}

/// Whether a `path` directive prepends or appends to `PATH`.
///
/// Named enum instead of `bool` eliminates boolean-blindness at call sites.
/// `Copy` means PathDir is passed in registers with zero allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PathDir {
    Prepend = 0, // path+
    Append = 1,  // path-
}

/// A single statement in a .shed source file.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Set { key: String, val: String },
    Path { dir: String, direction: PathDir }, // path+ / path-
    Call { cmd: String, args: String },       // eval-init style (starship, zoxide…)
    Alias { name: String, body: String },     // alias name body
    If(IfNode),
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfNode {
    pub cond: Cond,
    pub body: Vec<Node>,
    pub elifs: Vec<(Cond, Vec<Node>)>,
    pub else_: Vec<Node>,
}

/// A runtime or compile-time condition.
///
/// `Box<Cond>` for recursive variants is the minimum indirection needed to
/// break the size cycle. One heap allocation per compound node.
#[derive(Debug, Clone, PartialEq)]
pub enum Cond {
    Have(String),              // if have <cmd>    -- command on PATH (runtime)
    Exists(String),            // if exists <path> -- filesystem presence (runtime)
    Env(String),               // if env <VAR>     -- env-var is set & non-empty (runtime)
    Os(String),                // if os   darwin | linux | windows (compile-time fold)
    Shell(String),             // if shell bash | zsh | fish | pwsh (compile-time fold)
    Not(Box<Cond>),            // if not <cond>
    And(Box<Cond>, Box<Cond>), // if <cond> and <cond>
    Or(Box<Cond>, Box<Cond>),  // if <cond> or <cond>
}
