use std::collections::HashSet;

use serde_json::{json, Value};
use tracing::debug;

use crate::error::AgentError;
use crate::types::{ContentBlock, InferenceRequest, InferenceResponse};

/// Owns everything the LLM sees. The ONE place all context decisions happen.
pub trait ContextManager: Send + Sync {
    /// Build the complete inference request for the next turn.
    fn build_request(&self) -> InferenceRequest;

    /// Record the initial user prompt.
    fn add_prompt(&mut self, prompt: &str);

    /// Record what the model said.
    fn record_response(&mut self, response: &InferenceResponse);

    /// Record a tool execution result.
    fn record_tool_result(&mut self, call_id: &str, name: &str, result: &str, is_error: bool);

    /// Serialize the context state for session persistence.
    fn snapshot(&self) -> Value;

    /// Restore from a serialized snapshot.
    fn restore(&mut self, snapshot: &Value) -> Result<(), AgentError>;

    /// Returns true if auto-compaction should run before the next turn.
    /// The agent loop checks this and drives the summarization call.
    fn needs_compaction(&self) -> bool {
        false
    }

    /// Build an inference request for summarizing the conversation.
    /// Only called when `needs_compaction()` returns true.
    fn build_compaction_request(&self) -> Option<InferenceRequest> {
        None
    }

    /// Apply a compaction summary, replacing message history.
    fn compact(&mut self, _summary: &str) {}
}


// ---------------------------------------------------------------------------
// Token Budget
// ---------------------------------------------------------------------------

/// Tracks token usage across context components.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub context_window: u32,
    pub max_output: u32,
    pub message_tokens: u32,
    pub system_tokens: u32,
    pub tool_schema_tokens: u32,
}

impl TokenBudget {
    /// Effective window = context_window - min(max_output, 20_000).
    /// This is the usable space for input (system + tools + messages).
    pub fn effective_window(&self) -> u32 {
        self.context_window.saturating_sub(self.max_output.min(20_000))
    }

    /// Total tokens currently used by all input components.
    pub fn total_used(&self) -> u32 {
        self.message_tokens + self.system_tokens + self.tool_schema_tokens
    }

    /// Fraction of effective window currently used (0.0 to 1.0+).
    pub fn usage_fraction(&self) -> f32 {
        let eff = self.effective_window();
        if eff == 0 {
            return 1.0;
        }
        self.total_used() as f32 / eff as f32
    }
}

/// Estimate token count from a JSON value. Uses chars/4 heuristic — good enough
/// for trend detection. Claude Code uses local estimation too.
pub fn estimate_tokens(value: &Value) -> u32 {
    let s = value.to_string();
    (s.len() as u32) / 4
}

/// Estimate tokens for a plain string.
pub fn estimate_str_tokens(s: &str) -> u32 {
    (s.len() as u32) / 4
}

// ---------------------------------------------------------------------------
// Compaction State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct CompactionState {
    has_compacted: bool,
    /// Index in messages where the last compaction boundary sits.
    last_compaction_boundary: usize,
    compaction_count: u32,
}

// ---------------------------------------------------------------------------
// ManagedContextManager
// ---------------------------------------------------------------------------

/// Context manager with token tracking, micro-compaction, auto-compaction, and tool deferral.
/// With default thresholds and a large window, none of the compaction logic fires —
/// it's the simple case and the production case in one.
pub struct ManagedContextManager {
    model: String,
    max_tokens: u32,
    context_window: u32,
    system: Option<String>,
    messages: Vec<Value>,
    tool_schemas: Vec<Value>,

    // Tool deferral
    active_tools: HashSet<String>,

    // Compaction
    compaction_state: CompactionState,

    // Thresholds (fractions of effective window)
    compaction_threshold: f32,
    prune_threshold: f32,
    keep_recent_tool_results: usize,
    min_prune_savings_tokens: u32,
    tool_defer_threshold: f32,
}

impl ManagedContextManager {
    pub fn new(model: impl Into<String>, max_tokens: u32, context_window: u32) -> Self {
        Self {
            model: model.into(),
            max_tokens,
            context_window,
            system: None,
            messages: Vec::new(),
            tool_schemas: Vec::new(),
            active_tools: HashSet::new(),
            compaction_state: CompactionState::default(),
            compaction_threshold: 0.80,
            prune_threshold: 0.70,
            keep_recent_tool_results: 3,
            min_prune_savings_tokens: 5_000,
            tool_defer_threshold: 0.15,
        }
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    pub fn with_tools(mut self, schemas: Vec<Value>) -> Self {
        self.tool_schemas = schemas;
        self
    }

    pub fn with_compaction_threshold(mut self, fraction: f32) -> Self {
        self.compaction_threshold = fraction;
        self
    }

    pub fn with_prune_threshold(mut self, fraction: f32) -> Self {
        self.prune_threshold = fraction;
        self
    }

    pub fn with_keep_recent(mut self, n: usize) -> Self {
        self.keep_recent_tool_results = n;
        self
    }

    pub fn with_min_prune_savings(mut self, tokens: u32) -> Self {
        self.min_prune_savings_tokens = tokens;
        self
    }

    pub fn with_tool_defer_threshold(mut self, fraction: f32) -> Self {
        self.tool_defer_threshold = fraction;
        self
    }

    /// Compute the current token budget.
    fn token_budget(&self) -> TokenBudget {
        let system_tokens = self
            .system
            .as_ref()
            .map(|s| estimate_str_tokens(s))
            .unwrap_or(0);
        let tool_schema_tokens: u32 = self.tool_schemas.iter().map(|s| estimate_tokens(s)).sum();
        let message_tokens: u32 = self.messages.iter().map(|m| estimate_tokens(m)).sum();

        TokenBudget {
            context_window: self.context_window,
            max_output: self.max_tokens,
            message_tokens,
            system_tokens,
            tool_schema_tokens,
        }
    }

    /// Apply micro-compaction: prune old tool results, returning modified messages.
    /// Does NOT mutate self.messages — operates on a copy for the request.
    fn micro_compact(&self, messages: &mut Vec<Value>, budget: &TokenBudget) {
        let usage = budget.usage_fraction();
        if usage < self.prune_threshold {
            return;
        }

        // Find all tool_result content blocks with their message index + block index.
        // A user message with role=user can contain an array of tool_result blocks.
        let mut tool_result_locations: Vec<(usize, usize, String)> = Vec::new();
        for (msg_idx, msg) in messages.iter().enumerate() {
            if msg["role"] != "user" {
                continue;
            }
            if let Some(content) = msg["content"].as_array() {
                for (block_idx, block) in content.iter().enumerate() {
                    if block["type"] == "tool_result" {
                        let name = block
                            .get("tool_name")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string();
                        tool_result_locations.push((msg_idx, block_idx, name));
                    }
                }
            }
        }

        if tool_result_locations.len() <= self.keep_recent_tool_results {
            return;
        }

        // Prune all but the last N tool results
        let prune_count = tool_result_locations.len() - self.keep_recent_tool_results;
        let to_prune = &tool_result_locations[..prune_count];

        let mut total_savings: u32 = 0;
        let mut pruned_messages = messages.clone();

        for &(msg_idx, block_idx, ref name) in to_prune {
            if let Some(content) = pruned_messages[msg_idx]["content"].as_array_mut() {
                if let Some(block) = content.get(block_idx) {
                    let old_tokens = estimate_tokens(block);
                    let old_content = block["content"]
                        .as_str()
                        .map(|s| s.len())
                        .unwrap_or_else(|| block["content"].to_string().len());

                    let stub = format!("[tool result pruned — {name}: {old_content} bytes]");
                    let stub_tokens = estimate_str_tokens(&stub);

                    if old_tokens > stub_tokens {
                        total_savings += old_tokens - stub_tokens;
                    }

                    content[block_idx] = json!({
                        "type": "tool_result",
                        "tool_use_id": block["tool_use_id"],
                        "content": stub,
                    });
                }
            }
        }

        if total_savings >= self.min_prune_savings_tokens {
            debug!(
                savings = total_savings,
                pruned = prune_count,
                "micro-compaction applied"
            );
            *messages = pruned_messages;
        }
    }

    /// Apply tool deferral: only include schemas for tools the model has used,
    /// if total schema tokens exceed the defer threshold.
    fn deferred_tools(&self, budget: &TokenBudget) -> Vec<Value> {
        if self.active_tools.is_empty() {
            // Model hasn't used any tools yet — send all schemas
            return self.tool_schemas.clone();
        }

        let eff = budget.effective_window();
        if eff == 0 {
            return self.tool_schemas.clone();
        }

        let schema_fraction = budget.tool_schema_tokens as f32 / eff as f32;
        if schema_fraction <= self.tool_defer_threshold {
            return self.tool_schemas.clone();
        }

        // Only include tools the model has actually used
        let deferred: Vec<Value> = self
            .tool_schemas
            .iter()
            .filter(|schema| {
                schema["name"]
                    .as_str()
                    .map(|n| self.active_tools.contains(n))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        debug!(
            all = self.tool_schemas.len(),
            active = deferred.len(),
            "tool deferral applied"
        );

        deferred
    }
}

// Compaction prompt — adapted from Claude Code's summarization prompt.
const COMPACTION_PROMPT: &str = "\
Summarize the conversation so far. The summary will be used to continue the \
conversation in a fresh context window, so preserve all information needed to \
continue the task without re-reading the original messages.

Structure your summary as:
1. **Primary task and current state** — what was asked, what has been accomplished
2. **Key technical context** — files discussed, code patterns, architecture decisions
3. **Errors encountered and their resolutions** — what went wrong, how it was fixed
4. **Pending work and next steps** — what still needs to happen

Be concise but complete. Omit pleasantries and meta-discussion. Focus on facts \
and decisions.";

const PARTIAL_COMPACTION_PROMPT: &str = "\
Summarize only the RECENT messages (after the existing summary below). \
The earlier summary is retained — do not re-summarize it. Focus on what happened \
since the last summary:

1. What was accomplished
2. New files or code discussed
3. Errors and fixes
4. Updated next steps

Existing summary context follows in the first message.";

impl ContextManager for ManagedContextManager {
    fn build_request(&self) -> InferenceRequest {
        let budget = self.token_budget();
        let mut messages = self.messages.clone();

        // Micro-compaction: prune old tool results if over prune threshold
        self.micro_compact(&mut messages, &budget);

        // Tool deferral: drop unused tool schemas if they're eating too much context
        let tools = self.deferred_tools(&budget);

        InferenceRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system: self.system.clone(),
            tools,
            messages,
        }
    }

    fn add_prompt(&mut self, prompt: &str) {
        self.messages.push(json!({
            "role": "user",
            "content": prompt,
        }));
    }

    fn record_response(&mut self, response: &InferenceResponse) {
        let content: Vec<Value> = response
            .content
            .iter()
            .map(|block| match block {
                ContentBlock::Text(text) => json!({
                    "type": "text",
                    "text": text,
                }),
                ContentBlock::ToolUse { id, name, input } => json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input,
                }),
            })
            .collect();

        // Track which tools the model uses (for tool deferral)
        for block in &response.content {
            if let ContentBlock::ToolUse { name, .. } = block {
                self.active_tools.insert(name.clone());
            }
        }

        self.messages.push(json!({
            "role": "assistant",
            "content": content,
        }));
    }

    fn record_tool_result(&mut self, call_id: &str, name: &str, result: &str, is_error: bool) {
        let mut tool_result = json!({
            "type": "tool_result",
            "tool_use_id": call_id,
            "tool_name": name,
            "content": result,
        });
        if is_error {
            tool_result["is_error"] = json!(true);
        }

        // Batch tool results into existing user message if applicable
        if let Some(last) = self.messages.last_mut() {
            let is_tool_result_msg = last["role"] == "user"
                && last["content"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|c| c.get("type"))
                    .and_then(Value::as_str)
                    == Some("tool_result");

            if is_tool_result_msg {
                if let Some(arr) = last.get_mut("content").and_then(Value::as_array_mut) {
                    arr.push(tool_result);
                    return;
                }
            }
        }

        self.messages.push(json!({
            "role": "user",
            "content": [tool_result],
        }));
    }

    fn snapshot(&self) -> Value {
        json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "context_window": self.context_window,
            "system": self.system,
            "messages": self.messages,
            "tool_schemas": self.tool_schemas,
            "active_tools": self.active_tools.iter().collect::<Vec<_>>(),
            "compaction_state": {
                "has_compacted": self.compaction_state.has_compacted,
                "last_compaction_boundary": self.compaction_state.last_compaction_boundary,
                "compaction_count": self.compaction_state.compaction_count,
            },
        })
    }

    fn restore(&mut self, snapshot: &Value) -> Result<(), AgentError> {
        self.model = snapshot["model"]
            .as_str()
            .ok_or_else(|| AgentError::Context("missing model in snapshot".into()))?
            .to_string();
        self.max_tokens = snapshot["max_tokens"]
            .as_u64()
            .ok_or_else(|| AgentError::Context("missing max_tokens in snapshot".into()))?
            as u32;
        self.context_window = snapshot["context_window"]
            .as_u64()
            .unwrap_or(200_000) as u32;
        self.system = snapshot["system"].as_str().map(String::from);
        self.messages = snapshot["messages"]
            .as_array()
            .ok_or_else(|| AgentError::Context("missing messages in snapshot".into()))?
            .clone();
        self.tool_schemas = snapshot["tool_schemas"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        self.active_tools = snapshot["active_tools"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(cs) = snapshot.get("compaction_state") {
            self.compaction_state.has_compacted =
                cs["has_compacted"].as_bool().unwrap_or(false);
            self.compaction_state.last_compaction_boundary =
                cs["last_compaction_boundary"].as_u64().unwrap_or(0) as usize;
            self.compaction_state.compaction_count =
                cs["compaction_count"].as_u64().unwrap_or(0) as u32;
        }

        Ok(())
    }

    fn needs_compaction(&self) -> bool {
        let budget = self.token_budget();
        budget.usage_fraction() >= self.compaction_threshold
    }

    fn build_compaction_request(&self) -> Option<InferenceRequest> {
        if !self.needs_compaction() {
            return None;
        }

        let prompt = if !self.compaction_state.has_compacted {
            // Full compaction: summarize everything
            let all_text = self
                .messages
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("\n---\n");

            format!(
                "{COMPACTION_PROMPT}\n\n---\nConversation to summarize:\n{all_text}"
            )
        } else {
            // Partial compaction: only summarize messages after the boundary
            let boundary = self.compaction_state.last_compaction_boundary;
            let recent = &self.messages[boundary..];
            let recent_text = recent
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("\n---\n");

            format!(
                "{PARTIAL_COMPACTION_PROMPT}\n\n---\nRecent messages to summarize:\n{recent_text}"
            )
        };

        Some(InferenceRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            system: None,
            tools: vec![],
            messages: vec![json!({
                "role": "user",
                "content": prompt,
            })],
        })
    }

    fn compact(&mut self, summary: &str) {
        let pre_count = self.messages.len();

        if !self.compaction_state.has_compacted {
            // Full compaction: replace all messages with summary
            self.messages = vec![json!({
                "role": "user",
                "content": format!("[Conversation summary — compaction #{}]\n\n{summary}",
                    self.compaction_state.compaction_count + 1),
            })];
            self.compaction_state.last_compaction_boundary = 1;
        } else {
            // Partial compaction: keep messages up to boundary, replace rest with summary
            let boundary = self.compaction_state.last_compaction_boundary;
            self.messages.truncate(boundary);
            self.messages.push(json!({
                "role": "user",
                "content": format!("[Partial summary — compaction #{}]\n\n{summary}",
                    self.compaction_state.compaction_count + 1),
            }));
            self.compaction_state.last_compaction_boundary = self.messages.len();
        }

        self.compaction_state.has_compacted = true;
        self.compaction_state.compaction_count += 1;

        debug!(
            pre_messages = pre_count,
            post_messages = self.messages.len(),
            compaction_count = self.compaction_state.compaction_count,
            "auto-compaction applied"
        );
    }
}
