use super::Emitter;
use crate::ast::{Cond, IfNode, Node};

pub struct PwshEmitter;

impl Emitter for PwshEmitter {
    fn name(&self) -> &str {
        "pwsh"
    }

    fn emit_nodes(&self, nodes: &[Node], d: usize) -> Vec<String> {
        nodes.iter().flat_map(|n| self.node(n, d)).collect()
    }
}

impl PwshEmitter {
    fn node(&self, n: &Node, d: usize) -> Vec<String> {
        match n {
            Node::Set { key, val } => vec![self.indent(format!("$env:{} = \"{}\"", key, val), d)],

            Node::Path { dir, prepend: true } => {
                vec![self.indent(format!("$env:PATH = \"{};$env:PATH\"", dir), d)]
            }

            Node::Path {
                dir,
                prepend: false,
            } => vec![self.indent(format!("$env:PATH = \"$env:PATH;{}\"", dir), d)],

            Node::Inject { cmd, args } => {
                let a = args.replace("{shell}", "powershell");
                let a = a.trim();
                let call = if a.is_empty() {
                    format!("Invoke-Expression (& {})", cmd)
                } else {
                    format!("Invoke-Expression (& {} {})", cmd, a)
                };
                vec![self.indent(call, d)]
            }

            Node::If(node) => self.emit_if(node, d),
        }
    }

    fn cond(&self, c: &Cond) -> String {
        match c {
            Cond::Have(cmd) => format!("Get-Command {} -ErrorAction SilentlyContinue", cmd),
            Cond::Os(name) => match name.as_str() {
                "darwin" => "$IsMacOS".into(),
                "linux" => "$IsLinux".into(),
                "windows" => "$IsWindows".into(),
                other => format!("$false # unknown os: {}", other),
            },
            Cond::Shell(name) => {
                if name == "pwsh" {
                    "$true".into()
                } else {
                    "$false".into()
                }
            }
        }
    }

    fn emit_if(&self, n: &IfNode, d: usize) -> Vec<String> {
        let mut out = vec![self.indent(format!("if ({}) {{", self.cond(&n.cond)), d)];
        out.extend(self.emit_nodes(&n.body, d + 1));
        for (c, b) in &n.elifs {
            out.push(self.indent(format!("}} elseif ({}) {{", self.cond(c)), d));
            out.extend(self.emit_nodes(b, d + 1));
        }
        if !n.else_.is_empty() {
            out.push(self.indent("} else {", d));
            out.extend(self.emit_nodes(&n.else_, d + 1));
        }
        out.push(self.indent("}", d));
        out
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Cond, IfNode, Node};

    fn render(nodes: &[Node]) -> String {
        PwshEmitter.render(nodes)
    }

    #[test]
    fn set() {
        let out = render(&[Node::Set {
            key: "EDITOR".into(),
            val: "nvim".into(),
        }]);
        assert_eq!(out, "$env:EDITOR = \"nvim\"");
    }

    #[test]
    fn path_prepend() {
        let out = render(&[Node::Path {
            dir: "C:\\tools".into(),
            prepend: true,
        }]);
        assert_eq!(out, "$env:PATH = \"C:\\tools;$env:PATH\"");
    }

    #[test]
    fn path_append() {
        let out = render(&[Node::Path {
            dir: "C:\\opt".into(),
            prepend: false,
        }]);
        assert_eq!(out, "$env:PATH = \"$env:PATH;C:\\opt\"");
    }

    #[test]
    fn inject_with_shell_placeholder() {
        let out = render(&[Node::Inject {
            cmd: "starship".into(),
            args: "init {shell}".into(),
        }]);
        assert_eq!(out, "Invoke-Expression (& starship init powershell)");
    }

    #[test]
    fn inject_no_args() {
        let out = render(&[Node::Inject {
            cmd: "myprog".into(),
            args: String::new(),
        }]);
        assert_eq!(out, "Invoke-Expression (& myprog)");
    }

    #[test]
    fn cond_have() {
        assert_eq!(
            PwshEmitter.cond(&Cond::Have("git".into())),
            "Get-Command git -ErrorAction SilentlyContinue"
        );
    }

    #[test]
    fn cond_os_darwin() {
        assert_eq!(PwshEmitter.cond(&Cond::Os("darwin".into())), "$IsMacOS");
    }
    #[test]
    fn cond_os_linux() {
        assert_eq!(PwshEmitter.cond(&Cond::Os("linux".into())), "$IsLinux");
    }
    #[test]
    fn cond_os_windows() {
        assert_eq!(PwshEmitter.cond(&Cond::Os("windows".into())), "$IsWindows");
    }

    #[test]
    fn cond_shell_pwsh_is_true() {
        assert_eq!(PwshEmitter.cond(&Cond::Shell("pwsh".into())), "$true");
    }

    #[test]
    fn cond_shell_other_is_false() {
        assert_eq!(PwshEmitter.cond(&Cond::Shell("bash".into())), "$false");
    }

    #[test]
    fn if_braces() {
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
        assert!(out.contains("if (Get-Command git"), "missing if: {}", out);
        assert!(out.contains("$env:X = \"1\""), "missing body: {}", out);
        assert!(out.ends_with("}"), "missing close: {}", out);
    }

    #[test]
    fn if_elif_else_braces() {
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
        assert!(out.contains("elseif"), "missing elseif: {}", out);
        assert!(out.contains("} else {"), "missing else: {}", out);
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
            out.lines().any(|l| l.starts_with("  $env:")),
            "body not indented: {}",
            out
        );
    }
}
