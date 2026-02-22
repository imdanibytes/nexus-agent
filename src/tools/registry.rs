use serde_json::{json, Value};

use super::handler::{ToolDef, ToolHandler};

/// Catalog of available tools. Stores definitions, provides schemas,
/// looks up handlers by name, and offers a built-in search for tool discovery.
pub struct ToolRegistry {
    tools: Vec<ToolDef>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Register a tool. The schema is the complete JSON tool definition
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

    /// All tool schemas for the LLM API request.
    pub fn schemas(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.schema.clone()).collect()
    }

    /// Schema for a specific tool by name.
    pub fn schema(&self, name: &str) -> Option<&Value> {
        self.tools.iter().find(|t| t.name == name).map(|t| &t.schema)
    }

    /// Look up a tool's handler by name.
    pub async fn execute(&self, name: &str, input: &Value) -> Result<String, String> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| format!("unknown tool: {name}"))?;
        tool.handler.call(input).await
    }

    /// Search tools by query. Matches against name and description.
    /// Returns compact summaries (name + description only, no full input_schema)
    /// so the model can discover deferred tools without blowing the context budget.
    pub fn search(&self, query: &str) -> Vec<Value> {
        let query_lower = query.to_lowercase();
        let terms: Vec<&str> = query_lower.split_whitespace().collect();

        self.tools
            .iter()
            .filter(|t| {
                let name = t.name.to_lowercase();
                let desc = t.schema["description"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase();
                let haystack = format!("{name} {desc}");

                terms.iter().any(|term| haystack.contains(term))
            })
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.schema["description"],
                })
            })
            .collect()
    }

    /// The schema for the built-in `tool_search` meta-tool.
    /// Add this to the LLM's tool list so the model can discover deferred tools.
    pub fn search_tool_schema() -> Value {
        json!({
            "name": "tool_search",
            "description": "Search for available tools by keyword. Use when you need a tool that isn't in your current list. Returns tool names and descriptions matching the query.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query — keywords to match against tool names and descriptions"
                    }
                },
                "required": ["query"]
            }
        })
    }

    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.iter().map(|t| t.name.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> ToolRegistry {
        ToolRegistry::new()
            .add(
                "read_file",
                json!({
                    "name": "read_file",
                    "description": "Read the contents of a file at the given path",
                    "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
                }),
                NoopHandler,
            )
            .add(
                "write_file",
                json!({
                    "name": "write_file",
                    "description": "Write content to a file, creating it if needed",
                    "input_schema": {"type": "object", "properties": {"path": {"type": "string"}, "content": {"type": "string"}}}
                }),
                NoopHandler,
            )
            .add(
                "execute_command",
                json!({
                    "name": "execute_command",
                    "description": "Run a shell command and return stdout/stderr",
                    "input_schema": {"type": "object", "properties": {"command": {"type": "string"}}}
                }),
                NoopHandler,
            )
    }

    struct NoopHandler;

    #[async_trait::async_trait]
    impl ToolHandler for NoopHandler {
        async fn call(&self, _input: &Value) -> Result<String, String> {
            Ok("ok".into())
        }
    }

    #[test]
    fn search_by_name() {
        let reg = test_registry();
        let results = reg.search("read");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "read_file");
    }

    #[test]
    fn search_by_description() {
        let reg = test_registry();
        let results = reg.search("shell");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "execute_command");
    }

    #[test]
    fn search_multiple_matches() {
        let reg = test_registry();
        let results = reg.search("file");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_no_match() {
        let reg = test_registry();
        let results = reg.search("database");
        assert!(results.is_empty());
    }

    #[test]
    fn search_returns_compact_summaries() {
        let reg = test_registry();
        let results = reg.search("read");
        // Should have name + description but NOT full input_schema
        assert!(results[0].get("name").is_some());
        assert!(results[0].get("description").is_some());
        assert!(results[0].get("input_schema").is_none());
    }

    #[test]
    fn search_tool_schema_is_valid() {
        let schema = ToolRegistry::search_tool_schema();
        assert_eq!(schema["name"], "tool_search");
        assert!(schema["input_schema"]["properties"]["query"].is_object());
    }
}
