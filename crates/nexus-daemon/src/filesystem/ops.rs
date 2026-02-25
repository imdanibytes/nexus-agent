use std::fs;
use std::path::Path;

use super::validate::PathValidator;

// ── Args types ──

#[derive(Debug, serde::Deserialize)]
pub struct EditOp {
    #[serde(rename = "oldText")]
    pub old_text: String,
    #[serde(rename = "newText")]
    pub new_text: String,
}

// ── Read operations ──

pub fn read_text_file(
    validator: &PathValidator,
    path: &str,
    head: Option<usize>,
    tail: Option<usize>,
) -> Result<String, String> {
    let resolved = validator.validate_existing(path)?;
    let content =
        fs::read_to_string(&resolved).map_err(|e| format!("Failed to read '{}': {}", path, e))?;

    if let Some(n) = head {
        let lines: Vec<&str> = content.lines().take(n).collect();
        return Ok(lines.join("\n"));
    }
    if let Some(n) = tail {
        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(n);
        return Ok(all_lines[start..].join("\n"));
    }

    Ok(content)
}

pub fn read_media_file(validator: &PathValidator, path: &str) -> Result<String, String> {
    let resolved = validator.validate_existing(path)?;
    let bytes =
        fs::read(&resolved).map_err(|e| format!("Failed to read '{}': {}", path, e))?;

    let mime = mime_from_extension(&resolved);
    let encoded = base64_encode(&bytes);

    Ok(format!(
        "data:{};base64,{}\n\n(Binary file: {} bytes, type: {})",
        mime,
        encoded,
        bytes.len(),
        mime
    ))
}

pub fn read_multiple_files(
    validator: &PathValidator,
    paths: &[String],
) -> Result<String, String> {
    let mut sections = Vec::with_capacity(paths.len());
    for p in paths {
        match read_text_file(validator, p, None, None) {
            Ok(content) => sections.push(format!("--- {} ---\n{}", p, content)),
            Err(e) => sections.push(format!("--- {} ---\nError: {}", p, e)),
        }
    }
    Ok(sections.join("\n\n"))
}

// ── Write operations ──

pub fn write_file(
    validator: &PathValidator,
    path: &str,
    content: &str,
) -> Result<String, String> {
    let resolved = validator.validate(path)?;

    // Ensure parent directory exists
    if let Some(parent) = resolved.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directories: {}", e))?;
    }

    // Atomic write: temp file → rename (prevents TOCTOU races)
    let temp_path = resolved.with_extension("nexus-write-tmp");
    fs::write(&temp_path, content)
        .map_err(|e| format!("Failed to write '{}': {}", path, e))?;
    fs::rename(&temp_path, &resolved).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to finalize write to '{}': {}", path, e)
    })?;

    Ok(format!("Successfully wrote to {}", path))
}

pub fn edit_file(
    validator: &PathValidator,
    path: &str,
    edits: &[EditOp],
    dry_run: bool,
) -> Result<String, String> {
    let resolved = validator.validate_existing(path)?;
    let original =
        fs::read_to_string(&resolved).map_err(|e| format!("Failed to read '{}': {}", path, e))?;

    let mut content = original.clone();
    let mut results = Vec::with_capacity(edits.len());
    let mut any_applied = false;

    for (i, edit) in edits.iter().enumerate() {
        if let Some(pos) = content.find(&edit.old_text) {
            content = format!(
                "{}{}{}",
                &content[..pos],
                edit.new_text,
                &content[pos + edit.old_text.len()..]
            );
            any_applied = true;
            results.push(format!(
                "Edit {}/{}: Applied\n  -{}\n  +{}",
                i + 1,
                edits.len(),
                truncate_display(&edit.old_text, 200),
                truncate_display(&edit.new_text, 200),
            ));
        } else {
            results.push(format!(
                "Edit {}/{}: FAILED — oldText not found in file\n  oldText: {}",
                i + 1,
                edits.len(),
                truncate_display(&edit.old_text, 200),
            ));
        }
    }

    if !dry_run && any_applied {
        let temp_path = resolved.with_extension("nexus-edit-tmp");
        fs::write(&temp_path, &content)
            .map_err(|e| format!("Failed to write '{}': {}", path, e))?;
        fs::rename(&temp_path, &resolved).map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            format!("Failed to finalize edit to '{}': {}", path, e)
        })?;
    }

    let prefix = if dry_run { "DRY RUN — " } else { "" };
    Ok(format!("{}File: {}\n\n{}", prefix, path, results.join("\n\n")))
}

// ── Directory operations ──

pub fn create_directory(validator: &PathValidator, path: &str) -> Result<String, String> {
    let resolved = validator.validate(path)?;
    fs::create_dir_all(&resolved)
        .map_err(|e| format!("Failed to create directory '{}': {}", path, e))?;
    Ok(format!("Successfully created directory {}", path))
}

pub fn list_directory(validator: &PathValidator, path: &str) -> Result<String, String> {
    let resolved = validator.validate_existing(path)?;
    let mut entries: Vec<_> = fs::read_dir(&resolved)
        .map_err(|e| format!("Failed to read directory '{}': {}", path, e))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut lines = Vec::with_capacity(entries.len());
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let prefix = if is_dir { "[DIR]  " } else { "[FILE] " };
        lines.push(format!("{}{}", prefix, name));
    }

    Ok(lines.join("\n"))
}

pub fn list_directory_with_sizes(
    validator: &PathValidator,
    path: &str,
    sort_by: Option<&str>,
) -> Result<String, String> {
    let resolved = validator.validate_existing(path)?;
    let entries: Vec<_> = fs::read_dir(&resolved)
        .map_err(|e| format!("Failed to read directory '{}': {}", path, e))?
        .filter_map(|e| e.ok())
        .collect();

    let mut items: Vec<(String, bool, u64)> = entries
        .iter()
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = entry.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            (name, is_dir, size)
        })
        .collect();

    match sort_by {
        Some("size") => items.sort_by(|a, b| b.2.cmp(&a.2)),
        _ => items.sort_by(|a, b| a.0.cmp(&b.0)),
    }

    let mut lines = Vec::with_capacity(items.len());
    for (name, is_dir, size) in &items {
        if *is_dir {
            lines.push(format!("[DIR]  {}/", name));
        } else {
            lines.push(format!("[FILE] {:>10}  {}", format_size(*size), name));
        }
    }

    Ok(lines.join("\n"))
}

pub fn directory_tree(
    validator: &PathValidator,
    path: &str,
    exclude_patterns: &[String],
) -> Result<String, String> {
    let resolved = validator.validate_existing(path)?;
    let name = resolved
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    let mut output = format!("{}/\n", name);
    let mut entry_count: usize = 0;
    build_tree(
        &resolved,
        "",
        exclude_patterns,
        &mut output,
        0,
        10,
        &mut entry_count,
        10_000,
    );

    if entry_count >= 10_000 {
        output.push_str("\n... (truncated at 10,000 entries)\n");
    }

    Ok(output)
}

// ── File management ──

pub fn move_file(
    validator: &PathValidator,
    source: &str,
    destination: &str,
) -> Result<String, String> {
    let src = validator.validate_existing(source)?;
    let dst = validator.validate(destination)?;

    // Ensure destination parent exists
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create destination directory: {}", e))?;
    }

    fs::rename(&src, &dst).map_err(|e| format!("Failed to move '{}' to '{}': {}", source, destination, e))?;
    Ok(format!("Successfully moved {} to {}", source, destination))
}

pub fn search_files(
    validator: &PathValidator,
    path: &str,
    pattern: &str,
    exclude_patterns: &[String],
) -> Result<String, String> {
    let resolved = validator.validate_existing(path)?;
    let mut results = Vec::new();

    search_recursive(
        &resolved,
        pattern,
        exclude_patterns,
        &mut results,
        &resolved,
        200,
    );

    if results.is_empty() {
        Ok(format!("No files found matching '{}'", pattern))
    } else {
        let count = results.len();
        let truncated = if count >= 200 {
            "\n\n(Results truncated at 200 matches)"
        } else {
            ""
        };
        Ok(format!(
            "Found {} match{}:\n{}{}",
            count,
            if count == 1 { "" } else { "es" },
            results.join("\n"),
            truncated
        ))
    }
}

pub fn get_file_info(validator: &PathValidator, path: &str) -> Result<String, String> {
    let resolved = validator.validate_existing(path)?;
    let meta =
        fs::metadata(&resolved).map_err(|e| format!("Failed to get info for '{}': {}", path, e))?;

    let file_type = if meta.is_dir() {
        "directory"
    } else if meta.is_symlink() {
        "symlink"
    } else {
        "file"
    };

    let mut info = vec![
        format!("Path: {}", resolved.display()),
        format!("Type: {}", file_type),
        format!("Size: {} ({})", meta.len(), format_size(meta.len())),
    ];

    if let Ok(modified) = meta.modified() {
        let dt: chrono::DateTime<chrono::Utc> = modified.into();
        info.push(format!("Modified: {}", dt.to_rfc3339()));
    }
    if let Ok(accessed) = meta.accessed() {
        let dt: chrono::DateTime<chrono::Utc> = accessed.into();
        info.push(format!("Accessed: {}", dt.to_rfc3339()));
    }
    if let Ok(created) = meta.created() {
        let dt: chrono::DateTime<chrono::Utc> = created.into();
        info.push(format!("Created: {}", dt.to_rfc3339()));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        info.push(format!("Permissions: {:o}", meta.permissions().mode() & 0o777));
    }

    Ok(info.join("\n"))
}

pub fn list_allowed_directories(validator: &PathValidator) -> String {
    let dirs: Vec<String> = validator
        .allowed_dirs()
        .iter()
        .map(|d| d.display().to_string())
        .collect();

    if dirs.is_empty() {
        "No allowed directories configured.".to_string()
    } else {
        format!(
            "Allowed directories:\n{}",
            dirs.iter()
                .map(|d| format!("  {}", d))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

// ── Private helpers ──

fn truncate_display(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}... ({} chars)", &s[..max], s.len())
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn mime_from_extension(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// Simple base64 encoder (avoids adding an external crate for one tool).
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::with_capacity(4 * (data.len() / 3 + 1));
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        result.push(if chunk.len() > 1 {
            CHARS[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            CHARS[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    result
}

fn should_exclude(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if name == pattern {
            return true;
        }
        // Simple glob: *.ext
        if let Some(suffix) = pattern.strip_prefix('*') {
            if name.ends_with(suffix) {
                return true;
            }
        }
    }
    false
}

fn build_tree(
    dir: &Path,
    prefix: &str,
    exclude: &[String],
    output: &mut String,
    depth: usize,
    max_depth: usize,
    entry_count: &mut usize,
    max_entries: usize,
) {
    if depth >= max_depth || *entry_count >= max_entries {
        return;
    }

    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    let mut entries: Vec<_> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| !should_exclude(&e.file_name().to_string_lossy(), exclude))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let total = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        if *entry_count >= max_entries {
            return;
        }
        *entry_count += 1;

        let name = entry.file_name().to_string_lossy().to_string();
        let is_last = i == total - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let suffix = if is_dir { "/" } else { "" };

        output.push_str(&format!("{}{}{}{}\n", prefix, connector, name, suffix));

        if is_dir {
            build_tree(
                &entry.path(),
                &format!("{}{}", prefix, child_prefix),
                exclude,
                output,
                depth + 1,
                max_depth,
                entry_count,
                max_entries,
            );
        }
    }
}

fn search_recursive(
    dir: &Path,
    pattern: &str,
    exclude: &[String],
    results: &mut Vec<String>,
    base: &Path,
    limit: usize,
) {
    if results.len() >= limit {
        return;
    }

    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    let pattern_lower = pattern.to_lowercase();

    for entry in read_dir.filter_map(|e| e.ok()) {
        if results.len() >= limit {
            return;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if should_exclude(&name, exclude) {
            continue;
        }

        // Case-insensitive substring match on filename
        if name.to_lowercase().contains(&pattern_lower) {
            let rel = entry
                .path()
                .strip_prefix(base)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| entry.path().display().to_string());
            results.push(rel);
        }

        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            search_recursive(&entry.path(), pattern, exclude, results, base, limit);
        }
    }
}
