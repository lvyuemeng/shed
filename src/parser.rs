use crate::ast::{Cond, IfNode, Node, ParseError};

pub struct Parser {
    /// Each entry is `(source_line_number, tokens)`. Line numbers are 1-based
    /// and reflect the original file so error messages are actionable.
    lines: Vec<(usize, Vec<String>)>,
    pos: usize,
}

impl Parser {
    pub fn new(src: &str) -> Self {
        let lines = src
            .lines()
            .enumerate()
            .filter_map(|(i, raw)| {
                let s = raw.split('#').next().unwrap_or("").trim();
                if s.is_empty() {
                    None
                } else {
                    let toks = s.split_whitespace().map(String::from).collect();
                    Some((i + 1, toks))
                }
            })
            .collect();
        Self { lines, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Vec<Node>, ParseError> {
        self.block(&[])
    }

    // ── internal ────────────────────────────────────────────────────────────

    /// Return `(line_number, keyword, full_token_slice)` for the current line
    /// without advancing.
    fn peek(&self) -> Option<(usize, &[String])> {
        self.lines.get(self.pos).map(|(ln, t)| (*ln, t.as_slice()))
    }

    fn block(&mut self, stops: &[&str]) -> Result<Vec<Node>, ParseError> {
        let mut nodes = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some((_, t)) => {
                    // `.first()` is always `Some` here — `lines` never stores
                    // empty token vecs (the filter_map above guarantees this).
                    if stops.contains(&t[0].as_str()) {
                        break;
                    }
                }
            }
            let node = self.parse_statement()?;
            nodes.push(node);
        }
        Ok(nodes)
    }

    fn parse_statement(&mut self) -> Result<Node, ParseError> {
        // SAFETY: parse_statement is only called after peek() confirms a line
        // exists at self.pos, so the index is always valid.
        let (ln, toks) = &self.lines[self.pos];
        let ln = *ln;
        // SAFETY: filter_map in new() guarantees every stored line has ≥1 token.
        match toks[0].as_str() {
            "set" => {
                let key = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, "usage: set KEY VALUE"))?
                    .clone();
                if toks.len() < 3 {
                    return Err(ParseError::at(ln, "usage: set KEY VALUE"));
                }
                let val = toks[2..].join(" ");
                self.pos += 1;
                Ok(Node::Set { key, val })
            }
            "path+" => {
                let dir = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, "usage: path+ DIR"))?
                    .clone();
                self.pos += 1;
                Ok(Node::Path { dir, prepend: true })
            }
            "path-" => {
                let dir = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, "usage: path- DIR"))?
                    .clone();
                self.pos += 1;
                Ok(Node::Path {
                    dir,
                    prepend: false,
                })
            }
            "inject" => {
                let cmd = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, "usage: inject CMD [ARGS]"))?
                    .clone();
                let args = if toks.len() > 2 {
                    toks[2..].join(" ")
                } else {
                    String::new()
                };
                self.pos += 1;
                Ok(Node::Inject { cmd, args })
            }
            "if" => {
                // Borrow cond slice without allocating — parse_cond takes &[String].
                let cond_slice = toks
                    .get(1..)
                    .filter(|s| s.len() >= 2)
                    .ok_or_else(|| ParseError::at(ln, "usage: if <cond-type> <value>"))?;
                let cond = Self::parse_cond(ln, cond_slice)?;
                self.pos += 1;
                Ok(Node::If(self.parse_if(ln, cond)?))
            }
            kw => Err(ParseError::at(ln, format!("unknown keyword {:?}", kw))),
        }
    }

    fn parse_cond(ln: usize, toks: &[String]) -> Result<Cond, ParseError> {
        let kind = toks
            .first()
            .ok_or_else(|| ParseError::at(ln, "condition requires a type"))?;
        let val = toks
            .get(1)
            .cloned()
            .ok_or_else(|| ParseError::at(ln, format!("{} requires a value", kind)))?;
        match kind.as_str() {
            "have" => Ok(Cond::Have(val)),
            "os" => Ok(Cond::Os(val)),
            "shell" => Ok(Cond::Shell(val)),
            other => Err(ParseError::at(
                ln,
                format!("unknown condition {:?} — use: have | os | shell", other),
            )),
        }
    }

    fn parse_if(&mut self, if_ln: usize, cond: Cond) -> Result<IfNode, ParseError> {
        let mut node = IfNode {
            cond,
            body: self.block(&["elif", "else", "end"])?,
            elifs: Vec::new(),
            else_: Vec::new(),
        };

        // Flat while-let: consume elif*/else?/end without deep nesting.
        while let Some((ln, kw)) = self.peek().map(|(ln, t)| (ln, t[0].clone())) {
            match kw.as_str() {
                "elif" => {
                    // Borrow cond tokens directly from the stored line; no clone needed.
                    // SAFETY: self.pos is valid — peek() returned Some above.
                    let cond = {
                        let toks = &self.lines[self.pos].1;
                        let cond_slice = toks
                            .get(1..)
                            .filter(|s| s.len() >= 2)
                            .ok_or_else(|| ParseError::at(ln, "elif requires a condition"))?;
                        Self::parse_cond(ln, cond_slice)?
                    };
                    self.pos += 1;
                    let b = self.block(&["elif", "else", "end"])?;
                    node.elifs.push((cond, b));
                }
                "else" => {
                    self.pos += 1;
                    node.else_ = self.block(&["end"])?;
                }
                "end" => {
                    self.pos += 1;
                    break;
                }
                kw => {
                    return Err(ParseError::at(
                        ln,
                        format!("unexpected {:?} inside if-block", kw),
                    ));
                }
            }
        }

        // If the while exited without consuming "end" the block is unterminated.
        if self
            .lines
            .get(self.pos.saturating_sub(1))
            .map(|(_, t)| t[0].as_str() != "end")
            .unwrap_or(true)
            && self.peek().is_none()
        {
            return Err(ParseError::at(
                if_ln,
                "unterminated if-block (missing 'end')",
            ));
        }

        Ok(node)
    }
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Cond, Node};

    fn parse(src: &str) -> Result<Vec<Node>, ParseError> {
        Parser::new(src).parse()
    }

    // ── happy-path structural checks ─────────────────────────────────────────

    #[test]
    fn set_parses_key_and_value() {
        let nodes = parse("set EDITOR nvim").unwrap();
        match &nodes[0] {
            Node::Set { key, val } => {
                assert_eq!(key, "EDITOR");
                assert_eq!(val, "nvim");
            }
            n => panic!("{:?}", n),
        }
    }

    #[test]
    fn set_multiword_value() {
        let nodes = parse("set GREETING hello world").unwrap();
        match &nodes[0] {
            Node::Set { val, .. } => assert_eq!(val, "hello world"),
            n => panic!("{:?}", n),
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
            n => panic!("{:?}", n),
        }
    }

    #[test]
    fn if_elif_else_end_structure() {
        let src = "if os darwin\nset A 1\nelif os linux\nset A 2\nelse\nset A 3\nend";
        let nodes = parse(src).unwrap();
        match &nodes[0] {
            Node::If(n) => {
                assert_eq!(n.body.len(), 1);
                assert_eq!(n.elifs.len(), 1);
                assert_eq!(n.else_.len(), 1);
                match &n.cond {
                    Cond::Os(s) => assert_eq!(s, "darwin"),
                    c => panic!("{:?}", c),
                }
            }
            n => panic!("{:?}", n),
        }
    }

    #[test]
    fn comments_and_blanks_ignored() {
        let nodes = parse("# comment\n\nset A B # inline").unwrap();
        assert_eq!(nodes.len(), 1);
    }

    // ── error path: line numbers ──────────────────────────────────────────────

    #[test]
    fn errors_carry_line_number() {
        let err = parse("set A 1\nsett FOO bar").unwrap_err();
        assert_eq!(err.line, 2, "wrong line: {}", err);
        assert!(err.msg.contains("sett"), "wrong msg: {}", err);
    }

    #[test]
    fn unterminated_if_reports_opening_line() {
        let err = parse("set A 1\nif have git\nset B 2").unwrap_err();
        assert_eq!(err.line, 2, "should point to the if line: {}", err);
    }

    #[test]
    fn missing_args_errors() {
        assert!(parse("set").is_err());
        assert!(parse("set ONLY").is_err());
        assert!(parse("path+").is_err());
        assert!(parse("inject").is_err());
        assert!(parse("if have").is_err()); // missing value
        assert!(parse("if foobar baz\nend").is_err()); // unknown cond
    }

    #[test]
    fn nested_if_parses() {
        let src = "if have cargo\nif os darwin\nset A 1\nend\nend";
        let nodes = parse(src).unwrap();
        match &nodes[0] {
            Node::If(outer) => match &outer.body[0] {
                Node::If(_) => {}
                n => panic!("expected nested if: {:?}", n),
            },
            n => panic!("{:?}", n),
        }
    }
}
