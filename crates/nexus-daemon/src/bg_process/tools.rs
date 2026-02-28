pub use nexus_tools::bg_process::{is_bg_process_tool, tool_definitions};

use async_trait::async_trait;

use crate::agent::tool_dispatch::{ToolContext, ToolHandler, ToolResult};
use super::manager::ProcessManager;

// Implement the crate's ProcessBackend trait on daemon's ProcessManager.
#[async_trait]
impl nexus_tools::bg_process::ProcessBackend for ProcessManager {
    async fn read_output(
        &self,
        process_id: &str,
        tail: Option<usize>,
        head: Option<usize>,
    ) -> Result<String, String> {
        self.read_output(process_id, tail, head).await
    }

    async fn list_json(&self, conversation_id: &str) -> String {
        let processes = self.list(conversation_id).await;
        serde_json::to_string_pretty(&processes).unwrap_or_default()
    }

    async fn cancel(&self, process_id: &str) -> Result<(), String> {
        self.cancel(process_id).await
    }
}

pub struct BgProcessToolHandler<'a> {
    pub process_manager: &'a ProcessManager,
}

#[async_trait]
impl ToolHandler for BgProcessToolHandler<'_> {
    fn can_handle(&self, tool_name: &str) -> bool {
        is_bg_process_tool(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        let args: serde_json::Value = serde_json::from_str(ctx.args_json)
            .unwrap_or_else(|_| serde_json::json!({}));
        let (content, is_error) = nexus_tools::bg_process::execute(
            ctx.tool_name,
            &args,
            ctx.conversation_id,
            self.process_manager,
        )
        .await;
        ToolResult { content, is_error, injected_messages: Vec::new() }
    }
}
