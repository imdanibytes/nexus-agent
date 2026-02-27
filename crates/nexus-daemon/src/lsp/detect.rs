use super::config::LspServerConfig;

struct KnownLsp {
    name: &'static str,
    commands: &'static [&'static str],
    language_ids: &'static [&'static str],
    default_args: &'static [&'static str],
}

const KNOWN_LSPS: &[KnownLsp] = &[
    KnownLsp {
        name: "rust-analyzer",
        commands: &["rust-analyzer"],
        language_ids: &["rust"],
        default_args: &[],
    },
    KnownLsp {
        name: "TypeScript Language Server",
        commands: &["typescript-language-server"],
        language_ids: &["typescript", "javascript", "typescriptreact", "javascriptreact"],
        default_args: &["--stdio"],
    },
    KnownLsp {
        name: "Pyright",
        commands: &["pyright-langserver", "basedpyright-langserver"],
        language_ids: &["python"],
        default_args: &["--stdio"],
    },
    KnownLsp {
        name: "gopls",
        commands: &["gopls"],
        language_ids: &["go"],
        default_args: &["serve"],
    },
    KnownLsp {
        name: "clangd",
        commands: &["clangd"],
        language_ids: &["c", "cpp"],
        default_args: &[],
    },
    KnownLsp {
        name: "lua-language-server",
        commands: &["lua-language-server"],
        language_ids: &["lua"],
        default_args: &[],
    },
    KnownLsp {
        name: "Zls",
        commands: &["zls"],
        language_ids: &["zig"],
        default_args: &[],
    },
];

/// Scan PATH for known LSP binaries. Returns detected configs.
pub fn detect_installed_servers() -> Vec<LspServerConfig> {
    let mut found = Vec::new();

    for known in KNOWN_LSPS {
        for &cmd in known.commands {
            if which::which(cmd).is_ok() {
                found.push(LspServerConfig {
                    id: format!("auto-{}", cmd),
                    name: known.name.to_string(),
                    language_ids: known.language_ids.iter().map(|s| s.to_string()).collect(),
                    command: cmd.to_string(),
                    args: known.default_args.iter().map(|s| s.to_string()).collect(),
                    enabled: true,
                    auto_detected: true,
                });
                break; // Only need the first matching binary per known LSP
            }
        }
    }

    tracing::info!(count = found.len(), "LSP detection complete");
    for server in &found {
        tracing::debug!(name = %server.name, command = %server.command, "Detected LSP server");
    }

    found
}
