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
    handler: Box<dyn ToolHandler>,
}

/// Collection of tools available to the agent.
pub struct ToolSet {
    tools: Vec<ToolDef>,
}

impl ToolSet {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Add a tool. The schema is the complete JSON tool definition
    /// (name, description, input_schema) sent to the LLM.
    pub fn add(
        mut self,
        name: impl Into<String>,
        schema: Value,
        handler: impl ToolHandler + 'static,
    ) -> Self {
        self.tools.push(ToolDef {
            name: name.into(),
            schema,
            handler: Box::new(handler),
        });
        self
    }

    /// Returns tool schemas for the LLM API request.
    pub fn schemas(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.schema.clone()).collect()
    }

    /// Execute a tool by name. Returns Err if the tool is unknown.
    pub async fn execute(&self, name: &str, input: &Value) -> Result<String, String> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| format!("unknown tool: {name}"))?;
        tool.handler.call(input).await
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }
}

impl Default for ToolSet {
    fn default() -> Self {
        Self::new()
    }
}
