use serde_json::Value;
use tracing::warn;

use crate::decorator::{Decoration, ToolDecorator, ToolTransform};
use super::registry::ToolRegistry;

/// Executes tools through a two-phase output pipeline.
///
/// 1. **Transforms** mutate the output in order (redaction, source tagging)
/// 2. **Decorators** append enrichments (memory context, code index)
///
/// Both phases are non-fatal. A failing transform or decorator logs a warning
/// and the pipeline continues with the previous output.
pub struct ToolPipeline {
    registry: ToolRegistry,
    transforms: Vec<Box<dyn ToolTransform>>,
    decorators: Vec<Box<dyn ToolDecorator>>,
}

impl ToolPipeline {
    pub fn new(registry: ToolRegistry) -> Self {
        Self {
            registry,
            transforms: Vec::new(),
            decorators: Vec::new(),
        }
    }

    /// Add a transform. Transforms run in insertion order, before decorators.
    pub fn with_transform(mut self, transform: impl ToolTransform + 'static) -> Self {
        self.transforms.push(Box::new(transform));
        self
    }

    /// Add a decorator. Decorators run in insertion order, after transforms.
    pub fn with_decorator(mut self, decorator: impl ToolDecorator + 'static) -> Self {
        self.decorators.push(Box::new(decorator));
        self
    }

    /// Execute a tool by name and run the full pipeline on the result.
    pub async fn execute(&self, name: &str, input: &Value) -> Result<String, String> {
        // Handle the built-in tool_search meta-tool
        if name == "tool_search" {
            let query = input["query"].as_str().unwrap_or("");
            let results = self.registry.search(query);
            return Ok(serde_json::to_string_pretty(&results)
                .unwrap_or_else(|_| "[]".into()));
        }

        let raw = self.registry.execute(name, input).await?;
        Ok(self.run(name, input, raw).await)
    }

    /// Run the pipeline on arbitrary output (no tool lookup).
    pub async fn run(&self, tool_name: &str, input: &Value, raw_output: String) -> String {
        // Phase 1: transforms
        let mut output = raw_output;
        for t in &self.transforms {
            if !t.applies_to(tool_name, input) {
                continue;
            }
            match t.transform(tool_name, input, output.clone()).await {
                Ok(transformed) => output = transformed,
                Err(e) => {
                    warn!(
                        transform = t.name(),
                        tool = tool_name,
                        error = %e,
                        "transform failed, using previous output"
                    );
                }
            }
        }

        // Phase 2: decorators
        let decorations = self.decorate(tool_name, input, &output).await;
        if !decorations.is_empty() {
            output = format!(
                "{}\n\n---\n{}",
                output,
                decorations
                    .iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join("\n\n")
            );
        }

        output
    }

    async fn decorate(&self, tool_name: &str, input: &Value, output: &str) -> Vec<Decoration> {
        let mut decorations = Vec::new();

        for dec in &self.decorators {
            if !dec.applies_to(tool_name, input) {
                continue;
            }
            match dec.decorate(tool_name, input, output).await {
                Ok(Some(decoration)) => decorations.push(decoration),
                Ok(None) => {}
                Err(e) => {
                    warn!(
                        decorator = dec.name(),
                        tool = tool_name,
                        error = %e,
                        "decorator failed, skipping"
                    );
                }
            }
        }

        decorations
    }

    /// Delegate to the registry for schemas.
    pub fn schemas(&self) -> Vec<Value> {
        self.registry.schemas()
    }

    /// Search the registry for tools matching a query.
    pub fn search(&self, query: &str) -> Vec<Value> {
        self.registry.search(query)
    }

    /// All tool names in the registry.
    pub fn tool_names(&self) -> Vec<&str> {
        self.registry.tool_names()
    }

    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }

    pub fn len(&self) -> usize {
        self.registry.len()
    }

    /// Access the underlying registry.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }
}
