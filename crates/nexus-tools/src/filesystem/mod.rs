pub mod validate;
mod ops;

pub use ops::EditOp;
pub use validate::PathValidator;

use nexus_provider::types::Tool;
use crate::config::FilesystemConfig;

// ── Tool names ──

const READ_FILE: &str = "read_file"; // deprecated alias
const READ_TEXT_FILE: &str = "read_text_file";
const READ_MEDIA_FILE: &str = "read_media_file";
const READ_MULTIPLE_FILES: &str = "read_multiple_files";
const WRITE_FILE: &str = "write_file";
const EDIT_FILE: &str = "edit_file";
const CREATE_DIRECTORY: &str = "create_directory";
const LIST_DIRECTORY: &str = "list_directory";
const LIST_DIRECTORY_WITH_SIZES: &str = "list_directory_with_sizes";
const DIRECTORY_TREE: &str = "directory_tree";
const MOVE_FILE: &str = "move_file";
const SEARCH_FILES: &str = "search_files";
const GET_FILE_INFO: &str = "get_file_info";
const LIST_ALLOWED_DIRECTORIES: &str = "list_allowed_directories";

/// Default patterns always excluded from directory_tree and search_files.
const DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules",
    ".git",
    "__pycache__",
    ".next",
    ".venv",
    "venv",
    ".tox",
    ".turbo",
    ".svn",
    ".hg",
    ".DS_Store",
    "coverage",
    ".nyc_output",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "bower_components",
    ".cache",
];

const ALL_TOOLS: &[&str] = &[
    READ_FILE,
    READ_TEXT_FILE,
    READ_MEDIA_FILE,
    READ_MULTIPLE_FILES,
    WRITE_FILE,
    EDIT_FILE,
    CREATE_DIRECTORY,
    LIST_DIRECTORY,
    LIST_DIRECTORY_WITH_SIZES,
    DIRECTORY_TREE,
    MOVE_FILE,
    SEARCH_FILES,
    GET_FILE_INFO,
    LIST_ALLOWED_DIRECTORIES,
];

// ── Public API ──

/// Check if a tool name belongs to the filesystem toolset.
pub fn is_filesystem_tool(name: &str) -> bool {
    ALL_TOOLS.contains(&name)
}

/// Return tool definitions for the filesystem toolset.
///
/// Returns an empty vec if the config is disabled or has no allowed directories
/// (the tools are meaningless without at least one allowed directory).
pub fn tool_definitions(config: &FilesystemConfig) -> Vec<Tool> {
    if !config.enabled || config.allowed_directories.is_empty() {
        return Vec::new();
    }
    vec![
        Tool {
            name: READ_TEXT_FILE.into(),
            description: "Read the complete contents of a text file. Handles UTF-8 encoded \
                files. Use head/tail to read only the beginning or end of large files."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the text file" },
                    "head": { "type": "number", "description": "Read only the first N lines" },
                    "tail": { "type": "number", "description": "Read only the last N lines" }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: READ_MEDIA_FILE.into(),
            description: "Read a media file (image, audio, video, PDF) and return it as \
                base64-encoded data with MIME type."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the media file" }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: READ_MULTIPLE_FILES.into(),
            description: "Read multiple text files simultaneously. Each file's content is \
                returned with its path as a header. Failed reads are reported inline."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "description": "List of file paths to read"
                    }
                },
                "required": ["paths"]
            }),
        },
        Tool {
            name: WRITE_FILE.into(),
            description: "Create a new file or overwrite an existing file. Creates parent \
                directories as needed. For partial modifications, prefer edit_file instead."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to write to" },
                    "content": { "type": "string", "description": "File content to write" }
                },
                "required": ["path", "content"]
            }),
        },
        Tool {
            name: EDIT_FILE.into(),
            description: "Make line-based edits to a text file. Each edit replaces an exact \
                match of oldText with newText. Use dryRun to preview changes without applying."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file to edit" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "oldText": { "type": "string", "description": "Exact text to find" },
                                "newText": { "type": "string", "description": "Replacement text" }
                            },
                            "required": ["oldText", "newText"]
                        },
                        "description": "List of edits to apply sequentially"
                    },
                    "dryRun": {
                        "type": "boolean",
                        "default": false,
                        "description": "Preview changes without writing to disk"
                    }
                },
                "required": ["path", "edits"]
            }),
        },
        Tool {
            name: CREATE_DIRECTORY.into(),
            description: "Create a new directory or ensure it exists. Creates parent directories \
                as needed (like mkdir -p). Succeeds silently if already exists."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to create" }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: LIST_DIRECTORY.into(),
            description: "List files and directories at a path. Entries are prefixed with \
                [FILE] or [DIR]."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list" }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: LIST_DIRECTORY_WITH_SIZES.into(),
            description: "List files and directories with file sizes. Can sort by name or size."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list" },
                    "sortBy": {
                        "type": "string",
                        "enum": ["name", "size"],
                        "default": "name",
                        "description": "Sort order"
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: DIRECTORY_TREE.into(),
            description: "Recursive tree view of files and directories. Common non-source \
                directories (node_modules, .git, __pycache__, .next, .venv, target, etc.) are \
                excluded by default. Use excludePatterns to add more."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Root directory for the tree" },
                    "excludePatterns": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Directory/file names to exclude (e.g. node_modules, .git)"
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: MOVE_FILE.into(),
            description: "Move or rename a file or directory. Both source and destination must \
                be within allowed directories."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Source path" },
                    "destination": { "type": "string", "description": "Destination path" }
                },
                "required": ["source", "destination"]
            }),
        },
        Tool {
            name: SEARCH_FILES.into(),
            description: "Recursively search for files matching a pattern (case-insensitive \
                filename substring match). Returns matching paths relative to the search root."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Root directory to search" },
                    "pattern": { "type": "string", "description": "Filename pattern to match" },
                    "excludePatterns": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Directory/file names to exclude"
                    }
                },
                "required": ["path", "pattern"]
            }),
        },
        Tool {
            name: GET_FILE_INFO.into(),
            description: "Get detailed metadata: size, creation time, modification time, type, \
                and permissions."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File or directory path" }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: LIST_ALLOWED_DIRECTORIES.into(),
            description: "List the directories this tool is allowed to access. Use this to \
                understand the sandbox boundaries before attempting file operations."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

// ── Dispatch ──

/// Execute a filesystem tool by name.  Returns `Ok(output)` or `Err(error_message)`.
pub fn execute(
    name: &str,
    args_json: &str,
    validator: &PathValidator,
) -> Result<String, String> {
    let raw = if args_json.is_empty() { "{}" } else { args_json };
    let args: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("Invalid arguments: {e}"))?;

    match name {
        READ_FILE | READ_TEXT_FILE => {
            let path = require_str(&args, "path")?;
            let head = args.get("head").and_then(|v| v.as_u64()).map(|n| n as usize);
            let tail = args.get("tail").and_then(|v| v.as_u64()).map(|n| n as usize);
            ops::read_text_file(validator, path, head, tail)
        }
        READ_MEDIA_FILE => {
            let path = require_str(&args, "path")?;
            ops::read_media_file(validator, path)
        }
        READ_MULTIPLE_FILES => {
            let paths: Vec<String> = args
                .get("paths")
                .and_then(|v| v.as_array())
                .ok_or("Missing required field: 'paths'")?
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if paths.is_empty() {
                return Err("'paths' must contain at least one path".into());
            }
            ops::read_multiple_files(validator, &paths)
        }
        WRITE_FILE => {
            let path = require_str(&args, "path")?;
            let content = require_str(&args, "content")?;
            ops::write_file(validator, path, content)
        }
        EDIT_FILE => {
            let path = require_str(&args, "path")?;
            let edits: Vec<EditOp> = args
                .get("edits")
                .ok_or("Missing required field: 'edits'")?
                .as_array()
                .ok_or("'edits' must be an array")?
                .iter()
                .map(|v| serde_json::from_value::<EditOp>(v.clone()))
                .collect::<Result<_, _>>()
                .map_err(|e| format!("Invalid edit: {e}"))?;
            let dry_run = args
                .get("dryRun")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            ops::edit_file(validator, path, &edits, dry_run)
        }
        CREATE_DIRECTORY => {
            let path = require_str(&args, "path")?;
            ops::create_directory(validator, path)
        }
        LIST_DIRECTORY => {
            let path = require_str(&args, "path")?;
            ops::list_directory(validator, path)
        }
        LIST_DIRECTORY_WITH_SIZES => {
            let path = require_str(&args, "path")?;
            let sort_by = args.get("sortBy").and_then(|v| v.as_str());
            ops::list_directory_with_sizes(validator, path, sort_by)
        }
        DIRECTORY_TREE => {
            let path = require_str(&args, "path")?;
            let mut exclude: Vec<String> = DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect();
            if let Some(arr) = args.get("excludePatterns").and_then(|v| v.as_array()) {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        if !exclude.contains(&s.to_string()) {
                            exclude.push(s.to_string());
                        }
                    }
                }
            }
            ops::directory_tree(validator, path, &exclude)
        }
        MOVE_FILE => {
            let source = require_str(&args, "source")?;
            let destination = require_str(&args, "destination")?;
            ops::move_file(validator, source, destination)
        }
        SEARCH_FILES => {
            let path = require_str(&args, "path")?;
            let pattern = require_str(&args, "pattern")?;
            let mut exclude: Vec<String> = DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect();
            if let Some(arr) = args.get("excludePatterns").and_then(|v| v.as_array()) {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        if !exclude.contains(&s.to_string()) {
                            exclude.push(s.to_string());
                        }
                    }
                }
            }
            ops::search_files(validator, path, pattern, &exclude)
        }
        GET_FILE_INFO => {
            let path = require_str(&args, "path")?;
            ops::get_file_info(validator, path)
        }
        LIST_ALLOWED_DIRECTORIES => Ok(ops::list_allowed_directories(validator)),
        _ => Err(format!("Unknown filesystem tool: '{}'", name)),
    }
}

fn require_str<'a>(args: &'a serde_json::Value, field: &str) -> Result<&'a str, String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required field: '{}'", field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matches_all_tools() {
        for name in ALL_TOOLS {
            assert!(is_filesystem_tool(name), "{} should be recognized", name);
        }
    }

    #[test]
    fn identity_rejects_unknown() {
        assert!(!is_filesystem_tool("fetch"));
        assert!(!is_filesystem_tool("ask_user"));
        assert!(!is_filesystem_tool("read_file_extended"));
    }

    #[test]
    fn definitions_empty_when_disabled() {
        let config = FilesystemConfig {
            enabled: false,
            allowed_directories: vec!["/tmp".into()],
        };
        assert!(tool_definitions(&config).is_empty());
    }

    #[test]
    fn definitions_empty_when_no_dirs() {
        let config = FilesystemConfig {
            enabled: true,
            allowed_directories: vec![],
        };
        assert!(tool_definitions(&config).is_empty());
    }

    #[test]
    fn definitions_returned_when_configured() {
        let config = FilesystemConfig {
            enabled: true,
            allowed_directories: vec!["/tmp".into()],
        };
        let defs = tool_definitions(&config);
        // 13 tools (read_file alias not in definitions, only handled in dispatch)
        assert_eq!(defs.len(), 13);
    }
}
