use crate::ast::{Cond, IfNode, Node};

pub struct Parser {
    lines: Vec<Vec<String>>,
    pos:   usize,
}

impl Parser {
    pub fn new(src: &str) -> Self {
        let lines = src
            .lines()
            .filter_map(|raw| {
                // strip inline comments, trim, skip blanks
                let s = raw.split('#').next().unwrap_or("").trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s.split_whitespace().map(String::from).collect::<Vec<_>>())
                }
            })
            .collect();
        Self { lines, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Vec<Node>, String> {
        self.block(&[])
    }

    // ── internal ────────────────────────────────────────────────────────────

    fn peek(&self) -> Option<&Vec<String>> {
        self.lines.get(self.pos)
    }

    fn take(&mut self) -> Vec<String> {
        let t = self.lines[self.pos].clone();
        self.pos += 1;
        t
    }

    fn block(&mut self, stops: &[&str]) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some(t) if stops.contains(&t[0].as_str()) => break,
                _ => {}
            }
            let t = self.take();
            let node = match t[0].as_str() {
                "set" => {
                    require_len(&t, 3, "set KEY VALUE")?;
                    Node::Set { key: t[1].clone(), val: t[2..].join(" ") }
                }
                "path+" => {
                    require_len(&t, 2, "path+ DIR")?;
                    Node::Path { dir: t[1].clone(), prepend: true }
                }
                "path-" => {
                    require_len(&t, 2, "path- DIR")?;
                    Node::Path { dir: t[1].clone(), prepend: false }
                }
                "inject" => {
                    require_len(&t, 2, "inject CMD [ARGS]")?;
                    Node::Inject { cmd: t[1].clone(), args: t[2..].join(" ") }
                }
                "if" => {
                    require_len(&t, 3, "if <cond-type> <value>")?;
                    Node::If(self.parse_if(&t[1..])?)
                }
                kw => return Err(format!("unknown keyword {:?}", kw)),
            };
            nodes.push(node);
        }
        Ok(nodes)
    }

    fn parse_cond(toks: &[String]) -> Result<Cond, String> {
        let val = || {
            toks.get(1)
                .cloned()
                .ok_or_else(|| format!("{} requires a value", toks[0]))
        };
        match toks[0].as_str() {
            "have"  => Ok(Cond::Have(val()?)),
            "os"    => Ok(Cond::Os(val()?)),
            "shell" => Ok(Cond::Shell(val()?)),
            other   => Err(format!("unknown condition {:?} — use: have | os | shell", other)),
        }
    }

    fn parse_if(&mut self, cond_toks: &[String]) -> Result<IfNode, String> {
        let mut node = IfNode {
            cond:  Self::parse_cond(cond_toks)?,
            body:  self.block(&["elif", "else", "end"])?,
            elifs: Vec::new(),
            else_: Vec::new(),
        };
        loop {
            match self.peek().map(|v| v[0].as_str()) {
                Some("elif") => {
                    let t = self.take();
                    if t.len() < 3 { return Err("elif requires a condition".into()); }
                    let c = Self::parse_cond(&t[1..])?;
                    let b = self.block(&["elif", "else", "end"])?;
                    node.elifs.push((c, b));
                }
                Some("else") => {
                    self.take();
                    node.else_ = self.block(&["end"])?;
                }
                Some("end") => { self.take(); break; }
                Some(kw)   => return Err(format!("unexpected {:?} inside if-block", kw)),
                None       => return Err("unterminated if-block (missing 'end')".into()),
            }
        }
        Ok(node)
    }
}

fn require_len(t: &[String], min: usize, usage: &str) -> Result<(), String> {
    if t.len() < min {
        Err(format!("usage: {}", usage))
    } else {
        Ok(())
    }
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Cond, Node};

    fn parse(src: &str) -> Result<Vec<Node>, String> {
        Parser::new(src).parse()
    }

    // ── set ─────────────────────────────────────────────────────────────────

    #[test]
    fn set_simple() {
        let nodes = parse("set EDITOR nvim").unwrap();
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Node::Set { key, val } => {
                assert_eq!(key, "EDITOR");
                assert_eq!(val, "nvim");
            }
            other => panic!("expected Set, got {:?}", other),
        }
    }

    #[test]
    fn set_multi_word_value() {
        let nodes = parse("set GREETING hello world").unwrap();
        match &nodes[0] {
            Node::Set { val, .. } => assert_eq!(val, "hello world"),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn set_missing_value_errors() {
        assert!(parse("set EDITOR").is_err());
    }

    #[test]
    fn set_missing_key_errors() {
        assert!(parse("set").is_err());
    }

    // ── path ────────────────────────────────────────────────────────────────

    #[test]
    fn path_prepend() {
        let nodes = parse("path+ /usr/local/bin").unwrap();
        match &nodes[0] {
            Node::Path { dir, prepend } => {
                assert_eq!(dir, "/usr/local/bin");
                assert!(*prepend);
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn path_append() {
        let nodes = parse("path- /opt/bin").unwrap();
        match &nodes[0] {
            Node::Path { prepend, .. } => assert!(!*prepend),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn path_missing_dir_errors() {
        assert!(parse("path+").is_err());
        assert!(parse("path-").is_err());
    }

    // ── inject ──────────────────────────────────────────────────────────────

    #[test]
    fn inject_with_args() {
        let nodes = parse("inject starship init {shell}").unwrap();
        match &nodes[0] {
            Node::Inject { cmd, args } => {
                assert_eq!(cmd, "starship");
                assert_eq!(args, "init {shell}");
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn inject_no_args() {
        let nodes = parse("inject myprog").unwrap();
        match &nodes[0] {
            Node::Inject { cmd, args } => {
                assert_eq!(cmd, "myprog");
                assert_eq!(args, "");
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn inject_missing_cmd_errors() {
        assert!(parse("inject").is_err());
    }

    // ── comments and blank lines ─────────────────────────────────────────────

    #[test]
    fn comments_stripped() {
        let nodes = parse("# full-line comment\nset A B # inline comment").unwrap();
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Node::Set { key, val } => {
                assert_eq!(key, "A");
                assert_eq!(val, "B");
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn blank_lines_ignored() {
        let nodes = parse("\n\nset X Y\n\n").unwrap();
        assert_eq!(nodes.len(), 1);
    }

    // ── unknown keyword ──────────────────────────────────────────────────────

    #[test]
    fn unknown_keyword_errors() {
        let err = parse("sett FOO bar").unwrap_err();
        assert!(err.contains("sett"), "error should mention keyword: {}", err);
    }

    // ── conditions ──────────────────────────────────────────────────────────

    #[test]
    fn cond_have() {
        let nodes = parse("if have cargo\nset C 1\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => match &n.cond {
                Cond::Have(cmd) => assert_eq!(cmd, "cargo"),
                other => panic!("{:?}", other),
            },
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn cond_os() {
        let nodes = parse("if os darwin\nset A B\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => match &n.cond {
                Cond::Os(name) => assert_eq!(name, "darwin"),
                other => panic!("{:?}", other),
            },
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn cond_shell() {
        let nodes = parse("if shell fish\nset A B\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => match &n.cond {
                Cond::Shell(name) => assert_eq!(name, "fish"),
                other => panic!("{:?}", other),
            },
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn cond_unknown_errors() {
        let err = parse("if foobar baz\nend").unwrap_err();
        assert!(err.contains("foobar"), "error should mention cond: {}", err);
    }

    // ── if / elif / else / end ───────────────────────────────────────────────

    #[test]
    fn if_body_parsed() {
        let nodes = parse("if have git\nset GIT 1\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => assert_eq!(n.body.len(), 1),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn if_elif_else_end() {
        let src = "if os darwin\nset A 1\nelif os linux\nset A 2\nelse\nset A 3\nend";
        let nodes = parse(src).unwrap();
        match &nodes[0] {
            Node::If(n) => {
                assert_eq!(n.body.len(),  1);
                assert_eq!(n.elifs.len(), 1);
                assert_eq!(n.else_.len(), 1);
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn if_missing_end_errors() {
        let err = parse("if have git\nset A 1").unwrap_err();
        assert!(err.contains("end") || err.contains("unterminated"),
            "error should mention missing end: {}", err);
    }

    #[test]
    fn if_missing_cond_value_errors() {
        assert!(parse("if have\nend").is_err());
    }

    #[test]
    fn elif_missing_cond_errors() {
        // only keyword, no cond type
        assert!(parse("if have git\nelif\nend").is_err());
    }

    #[test]
    fn nested_if() {
        let src = "if have cargo\nif os darwin\nset A 1\nend\nend";
        let nodes = parse(src).unwrap();
        match &nodes[0] {
            Node::If(outer) => {
                assert_eq!(outer.body.len(), 1);
                match &outer.body[0] {
                    Node::If(_) => {}
                    other => panic!("expected nested If, got {:?}", other),
                }
            }
            other => panic!("{:?}", other),
        }
    }

    // ── multiple top-level nodes ─────────────────────────────────────────────

    #[test]
    fn multiple_nodes() {
        let src = "set A 1\npath+ /bin\ninject foo";
        let nodes = parse(src).unwrap();
        assert_eq!(nodes.len(), 3);
    }

    #[test]
    fn empty_source_gives_empty_ast() {
        let nodes = parse("").unwrap();
        assert!(nodes.is_empty());
    }
}