//! Interactive chat REPL for the nexus-agent.
//!
//! Usage:
//!   ANTHROPIC_API_KEY=sk-... cargo run --example chat
//!   ANTHROPIC_API_KEY=sk-... cargo run --example chat -- --thinking 10000
//!   cargo run --example chat -- --provider ollama --model llama3.2
//!   cargo run --example chat -- --provider ollama --model deepseek-r1:8b --thinking 1 --base-url http://192.168.88.45:11434
//!   OPENAI_API_KEY=sk-... cargo run --example chat -- --provider openai --model gpt-4o
//!
//! Ctrl-C or type "exit" / "quit" to leave.

use std::io::{self, BufRead, Write};

use clap::Parser;
use nexus_agent::{
    Agent, AgentConfig, AgentEvent, AnthropicProvider, InferenceProvider, ManagedContextManager,
    OllamaProvider, OpenAiProvider, ToolPipeline, ToolRegistry,
};

#[derive(Parser)]
#[command(name = "chat", about = "Chat with a nexus-agent")]
struct Cli {
    /// Provider: "anthropic", "ollama", or "openai"
    #[arg(long, default_value = "anthropic")]
    provider: String,

    /// Model to use
    #[arg(long, default_value = "claude-sonnet-4-20250514")]
    model: String,

    /// System prompt
    #[arg(long, short = 's')]
    system: Option<String>,

    /// Max output tokens per turn
    #[arg(long, default_value_t = 4096)]
    max_tokens: u32,

    /// Context window size
    #[arg(long, default_value_t = 200_000)]
    context_window: u32,

    /// Max agent turns per message
    #[arg(long, default_value_t = 1)]
    max_turns: usize,

    /// Enable extended thinking with the given token budget (Anthropic only)
    #[arg(long)]
    thinking: Option<u32>,

    /// API base URL (defaults depend on provider)
    #[arg(long)]
    base_url: Option<String>,
}

fn build_provider(cli: &Cli) -> Box<dyn InferenceProvider> {
    match cli.provider.as_str() {
        "anthropic" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| {
                eprintln!("error: ANTHROPIC_API_KEY not set");
                std::process::exit(1);
            });
            let mut p = AnthropicProvider::new(&api_key);
            if let Some(ref url) = cli.base_url {
                p = p.with_base_url(url);
            }
            Box::new(p)
        }
        "ollama" => {
            let mut p = OllamaProvider::new();
            if let Some(ref url) = cli.base_url {
                p = p.with_base_url(url);
            }
            Box::new(p)
        }
        "openai" => {
            let base = cli
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com".into());
            let mut p = OpenAiProvider::new(base);
            if let Ok(key) = std::env::var("OPENAI_API_KEY") {
                p = p.with_api_key(key);
            }
            Box::new(p)
        }
        other => {
            eprintln!("error: unknown provider '{other}'. Use 'anthropic', 'ollama', or 'openai'.");
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let provider = build_provider(&cli);

    // Context
    let mut context =
        ManagedContextManager::new(&cli.model, cli.max_tokens, cli.context_window);
    if let Some(ref sys) = cli.system {
        context = context.with_system(sys);
    }
    if let Some(budget) = cli.thinking {
        context = context.with_thinking(budget);
    }

    // No tools for now — pure chat
    let tools = ToolPipeline::new(ToolRegistry::new());

    let config = AgentConfig {
        model: cli.model.clone(),
        max_tokens: cli.max_tokens,
        max_turns: cli.max_turns,
        session_id: None,
    };

    let mut agent = Agent::new(provider, context, tools, config);

    // Header
    eprintln!("nexus-agent chat");
    eprintln!("provider: {}", cli.provider);
    eprintln!("model: {}", cli.model);
    if let Some(ref sys) = cli.system {
        eprintln!("system: {sys}");
    }
    if let Some(budget) = cli.thinking {
        eprintln!("thinking: {budget} token budget");
    }
    eprintln!("---");

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    let show_thinking = cli.thinking.is_some();

    loop {
        eprint!("\x1b[1;36myou>\x1b[0m ");
        io::stderr().flush().ok();

        let line = match lines.next() {
            Some(Ok(line)) => line,
            _ => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if matches!(trimmed, "exit" | "quit" | "/q") {
            break;
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(64);

        // Spawn a task to print events as they arrive
        let printer = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    AgentEvent::Thinking { content } => {
                        if show_thinking {
                            eprintln!("\x1b[2;3m{content}\x1b[0m");
                        }
                    }
                    AgentEvent::Text { content } => {
                        eprint!("\x1b[1;32magent>\x1b[0m ");
                        println!("{content}");
                    }
                    AgentEvent::ToolCall { name, input } => {
                        eprintln!("\x1b[33m  [tool: {name}]\x1b[0m {input}");
                    }
                    AgentEvent::ToolResult {
                        name,
                        output,
                        is_error,
                    } => {
                        let tag = if is_error { "error" } else { "result" };
                        let truncated = if output.len() > 200 {
                            format!("{}...", &output[..200])
                        } else {
                            output
                        };
                        eprintln!("\x1b[33m  [{tag}: {name}]\x1b[0m {truncated}");
                    }
                    AgentEvent::Compacted {
                        pre_tokens,
                        post_tokens,
                    } => {
                        eprintln!(
                            "\x1b[35m  [compacted: {pre_tokens} → {post_tokens} tokens]\x1b[0m"
                        );
                    }
                    AgentEvent::Finished { turns } => {
                        if turns > 1 {
                            eprintln!("\x1b[2m  ({turns} turns)\x1b[0m");
                        }
                    }
                    _ => {}
                }
            }
        });

        match agent.invoke_streaming(trimmed, tx).await {
            Ok(result) => {
                printer.await.ok();
                eprintln!(
                    "\x1b[2m  [{}in / {}out tokens]\x1b[0m",
                    result.usage.input_tokens, result.usage.output_tokens
                );
            }
            Err(e) => {
                printer.await.ok();
                eprintln!("\x1b[1;31merror:\x1b[0m {e}");
            }
        }
    }

    eprintln!("bye.");
}
