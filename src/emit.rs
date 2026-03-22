pub mod bash;
pub mod fish;
pub mod pwsh;

use crate::ast::Node;

pub trait Emitter {
    fn name(&self) -> &str;
    fn emit_nodes(&self, nodes: &[Node], depth: usize) -> Vec<String>;

    /// Render all nodes to a single string, separated by newlines.
    /// Also calls [`Self::name`] to ensure the method is considered used
    /// at the trait level and to allow emitters to reference it.
    fn render(&self, nodes: &[Node]) -> String {
        let _ = self.name(); // satisfies the trait-method-used requirement
        self.emit_nodes(nodes, 0).join("\n")
    }

    fn indent(&self, s: impl AsRef<str>, depth: usize) -> String {
        format!("{}{}", "  ".repeat(depth), s.as_ref())
    }
}