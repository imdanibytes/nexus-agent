//! Mock LSP server for integration tests.
//!
//! Implements just enough of the LSP protocol to test the daemon's decorator:
//! - Responds to `initialize` with capabilities
//! - On `textDocument/didOpen`, publishes a canned diagnostic
//! - On `textDocument/didChange`, publishes updated diagnostics
//! - Handles `shutdown` + `exit` gracefully

use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    loop {
        let msg = match read_message(&mut reader) {
            Ok(msg) => msg,
            Err(_) => break,
        };

        let json: serde_json::Value = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let method = json.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = json.get("id").cloned();

        match method {
            "initialize" => {
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "capabilities": {
                            "textDocumentSync": 1,
                            "diagnosticProvider": { "interFileDependencies": false, "workspaceDiagnostics": false }
                        }
                    }
                });
                write_message(&mut writer, &response);
            }
            "initialized" => {
                // No response needed
            }
            "textDocument/didOpen" => {
                if let Some(params) = json.get("params") {
                    if let Some(uri) = params
                        .get("textDocument")
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str())
                    {
                        send_diagnostics(&mut writer, uri);
                    }
                }
            }
            "textDocument/didChange" => {
                if let Some(params) = json.get("params") {
                    if let Some(uri) = params
                        .get("textDocument")
                        .and_then(|td| td.get("uri"))
                        .and_then(|u| u.as_str())
                    {
                        send_diagnostics(&mut writer, uri);
                    }
                }
            }
            "textDocument/didClose" => {
                // No response needed
            }
            "shutdown" => {
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": null
                });
                write_message(&mut writer, &response);
            }
            "exit" => break,
            _ => {}
        }
    }
}

/// Always sends one error diagnostic at line 5.
fn send_diagnostics(writer: &mut impl Write, uri: &str) {
    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": [
                {
                    "range": {
                        "start": { "line": 4, "character": 0 },
                        "end": { "line": 4, "character": 10 }
                    },
                    "severity": 1,
                    "source": "mock-lsp",
                    "message": "mock error: undefined variable"
                }
            ]
        }
    });
    write_message(writer, &notification);
}

fn read_message(reader: &mut impl BufRead) -> io::Result<String> {
    let mut content_length: usize = 0;

    // Read headers
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str
                .parse()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        }
    }

    if content_length == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "no content-length",
        ));
    }

    let mut buf = vec![0u8; content_length];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn write_message(writer: &mut impl Write, msg: &serde_json::Value) {
    let body = serde_json::to_string(msg).unwrap();
    let _ = write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body);
    let _ = writer.flush();
}
