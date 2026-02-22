use async_trait::async_trait;
use serde_json::Value;

/// A tool's execution handler. Consumers implement this for each tool.
#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn call(&self, input: &Value) -> Result<String, String>;
}

/// A tool definition: schema for the LLM + handler for execution.
pub struct ToolDef {
    pub name: String,
    pub schema: Value,
    pub(crate) handler: Box<dyn ToolHandler>,
}
