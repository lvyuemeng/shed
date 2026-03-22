pub mod bash;
pub mod fish;
pub mod pwsh;

use std::borrow::Cow;

use crate::ast::Node;

pub trait Emitter {
    /// The canonical shell name (e.g. "bash", "fish").
    /// Used by concrete emitters for `{shell}` substitution in inject args.
    #[allow(dead_code)]
    fn name(&self) -> &str;

    /// Emit `nodes` at the given indent `depth`, returning one `String` per output line.
    fn emit_nodes(&self, nodes: &[Node], depth: usize) -> Vec<String>;

    /// Render the full node list to a newline-joined `String`.
    /// Pre-allocates the output buffer to avoid repeated reallocations.
    fn render(&self, nodes: &[Node]) -> String {
        let lines = self.emit_nodes(nodes, 0);
        let cap: usize = lines.iter().map(|l| l.len() + 1).sum();
        let mut out = String::with_capacity(cap);
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(line);
        }
        out
    }

    /// Return `s` prefixed by `depth * 2` spaces.
    /// Returns `Cow::Borrowed` at depth 0 to avoid allocation.
    fn indent<'a>(&self, s: impl Into<Cow<'a, str>>, depth: usize) -> String {
        let s = s.into();
        if depth == 0 {
            s.into_owned()
        } else {
            let prefix = "  ".repeat(depth);
            let mut buf = String::with_capacity(prefix.len() + s.len());
            buf.push_str(&prefix);
            buf.push_str(&s);
            buf
        }
    }
}
