mod fence;
mod providers;

pub use fence::*;
pub use providers::*;

/// Context passed to each provider so it can decide what to emit.
pub struct SystemPromptContext {
    pub tool_names: Vec<String>,
    pub agent_name: String,
    pub custom_system_prompt: Option<String>,
    pub mode: String,
}

/// A composable section of the system prompt.
#[allow(dead_code)] // trait methods used by implementations and builder
pub trait SystemPromptProvider: Send + Sync {
    fn name(&self) -> &str;
    fn provide(&self, ctx: &SystemPromptContext) -> Option<String>;

    /// Whether this provider's output is stable across rounds/turns.
    /// Static providers go into the cached system prompt; dynamic ones
    /// are injected as `<state_update>` user messages at API call time.
    fn cacheable(&self) -> bool {
        true
    }
}

/// System prompt split into cacheable (system) and dynamic (state) parts.
pub struct SystemPromptParts {
    /// Static system prompt — cached by the API.
    pub system: String,
    /// Dynamic state — injected as a `<state_update>` user message. `None` if
    /// no dynamic providers produced output.
    pub state: Option<String>,
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
    /// Includes both cacheable and dynamic providers in a single string.
    #[allow(dead_code)] // non-split variant of build_parts, kept for simpler use cases
    pub fn build(&self, ctx: &SystemPromptContext) -> String {
        self.providers
            .iter()
            .filter_map(|p| p.provide(ctx))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Build split parts: static system prompt (cached) and dynamic state
    /// (injected as a user message). This enables prompt caching by keeping
    /// the system prompt invariant across rounds and turns.
    pub fn build_parts(&self, ctx: &SystemPromptContext) -> SystemPromptParts {
        let system = self
            .providers
            .iter()
            .filter(|p| p.cacheable())
            .filter_map(|p| p.provide(ctx))
            .collect::<Vec<_>>()
            .join("\n\n");

        let state_parts: Vec<String> = self
            .providers
            .iter()
            .filter(|p| !p.cacheable())
            .filter_map(|p| p.provide(ctx))
            .collect();

        let state = if state_parts.is_empty() {
            None
        } else {
            Some(format!(
                "<state_update>\n{}\n</state_update>",
                state_parts.join("\n")
            ))
        };

        SystemPromptParts { system, state }
    }

    /// Default builder with all standard providers registered.
    ///
    /// Static providers (cacheable) come first, followed by the state protocol
    /// descriptor, then dynamic providers. When using `build_parts()`, only
    /// the static providers go into the cached system prompt.
    pub fn default_builder() -> Self {
        Self::new()
            // Static (cacheable) providers
            .register(MessageBoundaryProvider)
            .register(IdentityProvider)
            .register(SystemInfoProvider)
            .register(ModeProvider)
            .register(WorkflowProvider)
            .register(CorePromptProvider)
            .register(StateProtocolProvider)
            // Dynamic (not cacheable) providers — injected as <state_update>
            .register(DatetimeProvider)
            // Note: TaskContextProvider and ConversationContextProvider are now
            // DaemonModules that contribute via the turn_start hook.
    }
}
