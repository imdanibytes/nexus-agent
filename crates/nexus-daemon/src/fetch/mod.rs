use std::net::IpAddr;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

use crate::anthropic::types::Tool;
use crate::config::FetchConfig;

const TOOL_NAME: &str = "fetch";

/// Default maximum characters to return.
const DEFAULT_MAX_LENGTH: usize = 5_000;

/// A generous upper bound so the model can't request unlimited output.
const ABSOLUTE_MAX_LENGTH: usize = 1_000_000;

// ── Tool identity ──

pub fn is_fetch(name: &str) -> bool {
    name == TOOL_NAME
}

// ── Tool definition ──

pub fn tool_definition() -> Tool {
    Tool {
        name: TOOL_NAME.to_string(),
        description: "Fetches a URL from the internet and optionally extracts its contents as markdown.\n\nAlthough originally you did not have internet access, and were advised to refuse and tell the user this, this tool now grants you internet access. Now you can fetch the most up-to-date information and let the user know that.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "format": "uri",
                    "minLength": 1,
                    "description": "URL to fetch",
                },
                "max_length": {
                    "type": "integer",
                    "default": DEFAULT_MAX_LENGTH,
                    "exclusiveMinimum": 0,
                    "exclusiveMaximum": ABSOLUTE_MAX_LENGTH,
                    "description": "Maximum number of characters to return.",
                },
                "start_index": {
                    "type": "integer",
                    "default": 0,
                    "minimum": 0,
                    "description": "On return output starting at this character index, useful if a previous fetch was truncated and more context is required.",
                },
                "raw": {
                    "type": "boolean",
                    "default": false,
                    "description": "Get the actual HTML content of the requested page, without simplification.",
                },
            },
            "required": ["url"],
        }),
    }
}

// ── URL validation ──

/// Check whether a URL is permitted by the fetch config.
pub fn check_url(url: &str, config: &FetchConfig) -> Result<(), String> {
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("Invalid URL: {e}"))?;

    // Only http(s) allowed
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(format!("Scheme '{other}' is not allowed. Only http and https are permitted.")),
    }

    let host = parsed.host_str()
        .ok_or_else(|| "URL has no host".to_string())?;

    // Block loopback / private IPs
    if let Ok(ip) = host.parse::<IpAddr>() {
        if ip.is_loopback() || is_private_ip(ip) {
            return Err(format!("Access to private/loopback address {ip} is not allowed."));
        }
    }

    let host_lower = host.to_lowercase();

    // Deny list always takes priority
    for denied in &config.deny_domains {
        let denied_lower = denied.to_lowercase();
        if domain_matches(&host_lower, &denied_lower) {
            return Err(format!("Domain '{host}' is blocked by deny list."));
        }
    }

    // Allow list (if set, domain must match)
    if let Some(ref allow_list) = config.allow_domains {
        let allowed = allow_list.iter().any(|allowed| {
            let allowed_lower = allowed.to_lowercase();
            domain_matches(&host_lower, &allowed_lower)
        });
        if !allowed {
            return Err(format!("Domain '{host}' is not in the allow list."));
        }
    }

    Ok(())
}

/// Check if `host` matches `pattern` — either exact match or subdomain match.
/// e.g. "api.github.com" matches pattern "github.com".
fn domain_matches(host: &str, pattern: &str) -> bool {
    host == pattern || host.ends_with(&format!(".{pattern}"))
}

/// Check for RFC 1918 / link-local private IP ranges.
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_link_local()
                || v4.is_loopback()
                // 169.254.0.0/16 (link-local) already covered by is_link_local
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                // ::1 already covered above
                // fe80::/10 link-local
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // fc00::/7 unique-local
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

// ── Fetch execution ──

/// Fetch arguments as deserialized from the tool call.
#[derive(Debug, serde::Deserialize)]
pub struct FetchArgs {
    pub url: String,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    #[serde(default)]
    pub start_index: usize,
    #[serde(default)]
    pub raw: bool,
}

fn default_max_length() -> usize {
    DEFAULT_MAX_LENGTH
}

/// Execute a fetch request. Returns the response content or an error message.
pub async fn execute_fetch(args: &FetchArgs, config: &FetchConfig) -> Result<String, String> {
    // Validate URL
    check_url(&args.url, config)?;

    // Clamp max_length
    let max_length = args.max_length.clamp(1, ABSOLUTE_MAX_LENGTH);

    // Build HTTP client
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Nexus-Agent/1.0 (built-in fetch)"),
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_secs as u64))
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    // Send request
    let response = client
        .get(&args.url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {status} for {}", args.url));
    }

    // Read body with size limit
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body_bytes = read_body_limited(response, config.max_response_bytes).await?;
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    // Convert to text/markdown if not raw and content is HTML
    let content = if !args.raw && is_html_content(&content_type, &body) {
        html_to_text(&body)
    } else {
        body
    };

    // Apply start_index and max_length truncation (char-based, not byte-based)
    let total_len = content.chars().count();
    let start = args.start_index.min(total_len);
    let sliced: String = content.chars().skip(start).collect();
    let sliced_len = sliced.chars().count();
    let truncated = if sliced_len > max_length {
        let cut: String = sliced.chars().take(max_length).collect();
        format!(
            "{cut}\n\n--- Truncated at {max_length} characters. Total length: {total_len}. Use start_index={} to read more. ---",
            start + max_length
        )
    } else {
        sliced
    };

    Ok(truncated)
}

/// Read the response body up to `max_bytes`, then stop.
async fn read_body_limited(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    use futures::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buf = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Error reading response body: {e}"))?;
        buf.extend_from_slice(&chunk);
        if buf.len() >= max_bytes {
            buf.truncate(max_bytes);
            break;
        }
    }

    Ok(buf)
}

/// Heuristic: is this HTML content?
fn is_html_content(content_type: &str, body: &str) -> bool {
    content_type.contains("text/html")
        || content_type.contains("application/xhtml")
        || body.trim_start().starts_with("<!DOCTYPE")
        || body.trim_start().starts_with("<!doctype")
        || body.trim_start().starts_with("<html")
}

// ── Lightweight HTML to text conversion ──

/// Strip HTML tags and extract readable text content.
/// This is intentionally simple — no external crate dependency.
/// Handles: tag stripping, <br>/<p> → newlines, entity decoding, whitespace normalization.
fn html_to_text(html: &str) -> String {
    let mut output = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_buf = String::new();
    let mut chars = html.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                in_tag = true;
                tag_buf.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let tag_lower = tag_buf.to_lowercase();
                let tag_name = tag_lower.split_whitespace().next().unwrap_or("");

                // Track script/style blocks
                if tag_name == "script" {
                    in_script = true;
                } else if tag_name == "/script" {
                    in_script = false;
                } else if tag_name == "style" {
                    in_style = true;
                } else if tag_name == "/style" {
                    in_style = false;
                }

                // Insert newlines for block elements
                if !in_script && !in_style {
                    match tag_name {
                        "br" | "br/" => output.push('\n'),
                        "p" | "/p" | "div" | "/div" | "h1" | "h2" | "h3" | "h4" | "h5"
                        | "h6" | "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" | "li"
                        | "tr" | "/tr" | "blockquote" | "/blockquote" | "hr" | "hr/" => {
                            output.push('\n');
                        }
                        _ => {}
                    }
                }
            }
            '&' if !in_tag && !in_script && !in_style => {
                // Decode common HTML entities
                let mut entity = String::new();
                for next in chars.by_ref() {
                    if next == ';' || entity.len() > 10 {
                        break;
                    }
                    entity.push(next);
                }
                match entity.as_str() {
                    "amp" => output.push('&'),
                    "lt" => output.push('<'),
                    "gt" => output.push('>'),
                    "quot" => output.push('"'),
                    "apos" => output.push('\''),
                    "nbsp" => output.push(' '),
                    "#39" => output.push('\''),
                    "#34" => output.push('"'),
                    _ if entity.starts_with('#') => {
                        // Numeric entity
                        let num_str = if entity.starts_with("#x") || entity.starts_with("#X") {
                            u32::from_str_radix(&entity[2..], 16).ok()
                        } else {
                            entity[1..].parse::<u32>().ok()
                        };
                        if let Some(cp) = num_str.and_then(char::from_u32) {
                            output.push(cp);
                        }
                    }
                    _ => {
                        // Unknown entity — pass through
                        output.push('&');
                        output.push_str(&entity);
                        output.push(';');
                    }
                }
            }
            _ if in_tag => {
                tag_buf.push(ch);
            }
            _ if in_script || in_style => {
                // Discard content inside script/style
            }
            _ => {
                output.push(ch);
            }
        }
    }

    // Normalize whitespace: collapse runs of blank lines, trim lines
    let lines: Vec<&str> = output.lines().map(|l| l.trim()).collect();
    let mut result = String::with_capacity(output.len());
    let mut blank_count = 0;

    for line in lines {
        if line.is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(line);
            result.push('\n');
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_matches_exact() {
        assert!(domain_matches("github.com", "github.com"));
    }

    #[test]
    fn test_domain_matches_subdomain() {
        assert!(domain_matches("api.github.com", "github.com"));
    }

    #[test]
    fn test_domain_no_match() {
        assert!(!domain_matches("evil-github.com", "github.com"));
    }

    #[test]
    fn test_check_url_deny_list() {
        let config = FetchConfig {
            deny_domains: vec!["evil.com".to_string()],
            ..Default::default()
        };
        assert!(check_url("https://evil.com/path", &config).is_err());
        assert!(check_url("https://sub.evil.com/path", &config).is_err());
        assert!(check_url("https://good.com/path", &config).is_ok());
    }

    #[test]
    fn test_check_url_allow_list() {
        let config = FetchConfig {
            allow_domains: Some(vec!["github.com".to_string()]),
            ..Default::default()
        };
        assert!(check_url("https://github.com/repo", &config).is_ok());
        assert!(check_url("https://api.github.com/v1", &config).is_ok());
        assert!(check_url("https://google.com", &config).is_err());
    }

    #[test]
    fn test_check_url_deny_beats_allow() {
        let config = FetchConfig {
            allow_domains: Some(vec!["github.com".to_string()]),
            deny_domains: vec!["github.com".to_string()],
            ..Default::default()
        };
        assert!(check_url("https://github.com/repo", &config).is_err());
    }

    #[test]
    fn test_check_url_blocks_private_ip() {
        let config = FetchConfig::default();
        assert!(check_url("http://127.0.0.1:8080", &config).is_err());
        assert!(check_url("http://192.168.1.1", &config).is_err());
        assert!(check_url("http://10.0.0.1", &config).is_err());
    }

    #[test]
    fn test_check_url_blocks_non_http() {
        let config = FetchConfig::default();
        assert!(check_url("ftp://files.example.com", &config).is_err());
        assert!(check_url("file:///etc/passwd", &config).is_err());
    }

    #[test]
    fn test_html_to_text_basic() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn test_html_to_text_strips_script() {
        let html = "<p>Before</p><script>alert('xss')</script><p>After</p>";
        let text = html_to_text(html);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn test_html_to_text_entities() {
        let html = "&amp; &lt; &gt; &quot;";
        let text = html_to_text(html);
        assert_eq!(text, "& < > \"");
    }
}
