// Re-export LSP core from the nexus-lsp crate.
pub use nexus_lsp::config;
pub use nexus_lsp::detect;
pub use nexus_lsp::diagnostics;
pub use nexus_lsp::manager;

pub use nexus_lsp::LspService;

// The DaemonModule adapter lives in the daemon (depends on daemon-specific types).
pub mod module;
