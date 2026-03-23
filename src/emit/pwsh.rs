use super::Emitter;
use crate::ast::{Cond, IfNode, Node, PathDir};

pub struct PwshEmitter;

impl Emitter for PwshEmitter {
    #[inline]
    fn name(&self) -> &str {
        "pwsh"
    }

    fn emit_nodes(&self, nodes: &[Node], d: usize) -> Vec<String> {
        let mut out = Vec::with_capacity(nodes.len());
        for n in nodes {
            self.node(n, d, &mut out);
        }
        out
    }
}

impl PwshEmitter {
    fn node(&self, n: &Node, d: usize, out: &mut Vec<String>) {
        match n {
            Node::Set { key, val } => {
                out.push(self.indent(format!("$env:{} = \"{}\"", key, val), d));
            }

            Node::Path { dir, direction } => {
                // Dedup guard: only mutate PATH when `dir` is not already present.
                let add = match direction {
                    PathDir::Prepend => format!("$env:PATH = \"{};$env:PATH\"", dir),
                    PathDir::Append  => format!("$env:PATH = \"$env:PATH;{}\"", dir),
                };
                let guard = format!("if ($env:PATH -notlike '*{dir}*') {{ {add} }}");
                out.push(self.indent(guard, d));
            }

            Node::Call { cmd, args } => {
                // pwsh uses "powershell" as the {shell} replacement for init-command compatibility.
                let a = args.replace("{shell}", "powershell");
                let a = a.trim();
                let s = if a.is_empty() {
                    format!("Invoke-Expression (& {})", cmd)
                } else {
                    format!("Invoke-Expression (& {} {})", cmd, a)
                };
                out.push(self.indent(s, d));
            }

            Node::Alias { name, body } => {
                out.push(self.indent(format!("Set-Alias {} {}", name, body), d));
            }

            Node::If(node) => self.emit_if(node, d, out),
        }
    }

    fn cond(&self, c: &Cond) -> String {
        match c {
            Cond::Have(cmd)    => format!("Get-Command {} -ErrorAction SilentlyContinue", cmd),
            Cond::Exists(path) => format!("Test-Path \"{}\"", path),
            Cond::Env(var)     => format!("(Test-Path env:{})", var),
            Cond::Os(name)     => match name.as_str() {
                "darwin"  => "$IsMacOS".into(),
                "linux"   => "$IsLinux".into(),
                "windows" => "$IsWindows".into(),
                other     => format!("$false # unknown os: {}", other),
            },
            Cond::Shell(name) => {
                if name == "pwsh" { "$true".into() } else { "$false".into() }
            }
            Cond::Not(inner) => {
                // Exact-capacity: "(-not (" + inner + "))"
                let inner_s = self.cond(inner);
                let mut s = String::with_capacity(8 + inner_s.len() + 2);
                s.push_str("(-not (");
                s.push_str(&inner_s);
                s.push_str("))");
                s
            }
            Cond::And(lhs, rhs) => {
                let l = self.cond(lhs);
                let r = self.cond(rhs);
                // "(" + l + ") -and (" + r + ")"
                let mut s = String::with_capacity(1 + l.len() + 8 + r.len() + 1);
                s.push('(');
                s.push_str(&l);
                s.push_str(") -and (");
                s.push_str(&r);
                s.push(')');
                s
            }
            Cond::Or(lhs, rhs) => {
                let l = self.cond(lhs);
                let r = self.cond(rhs);
                let mut s = String::with_capacity(1 + l.len() + 7 + r.len() + 1);
                s.push('(');
                s.push_str(&l);
                s.push_str(") -or (");
                s.push_str(&r);
                s.push(')');
                s
            }
        }
    }

    fn emit_if(&self, n: &IfNode, d: usize, out: &mut Vec<String>) {
        out.reserve(2 + n.body.len() + n.elifs.len() * 2 + n.else_.len());
        out.push(self.indent(format!("if ({}) {{", self.cond(&n.cond)), d));
        for node in &n.body {
            self.node(node, d + 1, out);
        }
        for (c, b) in &n.elifs {
            out.push(self.indent(format!("}} elseif ({}) {{", self.cond(c)), d));
            for node in b {
                self.node(node, d + 1, out);
            }
        }
        if !n.else_.is_empty() {
            out.push(self.indent("} else {".into(), d));
            for node in &n.else_ {
                self.node(node, d + 1, out);
            }
        }
        out.push(self.indent("}".into(), d));
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
        let out = render(&[Node::Set { key: "EDITOR".into(), val: "nvim".into() }]);
        assert_eq!(out, "$env:EDITOR = \"nvim\"");
    }

    #[test]
    fn path_prepend() {
        let out = render(&[Node::Path {
            dir: "C:\\tools".into(),
            direction: PathDir::Prepend,
        }]);
        assert!(out.contains("$env:PATH = \"C:\\tools;$env:PATH\""), "add: {}", out);
        assert!(out.contains("-notlike"), "guard: {}", out);
    }

    #[test]
    fn path_append() {
        let out = render(&[Node::Path {
            dir: "C:\\opt".into(),
            direction: PathDir::Append,
        }]);
        assert!(out.contains("$env:PATH = \"$env:PATH;C:\\opt\""), "add: {}", out);
        assert!(out.contains("-notlike"), "guard: {}", out);
    }

    #[test]
    fn call_with_shell_placeholder() {
        let out = render(&[Node::Call {
            cmd:  "starship".into(),
            args: "init {shell}".into(),
        }]);
        assert_eq!(out, "Invoke-Expression (& starship init powershell)");
    }

    #[test]
    fn call_no_args() {
        let out = render(&[Node::Call {
            cmd:  "myprog".into(),
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
    fn cond_exists() {
        assert_eq!(
            PwshEmitter.cond(&Cond::Exists("C:\\Users\\user\\.cargo\\bin".into())),
            "Test-Path \"C:\\Users\\user\\.cargo\\bin\""
        );
    }

    #[test]
    fn cond_env() {
        assert_eq!(
            PwshEmitter.cond(&Cond::Env("CARGO_HOME".into())),
            "(Test-Path env:CARGO_HOME)"
        );
    }

    #[test]
    fn cond_os_darwin()  { assert_eq!(PwshEmitter.cond(&Cond::Os("darwin".into())),  "$IsMacOS"); }
    #[test]
    fn cond_os_linux()   { assert_eq!(PwshEmitter.cond(&Cond::Os("linux".into())),   "$IsLinux"); }
    #[test]
    fn cond_os_windows() { assert_eq!(PwshEmitter.cond(&Cond::Os("windows".into())), "$IsWindows"); }

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
            cond:  Cond::Have("git".into()),
            body:  vec![Node::Set { key: "X".into(), val: "1".into() }],
            elifs: vec![],
            else_: vec![],
        });
        let out = render(&[node]);
        assert!(out.contains("if (Get-Command git"), "missing if: {}", out);
        assert!(out.contains("$env:X = \"1\""), "missing body: {}", out);
        assert!(out.ends_with('}'), "missing close: {}", out);
    }

    #[test]
    fn if_elif_else_braces() {
        let node = Node::If(IfNode {
            cond:  Cond::Os("darwin".into()),
            body:  vec![Node::Set { key: "A".into(), val: "1".into() }],
            elifs: vec![(Cond::Os("linux".into()), vec![Node::Set { key: "A".into(), val: "2".into() }])],
            else_: vec![Node::Set { key: "A".into(), val: "3".into() }],
        });
        let out = render(&[node]);
        assert!(out.contains("elseif"), "missing elseif: {}", out);
        assert!(out.contains("} else {"), "missing else: {}", out);
    }

    #[test]
    fn indent_depth() {
        let node = Node::If(IfNode {
            cond:  Cond::Have("cargo".into()),
            body:  vec![Node::Set { key: "Y".into(), val: "z".into() }],
            elifs: vec![],
            else_: vec![],
        });
        let out = render(&[node]);
        assert!(
            out.lines().any(|l| l.starts_with("  $env:")),
            "body not indented: {}", out
        );
    }

    #[test]
    fn cond_not() {
        assert_eq!(
            PwshEmitter.cond(&Cond::Not(Box::new(Cond::Have("git".into())))),
            "(-not (Get-Command git -ErrorAction SilentlyContinue))"
        );
    }

    #[test]
    fn cond_and() {
        assert_eq!(
            PwshEmitter.cond(&Cond::And(
                Box::new(Cond::Have("cargo".into())),
                Box::new(Cond::Os("linux".into())),
            )),
            "(Get-Command cargo -ErrorAction SilentlyContinue) -and ($IsLinux)"
        );
    }

    #[test]
    fn cond_or() {
        assert_eq!(
            PwshEmitter.cond(&Cond::Or(
                Box::new(Cond::Os("darwin".into())),
                Box::new(Cond::Os("linux".into())),
            )),
            "($IsMacOS) -or ($IsLinux)"
        );
    }
}
