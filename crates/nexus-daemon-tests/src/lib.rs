pub mod client;
pub mod fixtures;
pub mod harness;
pub mod mock_llm;
pub mod sse;

#[cfg(test)]
mod tests {
    mod agents;
    mod browse;
    mod chat;
    mod conversation_paths;
    mod conversations;
    mod debug_endpoints;
    mod error_cases;
    mod event_emission;
    mod health;
    mod mcp_servers;
    mod persistence;
    mod providers;
    mod sse;
    mod sse_advanced;
    mod tools;
    mod turn_lifecycle;
    mod workspace_projects;
    mod workspaces;
    mod lsp;
    mod hooks;
    mod processes;
    mod settings;
}
