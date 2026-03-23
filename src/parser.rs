use std::{
    borrow::Cow,
    env,
    path::{Path, PathBuf},
};

use crate::ast::{Cond, IfNode, Node, ParseError, PathDir};

pub struct Parser {
    /// Each entry is `(source_line_number, tokens)`. Line numbers are 1-based
    /// and reflect the original file so error messages are actionable.
    lines: Vec<(usize, Vec<String>)>,
    pos: usize,
    /// Anchor directory used to resolve relative `path+` / `path-` tokens.
    /// `None` when reading from stdin (no meaningful anchor).
    base: Option<PathBuf>,
}

/// Strip a single layer of surrounding `"…"` or `'…'` from `s`.
/// Only acts when the *entire* token is wrapped; partial quotes are left alone.
/// Returns a borrowed slice when no stripping is needed (zero allocation).
fn strip_quotes(s: &str) -> Cow<'_, str> {
    let b = s.as_bytes();
    let quoted = b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"')
            || (b[0] == b'\'' && b[b.len() - 1] == b'\''));
    if quoted {
        Cow::Owned(s[1..s.len() - 1].to_owned())
    } else {
        Cow::Borrowed(s)
    }
}

/// Join tokens from index `from` onward with spaces.
/// Returns `None` when the resulting slice is empty.
fn tail_joined(toks: &[String], from: usize) -> Option<String> {
    let rest = toks.get(from..)?;
    if rest.is_empty() { None } else { Some(rest.join(" ")) }
}

/// Find the rightmost position of `op` in `toks` satisfying the
/// left-associative binary-operator constraint:
///   - at least two tokens to the left  (`i >= 2`)
///   - at least one token to the right  (`i + 1 < toks.len()`)
/// The `i >= 2` requirement ensures there are enough tokens on the left
/// for a valid leaf condition (type + value).
fn last_op_pos(toks: &[String], op: &str) -> Option<usize> {
    toks.iter()
        .enumerate()
        .rev()
        .find_map(|(i, t)| (t == op && i >= 2 && i + 1 < toks.len()).then_some(i))
}

/// Resolve a path token from a shed source file.
///
/// Rules (applied in order):
/// 1. `~` prefix → expand to `$HOME` (Unix) or `$USERPROFILE` (Windows).
///    If neither variable is set the `~` is left as-is.
/// 2. Relative path → join onto `base` (the shed file's directory).
///    When `base` is `None` (stdin) relative paths are kept as-is.
/// 3. Absolute path → returned unchanged.
///
/// No I/O is performed; the resolved path need not exist.
pub fn resolve_path(raw: &str, base: Option<&Path>) -> String {
    let expanded: PathBuf = if let Some(rest) = raw.strip_prefix('~') {
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_default();
        if home.is_empty() {
            return raw.to_owned();
        }
        PathBuf::from(home).join(rest.trim_start_matches('/'))
    } else {
        PathBuf::from(raw)
    };

    if expanded.is_relative() {
        base.map(|b| b.join(&expanded))
            .unwrap_or(expanded)
            .to_string_lossy()
            .into_owned()
    } else {
        expanded.to_string_lossy().into_owned()
    }
}

impl Parser {
    /// Construct a parser for `src`.
    ///
    /// `base` is the directory of the shed source file, used to resolve
    /// relative and home-prefixed `path+` / `path-` tokens at parse time.
    /// Pass `None` when reading from stdin (no anchor directory).
    pub fn new(src: &str, base: Option<PathBuf>) -> Self {
        let lines = src
            .lines()
            .enumerate()
            .filter_map(|(i, raw)| {
                // SAFETY: split('#') always yields at least one element.
                let s = raw.split('#').next().unwrap_or("").trim();
                if s.is_empty() {
                    None
                } else {
                    // Reserve a small but reasonable capacity; most lines have
                    // 2–4 tokens so this avoids the first 1–2 realloc cycles.
                    let mut toks = Vec::with_capacity(4);
                    toks.extend(s.split_whitespace().map(String::from));
                    Some((i + 1, toks))
                }
            })
            .collect();
        Self { lines, pos: 0, base }
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
                // Need at least: set KEY VALUE  (3 tokens).
                let key = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, "usage: set KEY VALUE"))?
                    .clone();
                let val = tail_joined(toks, 2)
                    .ok_or_else(|| ParseError::at(ln, "usage: set KEY VALUE"))?;
                self.pos += 1;
                Ok(Node::Set { key, val })
            }
            kw @ ("path+" | "path-") => {
                let direction = if kw == "path+" { PathDir::Prepend } else { PathDir::Append };
                let raw = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, format!("usage: {} DIR", kw)))
                    .map(|s| strip_quotes(s).into_owned())?;
                // Resolve home-dir expansion and relative paths at parse time
                // so the rest of the pipeline (prune, emit) works with final paths.
                let dir = resolve_path(&raw, self.base.as_deref());
                self.pos += 1;
                Ok(Node::Path { dir, direction })
            }
            "call" => {
                let cmd = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, "usage: call CMD [ARGS]"))
                    .map(|s| strip_quotes(s).into_owned())?;
                let args = tail_joined(toks, 2).unwrap_or_default();
                self.pos += 1;
                Ok(Node::Call { cmd, args })
            }
            "alias" => {
                let name = toks
                    .get(1)
                    .ok_or_else(|| ParseError::at(ln, "usage: alias NAME BODY"))?
                    .clone();
                let body = tail_joined(toks, 2)
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
    /// Splits at the LAST `or`; left subtree recurses through `parse_or`,
    /// right side is parsed by `parse_and`.
    ///
    /// Example: `a or b or c`
    ///   → Or(parse_or("a or b"), parse_and("c"))
    ///   → Or(Or(a,b), c)  — left-associative
    fn parse_or(ln: usize, toks: &[String]) -> Result<Cond, ParseError> {
        if let Some(op) = last_op_pos(toks, "or") {
            let left  = Self::parse_or(ln, &toks[..op])?;
            let right = Self::parse_and(ln, &toks[op + 1..])?;
            return Ok(Cond::Or(Box::new(left), Box::new(right)));
        }
        Self::parse_and(ln, toks)
    }

    /// Medium precedence: `and`. Left-associative.
    ///
    /// Splits at the LAST `and`; left subtree recurses through `parse_and`,
    /// right side is parsed by `parse_not`.
    ///
    /// Example: `a and b and c`
    ///   → And(parse_and("a and b"), parse_not("c"))
    ///   → And(And(a,b), c)  — left-associative
    fn parse_and(ln: usize, toks: &[String]) -> Result<Cond, ParseError> {
        if let Some(op) = last_op_pos(toks, "and") {
            let left  = Self::parse_and(ln, &toks[..op])?;
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
            .map(|s| strip_quotes(s).into_owned())
            .ok_or_else(|| ParseError::at(ln, format!("{} requires a value", kind)))?;
        match kind.as_str() {
            "have"   => Ok(Cond::Have(val)),
            "exists" => Ok(Cond::Exists(val)),
            "env"    => Ok(Cond::Env(val)),
            "os"     => Ok(Cond::Os(val)),
            "shell"  => Ok(Cond::Shell(val)),
            other    => Err(ParseError::at(
                ln,
                format!(
                    "unknown condition {:?} -- use: have | exists | env | os | shell",
                    other
                ),
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
                    // SAFETY: peek() returned Some above; self.pos is valid.
                    let cond_slice = self.lines[self.pos]
                        .1
                        .get(1..)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| ParseError::at(ln, "elif requires a condition"))?;
                    let cond = Self::parse_cond(ln, cond_slice)?;
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
        Parser::new(src, None).parse()
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
