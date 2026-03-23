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
            write!(f, "{}", self.msg)
        } else {
            write!(f, "line {}: {}", self.line, self.msg)
        }
    }
}

#[derive(Debug, Clone)]
pub enum Node {
    Set { key: String, val: String },
    Path { dir: String, prepend: bool },  // path+ / path-
    Call { cmd: String, args: String },   // eval-init style (starship, zoxide…)
    Alias { name: String, body: String }, // alias name body
    If(IfNode),
}

#[derive(Debug, Clone)]
pub struct IfNode {
    pub cond: Cond,
    pub body: Vec<Node>,
    pub elifs: Vec<(Cond, Vec<Node>)>,
    pub else_: Vec<Node>,
}

#[derive(Debug, Clone)]
pub enum Cond {
    Have(String),              // if have <cmd>
    Os(String),                // if os   darwin | linux | windows
    Shell(String),             // if shell bash | zsh | fish | pwsh
    Not(Box<Cond>),            // if not <cond>
    And(Box<Cond>, Box<Cond>), // if <cond> and <cond>
    Or(Box<Cond>, Box<Cond>),  // if <cond> or <cond>
}
