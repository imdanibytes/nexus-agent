use crate::anthropic::types::Tool;
use crate::tasks::types::AgentMode;

/// Snapshot of plan state for filter decisions.
pub struct PlanSnapshot {
    pub approved: Option<bool>,
    pub task_count: usize,
    pub completed_count: usize,
}

/// Context passed to each filter for decision-making.
pub struct ToolFilterContext {
    pub mode: AgentMode,
    pub plan: Option<PlanSnapshot>,
}

/// A composable filter that decides which tools the model can see.
pub trait ToolFilter: Send + Sync {
    fn name(&self) -> &str;
    /// Return true to keep the tool, false to exclude it.
    fn allow(&self, ctx: &ToolFilterContext, tool_name: &str) -> bool;
}

/// Assembles a filter chain. All filters must allow a tool for it to be included (AND logic).
pub struct ToolFilterChain {
    filters: Vec<Box<dyn ToolFilter>>,
}

impl ToolFilterChain {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    pub fn register(mut self, f: impl ToolFilter + 'static) -> Self {
        self.filters.push(Box::new(f));
        self
    }

    /// Keep only tools that pass ALL filters.
    pub fn apply(&self, ctx: &ToolFilterContext, tools: Vec<Tool>) -> Vec<Tool> {
        tools
            .into_iter()
            .filter(|t| self.filters.iter().all(|f| f.allow(ctx, &t.name)))
            .collect()
    }

    /// Default chain with standard filters registered.
    pub fn default_chain() -> Self {
        Self::new()
            .register(ClientOnlyFilter)
            .register(ModeToolFilter)
    }
}

// ── Filter 1: Client-Only ──

/// Excludes tools marked as client-only (MCP Apps visibility pattern).
struct ClientOnlyFilter;

impl ToolFilter for ClientOnlyFilter {
    fn name(&self) -> &str {
        "client_only"
    }

    fn allow(&self, _ctx: &ToolFilterContext, tool_name: &str) -> bool {
        !crate::tasks::tools::is_client_only(tool_name)
    }
}

// ── Filter 2: Mode-Based ──

/// Restricts available tools based on the current agent mode.
struct ModeToolFilter;

impl ToolFilter for ModeToolFilter {
    fn name(&self) -> &str {
        "mode"
    }

    fn allow(&self, ctx: &ToolFilterContext, tool_name: &str) -> bool {
        match ctx.mode {
            AgentMode::General | AgentMode::Execution => true,
            AgentMode::Discovery => matches!(
                tool_name,
                "task_create_plan" | "task_list" | "ask_user"
            ),
            AgentMode::Planning => {
                crate::tasks::tools::is_builtin(tool_name) || tool_name == "ask_user"
            }
            AgentMode::Validation => !matches!(
                tool_name,
                "task_create_plan" | "task_approve_plan" | "task_create"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> Tool {
        Tool {
            name: name.to_string(),
            description: String::new(),
            input_schema: serde_json::json!({"type": "object"}),
        }
    }

    fn ctx(mode: AgentMode) -> ToolFilterContext {
        ToolFilterContext { mode, plan: None }
    }

    fn apply(mode: AgentMode, tools: Vec<Tool>) -> Vec<String> {
        let chain = ToolFilterChain::default_chain();
        chain
            .apply(&ctx(mode), tools)
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    fn all_tools() -> Vec<Tool> {
        vec![
            tool("task_create_plan"),
            tool("task_approve_plan"),
            tool("task_create"),
            tool("task_update"),
            tool("task_list"),
            tool("ask_user"),
            tool("mcp_read_file"),
            tool("mcp_write_file"),
            tool("mcp_run_tests"),
        ]
    }

    #[test]
    fn general_mode_allows_all() {
        let result = apply(AgentMode::General, all_tools());
        assert_eq!(result.len(), 9);
    }

    #[test]
    fn execution_mode_allows_all() {
        let result = apply(AgentMode::Execution, all_tools());
        assert_eq!(result.len(), 9);
    }

    #[test]
    fn discovery_mode_restricts_to_plan_creation() {
        let result = apply(AgentMode::Discovery, all_tools());
        assert_eq!(result, vec!["task_create_plan", "task_list", "ask_user"]);
    }

    #[test]
    fn planning_mode_restricts_to_task_tools() {
        let result = apply(AgentMode::Planning, all_tools());
        assert_eq!(
            result,
            vec![
                "task_create_plan",
                "task_approve_plan",
                "task_create",
                "task_update",
                "task_list",
                "ask_user",
            ]
        );
    }

    #[test]
    fn validation_mode_excludes_plan_creation() {
        let result = apply(AgentMode::Validation, all_tools());
        assert!(result.contains(&"task_update".to_string()));
        assert!(result.contains(&"task_list".to_string()));
        assert!(result.contains(&"ask_user".to_string()));
        assert!(result.contains(&"mcp_read_file".to_string()));
        assert!(result.contains(&"mcp_run_tests".to_string()));
        assert!(!result.contains(&"task_create_plan".to_string()));
        assert!(!result.contains(&"task_approve_plan".to_string()));
        assert!(!result.contains(&"task_create".to_string()));
    }
}
