use crate::anthropic::types::{ContentBlock, Message, Role};

/// Prune old tool results from API messages to reclaim context space.
///
/// Keeps the last `keep_recent` tool results intact. Earlier results are
/// replaced with compact stubs showing tool name and content size.
/// Also stubs out the matching `ToolUse.input` args for pruned calls
/// (write_file/edit_file args can be huge).
///
/// Operates in-place on the API message array — stored ChatMessages are
/// untouched.
pub fn prune_tool_results(messages: &mut [Message], keep_recent: usize) {
    // First pass: collect (message_idx, block_idx) of every ToolResult, in order.
    let mut tool_result_positions: Vec<(usize, usize)> = Vec::new();

    for (msg_idx, msg) in messages.iter().enumerate() {
        for (block_idx, block) in msg.content.iter().enumerate() {
            if matches!(block, ContentBlock::ToolResult { .. }) {
                tool_result_positions.push((msg_idx, block_idx));
            }
        }
    }

    let total = tool_result_positions.len();
    if total <= keep_recent {
        return; // Nothing to prune
    }

    let prune_count = total - keep_recent;
    let to_prune = &tool_result_positions[..prune_count];

    // Build a map of tool_use_id → tool_name from assistant messages
    let mut tool_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for msg in messages.iter() {
        if msg.role != Role::Assistant {
            continue;
        }
        for block in &msg.content {
            if let ContentBlock::ToolUse { id, name, .. } = block {
                tool_names.insert(id.clone(), name.clone());
            }
        }
    }

    // Collect tool_use_ids that we're pruning (for stubbing their args too)
    let mut pruned_tool_use_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Second pass: replace pruned tool results with stubs
    for &(msg_idx, block_idx) in to_prune {
        let block = &messages[msg_idx].content[block_idx];
        if let ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } = block
        {
            let tool_name = tool_names
                .get(tool_use_id)
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            let char_count = content.len();
            let stub = format!("[{}: {} chars]", tool_name, char_count);
            pruned_tool_use_ids.insert(tool_use_id.clone());

            messages[msg_idx].content[block_idx] = ContentBlock::ToolResult {
                tool_use_id: tool_use_id.clone(),
                content: stub,
                is_error: *is_error,
            };
        }
    }

    // Third pass: stub out ToolUse.input for pruned tool calls
    for msg in messages.iter_mut() {
        if msg.role != Role::Assistant {
            continue;
        }
        for block in msg.content.iter_mut() {
            if let ContentBlock::ToolUse { id, input, .. } = block {
                if pruned_tool_use_ids.contains(id) {
                    *input = serde_json::json!({});
                }
            }
        }
    }

    tracing::info!(
        pruned = prune_count,
        kept = keep_recent,
        total,
        "Tool result pruning"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool_pair(id: &str, name: &str, result: &str) -> (Message, Message) {
        let assistant = Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({"path": "/some/long/path/to/file.rs"}),
            }],
        };
        let user = Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: result.to_string(),
                is_error: None,
            }],
        };
        (assistant, user)
    }

    #[test]
    fn prune_keeps_recent_results() {
        let mut messages = Vec::new();
        messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
        });
        for i in 0..5 {
            let (a, u) = make_tool_pair(
                &format!("tool_{}", i),
                "read_file",
                &"x".repeat(10000),
            );
            messages.push(a);
            messages.push(u);
        }

        prune_tool_results(&mut messages, 3);

        let mut full = 0;
        let mut stubs = 0;
        for msg in &messages {
            for block in &msg.content {
                if let ContentBlock::ToolResult { content, .. } = block {
                    if content.starts_with('[') {
                        stubs += 1;
                    } else {
                        full += 1;
                    }
                }
            }
        }

        assert_eq!(full, 3, "should keep 3 recent results");
        assert_eq!(stubs, 2, "should prune 2 old results");
    }

    #[test]
    fn prune_stubs_include_tool_name_and_size() {
        let mut messages = Vec::new();
        messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
        });
        for i in 0..4 {
            let (a, u) = make_tool_pair(
                &format!("tool_{}", i),
                "read_file",
                &"y".repeat(5000),
            );
            messages.push(a);
            messages.push(u);
        }

        prune_tool_results(&mut messages, 3);

        let first_result = messages
            .iter()
            .flat_map(|m| m.content.iter())
            .find_map(|b| {
                if let ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } = b
                {
                    if tool_use_id == "tool_0" {
                        Some(content.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap();

        assert_eq!(first_result, "[read_file: 5000 chars]");
    }

    #[test]
    fn prune_stubs_tool_use_args() {
        let mut messages = Vec::new();
        messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
        });
        for i in 0..4 {
            let (a, u) = make_tool_pair(
                &format!("tool_{}", i),
                "write_file",
                &"z".repeat(1000),
            );
            messages.push(a);
            messages.push(u);
        }

        prune_tool_results(&mut messages, 3);

        let first_input = messages
            .iter()
            .flat_map(|m| m.content.iter())
            .find_map(|b| {
                if let ContentBlock::ToolUse { id, input, .. } = b {
                    if id == "tool_0" {
                        Some(input.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap();

        assert_eq!(first_input, serde_json::json!({}));
    }

    #[test]
    fn prune_noop_when_under_threshold() {
        let mut messages = Vec::new();
        messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
        });
        let (a, u) = make_tool_pair("tool_0", "read_file", "some content");
        messages.push(a);
        messages.push(u);

        let original_content = messages[2].content[0].clone();
        prune_tool_results(&mut messages, 3);

        assert_eq!(messages[2].content[0], original_content);
    }
}
