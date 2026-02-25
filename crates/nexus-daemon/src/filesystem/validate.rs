use std::path::{Path, PathBuf};

/// Resolved set of allowed directories for filesystem sandboxing.
///
/// At construction, each configured directory is canonicalized (symlinks resolved,
/// e.g. macOS `/tmp` → `/private/tmp`).  Every path accessed through the built-in
/// filesystem tools must pass validation against this set.
#[derive(Debug, Clone)]
pub struct PathValidator {
    /// (original_path, canonical_path) — accepts paths matching either form.
    allowed: Vec<(PathBuf, PathBuf)>,
}

impl PathValidator {
    /// Create a new validator.  Non-existent directories are warned and skipped.
    pub fn new(dirs: &[String]) -> Self {
        let mut allowed = Vec::new();
        for dir in dirs {
            let original = PathBuf::from(dir);
            match std::fs::canonicalize(&original) {
                Ok(canonical) => {
                    if original != canonical {
                        tracing::debug!(
                            original = %original.display(),
                            canonical = %canonical.display(),
                            "Resolved symlink for allowed directory"
                        );
                    }
                    allowed.push((original, canonical));
                }
                Err(e) => {
                    tracing::warn!(
                        dir = %original.display(),
                        error = %e,
                        "Allowed directory does not exist, skipping"
                    );
                }
            }
        }
        Self { allowed }
    }

    /// The original allowed directory paths.
    pub fn allowed_dirs(&self) -> Vec<&Path> {
        self.allowed.iter().map(|(orig, _)| orig.as_path()).collect()
    }

    /// Validate and resolve a path to an absolute, canonical form.
    ///
    /// Security checks (mirrors the reference MCP filesystem server):
    /// 1. Strip null bytes (prevent path injection)
    /// 2. Expand `~` to home directory
    /// 3. Resolve relative paths against allowed directories
    /// 4. Canonicalize (collapses `..`, resolves symlinks)
    /// 5. Verify result is inside an allowed directory
    pub fn validate(&self, path: &str) -> Result<PathBuf, String> {
        let clean = path.replace('\0', "");
        if clean.is_empty() {
            return Err("Path is empty".into());
        }

        // Expand ~
        let expanded = if let Some(rest) = clean.strip_prefix("~/") {
            dirs::home_dir()
                .ok_or("Cannot determine home directory")?
                .join(rest)
        } else if clean == "~" {
            dirs::home_dir().ok_or("Cannot determine home directory")?
        } else {
            PathBuf::from(&clean)
        };

        // Make absolute: resolve relative paths against first matching allowed dir
        let absolute = if expanded.is_absolute() {
            expanded
        } else {
            let mut resolved = None;
            for (_, canonical) in &self.allowed {
                let candidate = canonical.join(&expanded);
                if candidate.exists() {
                    resolved = Some(candidate);
                    break;
                }
            }
            resolved.unwrap_or_else(|| {
                self.allowed
                    .first()
                    .map(|(_, c)| c.join(&expanded))
                    .unwrap_or(expanded)
            })
        };

        // Canonicalize existing paths; for new files, canonicalize the parent
        let canonical = if absolute.exists() {
            std::fs::canonicalize(&absolute)
                .map_err(|e| format!("Cannot resolve '{}': {}", path, e))?
        } else {
            let parent = absolute
                .parent()
                .ok_or_else(|| format!("Path '{}' has no parent", path))?;
            let canonical_parent = std::fs::canonicalize(parent).map_err(|e| {
                format!(
                    "Parent directory '{}' does not exist: {}",
                    parent.display(),
                    e
                )
            })?;
            let filename = absolute
                .file_name()
                .ok_or_else(|| format!("Path '{}' has no filename", path))?;
            canonical_parent.join(filename)
        };

        self.check_allowed(&canonical, path)
    }

    /// Validate an existing path (must exist on disk).
    pub fn validate_existing(&self, path: &str) -> Result<PathBuf, String> {
        let resolved = self.validate(path)?;
        if !resolved.exists() {
            return Err(format!("Path does not exist: '{}'", path));
        }
        Ok(resolved)
    }

    fn check_allowed(&self, canonical: &Path, original_input: &str) -> Result<PathBuf, String> {
        let canonical_str = canonical.to_string_lossy();
        for (_original, allowed_canonical) in &self.allowed {
            let allowed_str = allowed_canonical.to_string_lossy();
            if canonical_str == *allowed_str
                || canonical_str.starts_with(&format!("{}/", allowed_str))
            {
                return Ok(canonical.to_path_buf());
            }
        }
        Err(format!(
            "Access denied: '{}' is outside allowed directories [{}]",
            original_input,
            self.allowed
                .iter()
                .map(|(orig, _)| orig.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_path() {
        let v = PathValidator {
            allowed: vec![],
        };
        assert!(v.validate("").is_err());
    }

    #[test]
    fn rejects_when_no_allowed_dirs() {
        let v = PathValidator {
            allowed: vec![],
        };
        assert!(v.validate("/tmp/file.txt").is_err());
    }

    #[test]
    fn allows_path_within_directory() {
        let dir = std::env::temp_dir().join("nexus-test-validate");
        std::fs::create_dir_all(&dir).unwrap();
        let test_file = dir.join("file.txt");
        std::fs::write(&test_file, "content").unwrap();

        let v = PathValidator::new(&[dir.to_string_lossy().to_string()]);
        let result = v.validate(&test_file.to_string_lossy());
        assert!(result.is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn blocks_path_traversal() {
        let dir = std::env::temp_dir().join("nexus-test-traversal");
        std::fs::create_dir_all(&dir).unwrap();

        let v = PathValidator::new(&[dir.to_string_lossy().to_string()]);
        // Traversal to /etc/passwd — should always fail, either because the
        // resolved path is outside allowed dirs or because it doesn't exist.
        let result = v.validate(&format!("{}/../../../etc/passwd", dir.display()));
        assert!(result.is_err(), "Path traversal should be rejected: {:?}", result);

        // Also test a path that DOES exist but is outside allowed dirs
        let result2 = v.validate("/usr");
        assert!(result2.is_err(), "/usr should be outside allowed dirs");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn allows_new_file_in_existing_dir() {
        let dir = std::env::temp_dir().join("nexus-test-newfile");
        std::fs::create_dir_all(&dir).unwrap();

        let v = PathValidator::new(&[dir.to_string_lossy().to_string()]);
        let result = v.validate(&format!("{}/new_file.txt", dir.display()));
        assert!(result.is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn strips_null_bytes() {
        let dir = std::env::temp_dir().join("nexus-test-null");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test.txt"), "hi").unwrap();

        let v = PathValidator::new(&[dir.to_string_lossy().to_string()]);
        // Null byte in path is stripped, leaving valid path "test.txt"
        let result = v.validate(&format!("{}/te\0st.txt", dir.display()));
        assert!(result.is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }
}
