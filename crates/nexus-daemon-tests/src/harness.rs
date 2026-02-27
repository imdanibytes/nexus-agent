use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

/// A running nexus daemon instance, isolated to a temp directory.
///
/// Each `TestDaemon` gets its own temp dir (HOME), random port, and
/// data directory. On drop the child process is killed and the temp
/// dir is removed.
pub struct TestDaemon {
    child: Child,
    pub port: u16,
    pub base_url: String,
    _home_dir: TempDir,
    pub home_path: PathBuf,
}

impl TestDaemon {
    /// Spawn a new isolated daemon instance.
    pub async fn spawn() -> anyhow::Result<Self> {
        Self::spawn_inner(false).await
    }

    /// Spawn a daemon with ANTHROPIC_API_KEY passed through.
    /// Returns `None` if the env var isn't set.
    pub async fn spawn_with_api_key() -> anyhow::Result<Option<Self>> {
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            return Ok(None);
        }
        Ok(Some(Self::spawn_inner(true).await?))
    }

    async fn spawn_inner(pass_api_key: bool) -> anyhow::Result<Self> {
        let home_dir = TempDir::new()?;
        let home_path = home_dir.path().to_path_buf();
        let nexus_dir = home_path.join(".nexus");
        std::fs::create_dir_all(&nexus_dir)?;

        let port = allocate_free_port()?;

        let config = serde_json::json!({
            "server": { "host": "127.0.0.1", "port": port }
        });
        std::fs::write(
            nexus_dir.join("nexus.json"),
            serde_json::to_string_pretty(&config)?,
        )?;
        std::fs::write(nexus_dir.join("mcp.json"), "[]")?;

        let binary = nexus_binary_path()?;
        let mut cmd = Command::new(&binary);
        cmd.env("HOME", &home_path)
            .current_dir(&home_path)
            .env(
                "RUST_LOG",
                std::env::var("NEXUS_TEST_LOG").unwrap_or_else(|_| "error".into()),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        if pass_api_key {
            if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                cmd.env("ANTHROPIC_API_KEY", key);
            }
        } else {
            cmd.env_remove("ANTHROPIC_API_KEY");
        }

        let child = cmd.spawn()?;
        let base_url = format!("http://127.0.0.1:{port}");

        let daemon = Self {
            child,
            port,
            base_url: base_url.clone(),
            _home_dir: home_dir,
            home_path,
        };

        daemon.wait_ready(Duration::from_secs(10)).await?;
        Ok(daemon)
    }

    pub fn client(&self) -> crate::client::DaemonClient {
        crate::client::DaemonClient::new(self.base_url.clone())
    }

    pub fn sse(&self) -> crate::sse::SseSubscription {
        crate::sse::SseSubscription::connect(format!("{}/api/events", self.base_url))
    }

    async fn wait_ready(&self, timeout: Duration) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/status", self.base_url);
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if let Ok(r) = client.get(&url).send().await {
                if r.status().is_success() {
                    return Ok(());
                }
            }

            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "Daemon on port {} did not become ready within {timeout:?}",
                    self.port
                );
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn allocate_free_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

fn nexus_binary_path() -> anyhow::Result<PathBuf> {
    // Try cargo-set env var first
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_nexus") {
        return Ok(PathBuf::from(p));
    }

    // Walk from CARGO_MANIFEST_DIR to find workspace target/debug/nexus
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let workspace_root = PathBuf::from(manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());

        if let Some(root) = workspace_root {
            let debug_bin = root.join("target").join("debug").join("nexus");
            if debug_bin.exists() {
                return Ok(debug_bin);
            }
        }
    }

    anyhow::bail!(
        "nexus binary not found. Run `cargo build -p nexus-daemon` first, \
         or set CARGO_BIN_EXE_nexus."
    )
}
