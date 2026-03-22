use super::Emitter;
use crate::ast::{Cond, IfNode, Node};

pub struct FishEmitter;

impl Emitter for FishEmitter {
    fn name(&self) -> &str {
        "fish"
    }

    fn emit_nodes(&self, nodes: &[Node], d: usize) -> Vec<String> {
        nodes.iter().flat_map(|n| self.node(n, d)).collect()
    }
}

impl FishEmitter {
    fn node(&self, n: &Node, d: usize) -> Vec<String> {
        match n {
            Node::Set { key, val } => vec![self.indent(format!("set -gx {} \"{}\"", key, val), d)],

            // fish_add_path deduplicates automatically — no double-PATH-entry problem
            Node::Path { dir, prepend } => {
                let flag = if *prepend { "-gP" } else { "-gaP" };
                vec![self.indent(format!("fish_add_path {} \"{}\"", flag, dir), d)]
            }

            Node::Call { cmd, args } => {
                let a = args.replace("{shell}", self.name());
                let a = a.trim();
                let s = if a.is_empty() {
                    format!("{} | source", cmd)
                } else {
                    format!("{} {} | source", cmd, a)
                };
                vec![self.indent(s, d)]
            }

            Node::If(node) => self.emit_if(node, d),
        }
    }

    fn cond(&self, c: &Cond) -> String {
        match c {
            Cond::Have(cmd) => format!("type -q {}", cmd),
            Cond::Os(name) => {
                let uname = match name.as_str() {
                    "darwin" => "Darwin",
                    "linux" => "Linux",
                    "windows" => "Windows_NT",
                    other => other,
                };
                format!("test (uname -s) = \"{}\"", uname)
            }
            Cond::Shell(name) => {
                if name == "fish" {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Cond::Not(inner) => format!("not {}", self.cond(inner)),
            Cond::And(lhs, rhs) => format!("{}; and {}", self.cond(lhs), self.cond(rhs)),
            Cond::Or(lhs, rhs) => format!("{}; or {}", self.cond(lhs), self.cond(rhs)),
        }
    }

    fn emit_if(&self, n: &IfNode, d: usize) -> Vec<String> {
        let mut out = vec![self.indent(format!("if {}", self.cond(&n.cond)), d)];
        out.extend(self.emit_nodes(&n.body, d + 1));
        for (c, b) in &n.elifs {
            out.push(self.indent(format!("else if {}", self.cond(c)), d));
            out.extend(self.emit_nodes(b, d + 1));
        }
        if !n.else_.is_empty() {
            out.push(self.indent("else".into(), d));
            out.extend(self.emit_nodes(&n.else_, d + 1));
        }
        out.push(self.indent("end".into(), d));
        out
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Cond, IfNode, Node};

    fn render(nodes: &[Node]) -> String {
        FishEmitter.render(nodes)
    }

    #[test]
    fn set() {
        let out = render(&[Node::Set {
            key: "EDITOR".into(),
            val: "nvim".into(),
        }]);
        assert_eq!(out, "set -gx EDITOR \"nvim\"");
    }

    #[test]
    fn path_prepend() {
        let out = render(&[Node::Path {
            dir: "/usr/local/bin".into(),
            prepend: true,
        }]);
        assert_eq!(out, "fish_add_path -gP \"/usr/local/bin\"");
    }

    #[test]
    fn path_append() {
        let out = render(&[Node::Path {
            dir: "/opt/bin".into(),
            prepend: false,
        }]);
        assert_eq!(out, "fish_add_path -gaP \"/opt/bin\"");
    }

    #[test]
    fn call_with_shell_placeholder() {
        let out = render(&[Node::Call {
            cmd: "starship".into(),
            args: "init {shell}".into(),
        }]);
        assert_eq!(out, "starship init fish | source");
    }

    #[test]
    fn call_no_args() {
        let out = render(&[Node::Call {
            cmd: "myprog".into(),
            args: String::new(),
        }]);
        assert_eq!(out, "myprog | source");
    }

    #[test]
    fn cond_have() {
        assert_eq!(FishEmitter.cond(&Cond::Have("git".into())), "type -q git");
    }

    #[test]
    fn cond_os_darwin() {
        assert_eq!(
            FishEmitter.cond(&Cond::Os("darwin".into())),
            "test (uname -s) = \"Darwin\""
        );
    }

    #[test]
    fn cond_os_linux() {
        assert_eq!(
            FishEmitter.cond(&Cond::Os("linux".into())),
            "test (uname -s) = \"Linux\""
        );
    }

    #[test]
    fn cond_shell_fish_is_true() {
        assert_eq!(FishEmitter.cond(&Cond::Shell("fish".into())), "true");
    }

    #[test]
    fn cond_shell_other_is_false() {
        assert_eq!(FishEmitter.cond(&Cond::Shell("bash".into())), "false");
    }

    #[test]
    fn if_end() {
        let node = Node::If(IfNode {
            cond: Cond::Have("git".into()),
            body: vec![Node::Set {
                key: "X".into(),
                val: "1".into(),
            }],
            elifs: vec![],
            else_: vec![],
        });
        let out = render(&[node]);
        assert!(out.contains("if type -q git"), "missing if: {}", out);
        assert!(out.contains("set -gx X"), "missing body: {}", out);
        assert!(out.contains("end"), "missing end: {}", out);
    }

    #[test]
    fn if_elif_else_end() {
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
        let out = render(&[node]);
        assert!(out.contains("else if"), "missing else if: {}", out);
        assert!(out.contains("else"), "missing else: {}", out);
        assert!(out.contains("end"), "missing end: {}", out);
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
        let out = render(&[node]);
        assert!(
            out.lines().any(|l| l.starts_with("  set -gx")),
            "body not indented: {}",
            out
        );
    }

    #[test]
    fn cond_not() {
        assert_eq!(
            FishEmitter.cond(&Cond::Not(Box::new(Cond::Have("git".into())))),
            "not type -q git"
        );
    }

    #[test]
    fn cond_and() {
        assert_eq!(
            FishEmitter.cond(&Cond::And(
                Box::new(Cond::Have("cargo".into())),
                Box::new(Cond::Os("linux".into())),
            )),
            "type -q cargo; and test (uname -s) = \"Linux\""
        );
    }

    #[test]
    fn cond_or() {
        assert_eq!(
            FishEmitter.cond(&Cond::Or(
                Box::new(Cond::Os("darwin".into())),
                Box::new(Cond::Os("linux".into())),
            )),
            "test (uname -s) = \"Darwin\"; or test (uname -s) = \"Linux\""
        );
    }
}
