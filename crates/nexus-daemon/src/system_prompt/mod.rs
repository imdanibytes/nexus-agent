mod providers;

pub use providers::*;

/// Snapshot of the current task for system prompt injection.
pub struct CurrentTaskInfo {
    pub plan_title: String,
    pub task_id: String,
    pub task_title: String,
    pub task_description: Option<String>,
    pub completed_count: usize,
    pub total_count: usize,
}

/// Context passed to each provider so it can decide what to emit.
pub struct SystemPromptContext {
    pub conversation_title: String,
    pub message_count: usize,
    pub tool_names: Vec<String>,
    pub agent_name: String,
    pub custom_system_prompt: Option<String>,
    pub input_tokens: u32,
    pub context_window: u32,
    pub mode: String,
    pub current_task: Option<CurrentTaskInfo>,
}

/// A composable section of the system prompt.
pub trait SystemPromptProvider: Send + Sync {
    fn name(&self) -> &str;
    fn provide(&self, ctx: &SystemPromptContext) -> Option<String>;
}

/// Assembles a full system prompt from registered providers.
pub struct SystemPromptBuilder {
    providers: Vec<Box<dyn SystemPromptProvider>>,
}

impl SystemPromptBuilder {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn register(mut self, provider: impl SystemPromptProvider + 'static) -> Self {
        self.providers.push(Box::new(provider));
        self
    }

    /// Build the final system prompt by running each provider in order.
    pub fn build(&self, ctx: &SystemPromptContext) -> String {
        self.providers
            .iter()
            .filter_map(|p| p.provide(ctx))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Default builder with all standard providers registered.
    pub fn default_builder() -> Self {
        Self::new()
            .register(MessageBoundaryProvider)
            .register(IdentityProvider)
            .register(ModeProvider)
            .register(WorkflowProvider)
            .register(CorePromptProvider)
            .register(DatetimeProvider)
            .register(TaskContextProvider)
            .register(ConversationContextProvider)
    }
}
