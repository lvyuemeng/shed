use super::Emitter;
use crate::ast::{Cond, IfNode, Node};

/// Emits POSIX-compatible sh/bash/zsh.
/// `shell_name` is "bash" or "zsh" — used to fill {shell} in call args.
pub struct BashEmitter {
    pub shell_name: &'static str,
}

impl BashEmitter {
    pub fn new(name: &'static str) -> Self {
        Self { shell_name: name }
    }
}

impl Emitter for BashEmitter {
    fn name(&self) -> &str {
        self.shell_name
    }

    fn emit_nodes(&self, nodes: &[Node], d: usize) -> Vec<String> {
        nodes.iter().flat_map(|n| self.node(n, d)).collect()
    }
}

impl BashEmitter {
    fn node(&self, n: &Node, d: usize) -> Vec<String> {
        match n {
            Node::Set { key, val } => vec![self.indent(format!("export {}=\"{}\"", key, val), d)],

            Node::Path { dir, prepend } => {
                let s = if *prepend {
                    format!("export PATH=\"{}:$PATH\"", dir)
                } else {
                    format!("export PATH=\"$PATH:{}\"", dir)
                };
                vec![self.indent(s, d)]
            }

            Node::Call { cmd, args } => {
                let a = args.replace("{shell}", self.name());
                let a = a.trim();
                let s = if a.is_empty() {
                    format!("eval \"$({})\"", cmd)
                } else {
                    format!("eval \"$({} {})\"", cmd, a)
                };
                vec![self.indent(s, d)]
            }

            Node::If(node) => self.emit_if(node, d),
        }
    }

    fn cond(&self, c: &Cond) -> String {
        match c {
            Cond::Have(cmd) => format!("command -v {} >/dev/null 2>&1", cmd),
            Cond::Os(name) => {
                let uname = match name.as_str() {
                    "darwin" => "Darwin",
                    "linux" => "Linux",
                    "windows" => "Windows_NT",
                    other => other,
                };
                format!("[ \"$(uname -s)\" = \"{}\" ]", uname)
            }
            Cond::Shell(name) => match name.as_str() {
                "bash" => "[ -n \"$BASH_VERSION\" ]".into(),
                "zsh" => "[ -n \"$ZSH_VERSION\" ]".into(),
                _ => "false".into(),
            },
            Cond::Not(inner) => format!("! {}", self.cond(inner)),
            Cond::And(lhs, rhs) => format!("{} && {}", self.cond(lhs), self.cond(rhs)),
            Cond::Or(lhs, rhs) => format!("{} || {}", self.cond(lhs), self.cond(rhs)),
        }
    }

    fn emit_if(&self, n: &IfNode, d: usize) -> Vec<String> {
        let mut out = vec![self.indent(format!("if {}; then", self.cond(&n.cond)), d)];
        out.extend(self.emit_nodes(&n.body, d + 1));
        for (c, b) in &n.elifs {
            out.push(self.indent(format!("elif {}; then", self.cond(c)), d));
            out.extend(self.emit_nodes(b, d + 1));
        }
        if !n.else_.is_empty() {
            out.push(self.indent("else".into(), d));
            out.extend(self.emit_nodes(&n.else_, d + 1));
        }
        out.push(self.indent("fi".into(), d));
        out
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Cond, IfNode, Node};

    fn bash() -> BashEmitter {
        BashEmitter::new("bash")
    }
    fn zsh() -> BashEmitter {
        BashEmitter::new("zsh")
    }

    fn render(e: &BashEmitter, nodes: &[Node]) -> String {
        e.render(nodes)
    }

    #[test]
    fn set() {
        let out = render(
            &bash(),
            &[Node::Set {
                key: "EDITOR".into(),
                val: "nvim".into(),
            }],
        );
        assert_eq!(out, "export EDITOR=\"nvim\"");
    }

    #[test]
    fn path_prepend() {
        let out = render(
            &bash(),
            &[Node::Path {
                dir: "/usr/local/bin".into(),
                prepend: true,
            }],
        );
        assert_eq!(out, "export PATH=\"/usr/local/bin:$PATH\"");
    }

    #[test]
    fn path_append() {
        let out = render(
            &bash(),
            &[Node::Path {
                dir: "/opt/bin".into(),
                prepend: false,
            }],
        );
        assert_eq!(out, "export PATH=\"$PATH:/opt/bin\"");
    }

    #[test]
    fn call_with_shell_placeholder() {
        let out = render(
            &bash(),
            &[Node::Call {
                cmd: "starship".into(),
                args: "init {shell}".into(),
            }],
        );
        assert_eq!(out, "eval \"$(starship init bash)\"");
    }

    #[test]
    fn call_shell_placeholder_zsh() {
        let out = render(
            &zsh(),
            &[Node::Call {
                cmd: "starship".into(),
                args: "init {shell}".into(),
            }],
        );
        assert_eq!(out, "eval \"$(starship init zsh)\"");
    }

    #[test]
    fn call_no_args() {
        let out = render(
            &bash(),
            &[Node::Call {
                cmd: "myprog".into(),
                args: String::new(),
            }],
        );
        assert_eq!(out, "eval \"$(myprog)\"");
    }

    #[test]
    fn cond_have() {
        let e = bash();
        assert_eq!(
            e.cond(&Cond::Have("git".into())),
            "command -v git >/dev/null 2>&1"
        );
    }

    #[test]
    fn cond_os_darwin() {
        let e = bash();
        assert_eq!(
            e.cond(&Cond::Os("darwin".into())),
            "[ \"$(uname -s)\" = \"Darwin\" ]"
        );
    }

    #[test]
    fn cond_os_linux() {
        let e = bash();
        assert_eq!(
            e.cond(&Cond::Os("linux".into())),
            "[ \"$(uname -s)\" = \"Linux\" ]"
        );
    }

    #[test]
    fn cond_os_windows() {
        let e = bash();
        assert_eq!(
            e.cond(&Cond::Os("windows".into())),
            "[ \"$(uname -s)\" = \"Windows_NT\" ]"
        );
    }

    #[test]
    fn cond_shell_bash() {
        let e = bash();
        assert_eq!(
            e.cond(&Cond::Shell("bash".into())),
            "[ -n \"$BASH_VERSION\" ]"
        );
    }

    #[test]
    fn cond_shell_zsh() {
        let e = bash();
        assert_eq!(
            e.cond(&Cond::Shell("zsh".into())),
            "[ -n \"$ZSH_VERSION\" ]"
        );
    }

    #[test]
    fn cond_shell_other_is_false() {
        let e = bash();
        assert_eq!(e.cond(&Cond::Shell("fish".into())), "false");
    }

    #[test]
    fn if_then_fi() {
        let node = Node::If(IfNode {
            cond: Cond::Have("git".into()),
            body: vec![Node::Set {
                key: "X".into(),
                val: "1".into(),
            }],
            elifs: vec![],
            else_: vec![],
        });
        let out = render(&bash(), &[node]);
        assert!(out.contains("if command -v git"), "missing if: {}", out);
        assert!(out.contains("export X=\"1\""), "missing body: {}", out);
        assert!(out.contains("fi"), "missing fi: {}", out);
    }

    #[test]
    fn if_elif_else_fi() {
        let node = Node::If(IfNode {
            cond: Cond::Os("darwin".into()),
            body: vec![Node::Set {
                key: "A".into(),
                val: "1".into(),
            }],
            elifs: vec![(
                Cond::Os("linux".into()),
                vec![Node::Set {
                    key: "A".into(),
                    val: "2".into(),
                }],
            )],
            else_: vec![Node::Set {
                key: "A".into(),
                val: "3".into(),
            }],
        });
        let out = render(&bash(), &[node]);
        assert!(out.contains("elif"), "missing elif: {}", out);
        assert!(out.contains("else"), "missing else: {}", out);
        assert!(out.contains("fi"), "missing fi: {}", out);
    }

    #[test]
    fn indent_depth() {
        let node = Node::If(IfNode {
            cond: Cond::Have("cargo".into()),
            body: vec![Node::Set {
                key: "Y".into(),
                val: "z".into(),
            }],
            elifs: vec![],
            else_: vec![],
        });
        let out = render(&bash(), &[node]);
        assert!(
            out.lines().any(|l| l.starts_with("  export")),
            "body not indented: {}",
            out
        );
    }

    #[test]
    fn cond_not() {
        let e = bash();
        assert_eq!(
            e.cond(&Cond::Not(Box::new(Cond::Have("git".into())))),
            "! command -v git >/dev/null 2>&1"
        );
    }

    #[test]
    fn cond_and() {
        let e = bash();
        assert_eq!(
            e.cond(&Cond::And(
                Box::new(Cond::Have("cargo".into())),
                Box::new(Cond::Os("linux".into())),
            )),
            "command -v cargo >/dev/null 2>&1 && [ \"$(uname -s)\" = \"Linux\" ]"
        );
    }

    #[test]
    fn cond_or() {
        let e = bash();
        assert_eq!(
            e.cond(&Cond::Or(
                Box::new(Cond::Os("darwin".into())),
                Box::new(Cond::Os("linux".into())),
            )),
            "[ \"$(uname -s)\" = \"Darwin\" ] || [ \"$(uname -s)\" = \"Linux\" ]"
        );
    }
}
