/// AST for .shed files.
/// Intentionally flat — no expression trees, no precedence, no ambiguity.

#[derive(Debug, Clone)]
pub enum Node {
    Set    { key: String, val: String },
    Path   { dir: String, prepend: bool },   // path+ / path-
    Inject { cmd: String, args: String },    // eval-init style (starship, zoxide…)
    If(IfNode),
}

#[derive(Debug, Clone)]
pub struct IfNode {
    pub cond:  Cond,
    pub body:  Vec<Node>,
    pub elifs: Vec<(Cond, Vec<Node>)>,
    pub else_: Vec<Node>,
}

#[derive(Debug, Clone)]
pub enum Cond {
    Have(String),   // if have <cmd>
    Os(String),     // if os   darwin | linux | windows
    Shell(String),  // if shell bash | zsh | fish | pwsh
}