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

    /// Return `(line_number, token_slice)` for the current line without advancing.
    fn peek(&self) -> Option<(usize, &[String])> {
        self.lines.get(self.pos).map(|(ln, t)| (*ln, t.as_slice()))
    }

    fn block(&mut self, stops: &[&str]) -> Result<Vec<Node>, ParseError> {
        let mut nodes = Vec::new();
        // filter_map in new() guarantees every stored line has >=1 token,
        // so t[0] is always safe to index inside the loop body.
        while let Some((_, t)) = self.peek() {
            if stops.contains(&t[0].as_str()) {
                break;
            }
            nodes.push(self.parse_statement()?);
        }
        Ok(nodes)
    }

    fn parse_statement(&mut self) -> Result<Node, ParseError> {
        // SAFETY: only called after peek() confirms a line exists at self.pos.
        let (ln, toks) = &self.lines[self.pos];
        let ln = *ln;
        // SAFETY: filter_map in new() guarantees every stored line has >=1 token.
        match toks[0].as_str() {
            "set" => {
                if toks.len() < 3 {
                    return Err(ParseError::at(ln, "usage: set KEY VALUE"));
                }
                let key = toks[1].clone();
                let val = toks[2..].join(" ");
                self.pos += 1;
                Ok(Node::Set { key, val })
            }
            kw @ ("path+" | "path-") => {
                let prepend = kw == "path+";
                let dir = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, format!("usage: {} DIR", kw)))?
                    .clone();
                self.pos += 1;
                Ok(Node::Path { dir, prepend })
            }
            "call" => {
                let cmd = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, "usage: call CMD [ARGS]"))?
                    .clone();
                let args = toks
                    .get(2..)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.join(" "))
                    .unwrap_or_default();
                self.pos += 1;
                Ok(Node::Call { cmd, args })
            }
            "alias" => {
                let name = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, "usage: alias NAME BODY"))?
                    .clone();
                let body = toks
                    .get(2..)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.join(" "))
                    .ok_or_else(|| ParseError::at(ln, "usage: alias NAME BODY"))?;
                self.pos += 1;
                Ok(Node::Alias { name, body })
            }
            "if" => {
                // toks[1..] is the condition token list; must be non-empty.
                let cond_slice = toks
                    .get(1..)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| ParseError::at(ln, "usage: if <cond-type> <value>"))?;
                let cond = Self::parse_cond(ln, cond_slice)?;
                self.pos += 1;
                Ok(Node::If(self.parse_if(ln, cond)?))
            }
            kw => Err(ParseError::at(ln, format!("unknown keyword {:?}", kw))),
        }
    }

    fn parse_cond(ln: usize, toks: &[String]) -> Result<Cond, ParseError> {
        Self::parse_or(ln, toks)
    }

    /// Lowest precedence: `or`. Left-associative.
    ///
    /// Left-associativity requires splitting at the LAST `or` (rightmost
    /// operator at this precedence level), then recursing `parse_or` on
    /// the LEFT subtree. The right side is parsed by `parse_and`.
    ///
    /// Example: `a or b or c`
    ///   last `or` at position of second `or`
    ///   → Or(parse_or("a or b"), parse_and("c"))
    ///   → Or(Or(a,b), c)  -- left-associative
    fn parse_or(ln: usize, toks: &[String]) -> Result<Cond, ParseError> {
        if let Some(op) = toks
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, t)| (t == "or" && i >= 2 && i + 1 < toks.len()).then_some(i))
        {
            let left = Self::parse_or(ln, &toks[..op])?;
            let right = Self::parse_and(ln, &toks[op + 1..])?;
            return Ok(Cond::Or(Box::new(left), Box::new(right)));
        }
        Self::parse_and(ln, toks)
    }

    /// Medium precedence: `and`. Left-associative.
    ///
    /// Same strategy: split at the LAST `and`, recurse `parse_and` on the
    /// left, delegate the right to `parse_not`.
    ///
    /// Example: `a and b and c`
    ///   last `and` at position of second `and`
    ///   → And(parse_and("a and b"), parse_not("c"))
    ///   → And(And(a,b), c)  -- left-associative
    fn parse_and(ln: usize, toks: &[String]) -> Result<Cond, ParseError> {
        if let Some(op) = toks
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, t)| (t == "and" && i >= 2 && i + 1 < toks.len()).then_some(i))
        {
            let left = Self::parse_and(ln, &toks[..op])?;
            let right = Self::parse_not(ln, &toks[op + 1..])?;
            return Ok(Cond::And(Box::new(left), Box::new(right)));
        }
        Self::parse_not(ln, toks)
    }

    /// Highest precedence: prefix `not`. Right-associative (naturally).
    fn parse_not(ln: usize, toks: &[String]) -> Result<Cond, ParseError> {
        if toks.first().map(|s| s.as_str()) == Some("not") {
            let rest = toks
                .get(1..)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ParseError::at(ln, "'not' requires a condition"))?;
            return Ok(Cond::Not(Box::new(Self::parse_not(ln, rest)?)));
        }
        Self::parse_leaf(ln, toks)
    }

    /// Parse a leaf condition: `<type> <value>`.
    fn parse_leaf(ln: usize, toks: &[String]) -> Result<Cond, ParseError> {
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
                format!("unknown condition {:?} -- use: have | os | shell", other),
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

        // Flat loop: consume elif*/else?/end; return Ok on `end`, error on EOF.
        loop {
            // SAFETY: new() guarantees every stored line has >=1 token.
            match self
                .peek()
                .and_then(|(ln, t)| t.first().map(|kw| (ln, kw.clone())))
            {
                None => break,
                Some((_, kw)) if kw == "end" => {
                    self.pos += 1;
                    return Ok(node);
                }
                Some((ln, kw)) if kw == "elif" => {
                    // SAFETY: peek() returned Some above.
                    let cond = {
                        let toks = &self.lines[self.pos].1;
                        let cond_slice = toks
                            .get(1..)
                            .filter(|s| !s.is_empty())
                            .ok_or_else(|| ParseError::at(ln, "elif requires a condition"))?;
                        Self::parse_cond(ln, cond_slice)?
                    };
                    self.pos += 1;
                    let b = self.block(&["elif", "else", "end"])?;
                    node.elifs.push((cond, b));
                }
                Some((_, kw)) if kw == "else" => {
                    self.pos += 1;
                    node.else_ = self.block(&["end"])?;
                }
                Some((ln, kw)) => {
                    return Err(ParseError::at(
                        ln,
                        format!("unexpected {:?} inside if-block", kw),
                    ));
                }
            }
        }

        Err(ParseError::at(
            if_ln,
            "unterminated if-block (missing 'end')",
        ))
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
    fn call_no_args() {
        let nodes = parse("call myprog").unwrap();
        match &nodes[0] {
            Node::Call { cmd, args } => {
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
        assert!(parse("call").is_err());
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

    // -- compound condition precedence ----------------------------------------

    /// not have cargo  →  Not(Have("cargo"))
    #[test]
    fn not_leaf() {
        let nodes = parse("if not have cargo\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => {
                assert!(matches!(&n.cond, Cond::Not(c) if matches!(c.as_ref(), Cond::Have(_))))
            }
            n => panic!("{:?}", n),
        }
    }

    /// not have cargo and os linux  →  And(Not(Have), Os)  -- not binds tighter than and
    #[test]
    fn not_and_precedence() {
        let nodes = parse("if not have cargo and os linux\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => match &n.cond {
                Cond::And(l, r) => {
                    assert!(matches!(l.as_ref(), Cond::Not(_)), "lhs should be Not");
                    assert!(matches!(r.as_ref(), Cond::Os(_)), "rhs should be Os");
                }
                c => panic!("expected And, got {:?}", c),
            },
            n => panic!("{:?}", n),
        }
    }

    /// have cargo or not shell fish  →  Or(Have, Not(Shell))  -- not binds tighter than or
    #[test]
    fn or_not_precedence() {
        let nodes = parse("if have cargo or not shell fish\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => match &n.cond {
                Cond::Or(l, r) => {
                    assert!(matches!(l.as_ref(), Cond::Have(_)), "lhs should be Have");
                    assert!(matches!(r.as_ref(), Cond::Not(_)), "rhs should be Not");
                }
                c => panic!("expected Or, got {:?}", c),
            },
            n => panic!("{:?}", n),
        }
    }

    /// have cargo and os linux or shell bash
    ///   →  Or(And(Have, Os), Shell)  -- and binds tighter than or
    #[test]
    fn and_or_precedence() {
        let nodes = parse("if have cargo and os linux or shell bash\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => match &n.cond {
                Cond::Or(l, r) => {
                    assert!(matches!(l.as_ref(), Cond::And(_, _)), "lhs should be And");
                    assert!(matches!(r.as_ref(), Cond::Shell(_)), "rhs should be Shell");
                }
                c => panic!("expected Or(And, Shell), got {:?}", c),
            },
            n => panic!("{:?}", n),
        }
    }

    /// os linux or have cargo and shell bash
    ///   →  Or(Os, And(Have, Shell))  -- and on the right still binds first
    #[test]
    fn or_and_right_precedence() {
        let nodes = parse("if os linux or have cargo and shell bash\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => match &n.cond {
                Cond::Or(l, r) => {
                    assert!(matches!(l.as_ref(), Cond::Os(_)), "lhs should be Os");
                    assert!(matches!(r.as_ref(), Cond::And(_, _)), "rhs should be And");
                }
                c => panic!("expected Or(Os, And), got {:?}", c),
            },
            n => panic!("{:?}", n),
        }
    }

    /// have cargo and os linux and shell bash
    ///   →  And(And(Have, Os), Shell)  -- left-associative chaining of and
    #[test]
    fn and_left_associative() {
        let nodes = parse("if have cargo and os linux and shell bash\nend").unwrap();
        match &nodes[0] {
            Node::If(n) => match &n.cond {
                Cond::And(l, r) => {
                    assert!(
                        matches!(l.as_ref(), Cond::And(_, _)),
                        "outer lhs should be And(And)"
                    );
                    assert!(
                        matches!(r.as_ref(), Cond::Shell(_)),
                        "outer rhs should be Shell"
                    );
                }
                c => panic!("expected And(And, Shell), got {:?}", c),
            },
            n => panic!("{:?}", n),
        }
    }
}
