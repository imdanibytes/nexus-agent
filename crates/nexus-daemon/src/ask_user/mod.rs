use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use nexus_provider::types::Tool;

// ── Types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    Confirm,
    Select,
    MultiSelect,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub value: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserArgs {
    pub question: String,
    #[serde(rename = "type")]
    pub question_type: QuestionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QuestionOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAnswer {
    pub value: serde_json::Value,
    #[serde(default)]
    pub dismissed: bool,
}

/// A question waiting for user input. Holds the oneshot sender to resume the agent turn.
#[allow(dead_code)] // fields used when serving pending questions to the frontend
pub struct PendingQuestion {
    pub id: String,
    pub conversation_id: String,
    pub tool_call_id: String,
    pub args: AskUserArgs,
    pub created_at: DateTime<Utc>,
    pub response_tx: oneshot::Sender<UserAnswer>,
}

// ── Store ──

pub struct PendingQuestionStore {
    questions: HashMap<String, PendingQuestion>,
}

impl PendingQuestionStore {
    pub fn new() -> Self {
        Self {
            questions: HashMap::new(),
        }
    }

    pub fn insert(&mut self, question: PendingQuestion) {
        self.questions.insert(question.id.clone(), question);
    }

    pub fn remove(&mut self, question_id: &str) -> Option<PendingQuestion> {
        self.questions.remove(question_id)
    }

    #[allow(dead_code)] // part of store API, will be used by question listing endpoint
    pub fn get_for_conversation(&self, conversation_id: &str) -> Vec<&PendingQuestion> {
        self.questions
            .values()
            .filter(|q| q.conversation_id == conversation_id)
            .collect()
    }
}

// ── Tool definition ──

pub fn tool_definition() -> Tool {
    Tool {
        name: "ask_user".into(),
        description:
            "Ask the user a question and wait for their response. Use this for plan approval, \
             confirmations, choosing between options, or collecting free-text input. The agent \
             turn pauses until the user answers."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask. Supports markdown."
                },
                "type": {
                    "type": "string",
                    "enum": ["confirm", "select", "multi_select", "text"],
                    "description": "Question type: confirm (yes/no), select (pick one), multi_select (pick many), text (free input)"
                },
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "value": { "type": "string" },
                            "label": { "type": "string" },
                            "description": { "type": "string" }
                        },
                        "required": ["value", "label"]
                    },
                    "description": "Options for select/multi_select types"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context rendered above the question (plan summary, diff, etc.). Supports markdown."
                },
                "placeholder": {
                    "type": "string",
                    "description": "Placeholder text for text inputs"
                }
            },
            "required": ["question", "type"]
        }),
    }
}

/// Check if a tool name is the ask_user built-in.
pub fn is_ask_user(tool_name: &str) -> bool {
    tool_name == "ask_user"
}
