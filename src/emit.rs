pub mod bash;
pub mod fish;
pub mod pwsh;

use crate::ast::Node;

/// Return `s` prefixed by `depth * 2` spaces.
///
/// Identity law:  `indent(s, 0) == s`
/// Composition:   `indent(indent(s, a), b) == indent(s, a + b)`
///
/// `#[inline]` allows LLVM to see the depth == 0 fast-path at call sites and
/// eliminate the allocation entirely for the common top-level case.
#[inline]
pub fn indent(s: String, depth: usize) -> String {
    if depth == 0 {
        return s;
    }
    // Pre-allocate exactly `depth*2 + s.len()` bytes — one heap allocation.
    let pad = depth * 2;
    let mut buf = String::with_capacity(pad + s.len());
    for _ in 0..pad {
        buf.push(' ');
    }
    buf.push_str(&s);
    buf
}

pub trait Emitter {
    /// The canonical shell name (e.g. "bash", "fish").
    /// Used by concrete emitters for `{shell}` substitution in call args.
    fn name(&self) -> &str;

    /// Emit `nodes` at the given indent `depth`, returning one `String` per output line.
    fn emit_nodes(&self, nodes: &[Node], depth: usize) -> Vec<String>;

    /// Render the full node list to a newline-joined `String`.
    ///
    /// Pre-allocates a reasonable output buffer; the `join` still allocates once
    /// but avoids repeated `push_str` reallocations.
    fn render(&self, nodes: &[Node]) -> String {
        self.emit_nodes(nodes, 0).join("\n")
    }

    /// Expand `{shell}` in `args` with the target shell name, then trim.
    /// Returns owned `String`; the trim avoids a trailing newline in output.
    #[inline]
    fn resolve_call_args(&self, args: &str) -> String {
        args.replace("{shell}", self.name()).trim().to_owned()
    }

    /// Return `s` prefixed by `depth * 2` spaces (delegates to the free function).
    #[inline]
    fn indent(&self, s: String, depth: usize) -> String {
        indent(s, depth)
    }
}
