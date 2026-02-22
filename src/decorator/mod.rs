pub mod redaction;
pub mod source_tag;

use async_trait::async_trait;
use serde_json::Value;

/// Error from a decorator or transform. Non-fatal — the raw output still goes through.
#[derive(Debug, thiserror::Error)]
pub enum DecoratorError {
    #[error("{0}")]
    Failed(String),
}

/// A transform mutates tool output in-place. Runs before decorators.
/// Use for security (redaction, tagging) or normalization.
#[async_trait]
pub trait ToolTransform: Send + Sync {
    fn name(&self) -> &str;

    fn applies_to(&self, tool_name: &str, input: &Value) -> bool;

    /// Transform the output. Receives ownership, returns the modified version.
    async fn transform(
        &self,
        tool_name: &str,
        input: &Value,
        output: String,
    ) -> Result<String, DecoratorError>;
}

/// A decorator enriches tool output with additional context appended after it.
/// Decorators are advisory — if one fails, the output is unchanged.
#[async_trait]
pub trait ToolDecorator: Send + Sync {
    fn name(&self) -> &str;

    fn applies_to(&self, tool_name: &str, input: &Value) -> bool;

    /// Return additional context to append. `Ok(None)` means nothing to add.
    async fn decorate(
        &self,
        tool_name: &str,
        input: &Value,
        output: &str,
    ) -> Result<Option<Decoration>, DecoratorError>;
}

/// A labeled block of additional context appended to tool output.
#[derive(Debug, Clone)]
pub struct Decoration {
    pub label: String,
    pub content: String,
}

impl std::fmt::Display for Decoration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]\n{}", self.label, self.content)
    }
}
