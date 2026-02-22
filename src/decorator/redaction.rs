use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

use super::{DecoratorError, ToolTransform};

/// Scans tool output for secrets and replaces them with [REDACTED].
/// Runs as a transform (phase 1) so no decorator or model ever sees raw credentials.
pub struct RedactionTransform {
    patterns: Vec<SecretPattern>,
}

struct SecretPattern {
    name: &'static str,
    prefix: &'static str,
    /// Minimum length of the secret portion (after prefix).
    min_len: usize,
    /// Characters allowed in the secret portion.
    charset: CharSet,
}

enum CharSet {
    AlphaNum,
    AlphaNumDash,
    Hex,
    Base64,
}

impl CharSet {
    fn matches(&self, c: char) -> bool {
        match self {
            CharSet::AlphaNum => c.is_ascii_alphanumeric(),
            CharSet::AlphaNumDash => c.is_ascii_alphanumeric() || c == '-' || c == '_',
            CharSet::Hex => c.is_ascii_hexdigit(),
            CharSet::Base64 => c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=',
        }
    }
}

impl Default for RedactionTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl RedactionTransform {
    pub fn new() -> Self {
        Self {
            patterns: vec![
                // OpenAI
                SecretPattern {
                    name: "OpenAI API key",
                    prefix: "sk-",
                    min_len: 20,
                    charset: CharSet::AlphaNumDash,
                },
                // Anthropic
                SecretPattern {
                    name: "Anthropic API key",
                    prefix: "sk-ant-",
                    min_len: 20,
                    charset: CharSet::AlphaNumDash,
                },
                // GitHub PAT (classic)
                SecretPattern {
                    name: "GitHub PAT",
                    prefix: "ghp_",
                    min_len: 20,
                    charset: CharSet::AlphaNum,
                },
                // GitHub OAuth
                SecretPattern {
                    name: "GitHub OAuth",
                    prefix: "gho_",
                    min_len: 20,
                    charset: CharSet::AlphaNum,
                },
                // GitHub App
                SecretPattern {
                    name: "GitHub App token",
                    prefix: "ghu_",
                    min_len: 20,
                    charset: CharSet::AlphaNum,
                },
                // GitHub fine-grained
                SecretPattern {
                    name: "GitHub fine-grained PAT",
                    prefix: "github_pat_",
                    min_len: 20,
                    charset: CharSet::AlphaNum,
                },
                // AWS access key
                SecretPattern {
                    name: "AWS access key",
                    prefix: "AKIA",
                    min_len: 16,
                    charset: CharSet::AlphaNum,
                },
                // Stripe
                SecretPattern {
                    name: "Stripe secret key",
                    prefix: "sk_live_",
                    min_len: 20,
                    charset: CharSet::AlphaNum,
                },
                SecretPattern {
                    name: "Stripe test key",
                    prefix: "sk_test_",
                    min_len: 20,
                    charset: CharSet::AlphaNum,
                },
                // Slack
                SecretPattern {
                    name: "Slack token",
                    prefix: "xoxb-",
                    min_len: 20,
                    charset: CharSet::AlphaNumDash,
                },
                SecretPattern {
                    name: "Slack token",
                    prefix: "xoxp-",
                    min_len: 20,
                    charset: CharSet::AlphaNumDash,
                },
                // Bearer tokens in output
                SecretPattern {
                    name: "Bearer token",
                    prefix: "Bearer ",
                    min_len: 20,
                    charset: CharSet::Base64,
                },
                // Generic long hex strings (often API keys or hashes)
                SecretPattern {
                    name: "hex secret",
                    prefix: "",
                    min_len: 40,
                    charset: CharSet::Hex,
                },
            ],
        }
    }

    fn redact(&self, input: &str) -> (String, Vec<String>) {
        let mut output = input.to_string();
        let mut found = Vec::new();

        for pattern in &self.patterns {
            if pattern.prefix.is_empty() {
                // Standalone pattern (long hex strings) — scan for runs
                output = redact_long_runs(&output, pattern, &mut found);
            } else {
                output = redact_prefixed(&output, pattern, &mut found);
            }
        }

        (output, found)
    }
}

fn redact_prefixed(input: &str, pattern: &SecretPattern, found: &mut Vec<String>) -> String {
    let mut result = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(pos) = remaining.find(pattern.prefix) {
        result.push_str(&remaining[..pos]);

        let after_prefix = &remaining[pos + pattern.prefix.len()..];
        let secret_len = after_prefix
            .chars()
            .take_while(|c| pattern.charset.matches(*c))
            .count();

        if secret_len >= pattern.min_len {
            found.push(pattern.name.to_string());
            result.push_str("[REDACTED]");
            remaining = &remaining[pos + pattern.prefix.len() + secret_len..];
        } else {
            result.push_str(pattern.prefix);
            remaining = after_prefix;
        }
    }

    result.push_str(remaining);
    result
}

fn redact_long_runs(input: &str, pattern: &SecretPattern, found: &mut Vec<String>) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check if we're at the start of a long matching run
        if pattern.charset.matches(chars[i]) {
            let start = i;
            while i < chars.len() && pattern.charset.matches(chars[i]) {
                i += 1;
            }
            let run_len = i - start;

            if run_len >= pattern.min_len {
                // Check it's not just a normal word — must have mixed case or digits
                let run: String = chars[start..i].iter().collect();
                let has_digit = run.chars().any(|c| c.is_ascii_digit());
                let has_alpha = run.chars().any(|c| c.is_ascii_alphabetic());
                if has_digit && has_alpha {
                    found.push(pattern.name.to_string());
                    result.push_str("[REDACTED]");
                    continue;
                }
            }

            // Not a secret, push the original chars
            for c in &chars[start..i] {
                result.push(*c);
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

#[async_trait]
impl ToolTransform for RedactionTransform {
    fn name(&self) -> &str {
        "redaction"
    }

    fn applies_to(&self, _tool_name: &str, _input: &Value) -> bool {
        true // always scan
    }

    async fn transform(
        &self,
        tool_name: &str,
        _input: &Value,
        output: String,
    ) -> Result<String, DecoratorError> {
        let (redacted, found) = self.redact(&output);

        if !found.is_empty() {
            info!(
                tool = tool_name,
                redacted_count = found.len(),
                types = ?found,
                "redacted secrets from tool output"
            );
        }

        Ok(redacted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> RedactionTransform {
        RedactionTransform::new()
    }

    #[test]
    fn redacts_openai_key() {
        let input = "api_key = sk-abc123def456ghi789jkl012mno345";
        let (output, found) = redactor().redact(input);
        assert!(output.contains("[REDACTED]"), "got: {output}");
        assert!(!output.contains("abc123"), "secret leaked: {output}");
        assert!(!found.is_empty());
    }

    #[test]
    fn redacts_github_pat() {
        let input = "token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let (output, found) = redactor().redact(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("ABCDEFGHIJ"));
        assert!(!found.is_empty());
    }

    #[test]
    fn redacts_aws_key() {
        let input = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE1";
        let (output, found) = redactor().redact(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("IOSFODNN7EXAMPLE1"));
        assert!(!found.is_empty());
    }

    #[test]
    fn preserves_short_strings() {
        let input = "sk-short";
        let (output, found) = redactor().redact(input);
        assert_eq!(output, input, "short string should not be redacted");
        assert!(found.is_empty());
    }

    #[test]
    fn preserves_normal_text() {
        let input = "This is a normal line of code with no secrets.";
        let (output, _) = redactor().redact(input);
        assert_eq!(output, input);
    }

    #[test]
    fn redacts_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9abcdef";
        let (output, found) = redactor().redact(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("eyJhbGci"));
        assert!(!found.is_empty());
    }

    #[test]
    fn redacts_multiple_secrets() {
        let input = "key1=sk-abc123def456ghi789jkl012mno345 key2=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let (output, found) = redactor().redact(input);
        assert_eq!(output.matches("[REDACTED]").count(), 2, "got: {output}");
        assert!(found.len() >= 2);
    }

    #[test]
    fn redacts_anthropic_key() {
        let input = "ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwxyz123456";
        let (output, found) = redactor().redact(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!found.is_empty());
    }
}
