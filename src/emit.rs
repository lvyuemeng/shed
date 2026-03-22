pub mod bash;
pub mod fish;
pub mod pwsh;

use crate::ast::Node;

pub trait Emitter {
    /// The canonical shell name (e.g. "bash", "fish").
    /// Used by concrete emitters for `{shell}` substitution in call args.
    fn name(&self) -> &str;

    /// Emit `nodes` at the given indent `depth`, returning one `String` per output line.
    fn emit_nodes(&self, nodes: &[Node], depth: usize) -> Vec<String>;

    /// Render the full node list to a newline-joined `String`.
    fn render(&self, nodes: &[Node]) -> String {
        self.emit_nodes(nodes, 0).join("\n")
    }

    /// Return `s` prefixed by `depth * 2` spaces.
    fn indent(&self, s: String, depth: usize) -> String {
        if depth == 0 {
            s
        } else {
            let mut buf = "  ".repeat(depth);
            buf.push_str(&s);
            buf
        }
    }
}
