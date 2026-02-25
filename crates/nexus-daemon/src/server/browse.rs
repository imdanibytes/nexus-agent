use axum::extract::Query;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct BrowseQuery {
    /// Directory to list. Defaults to the user's home directory.
    pub path: Option<String>,
}

/// List subdirectories at a path. Used by the frontend folder picker modal.
///
/// Not sandboxed by `allowed_directories` — the user needs to browse outside
/// existing dirs to add new workspaces.
pub async fn browse(
    Query(q): Query<BrowseQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let target = match &q.path {
        Some(p) if !p.is_empty() => {
            if let Some(rest) = p.strip_prefix("~/") {
                dirs::home_dir()
                    .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
                    .join(rest)
            } else if p == "~" {
                dirs::home_dir().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
            } else {
                PathBuf::from(p)
            }
        }
        _ => dirs::home_dir().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
    };

    let canonical = std::fs::canonicalize(&target).map_err(|_| StatusCode::NOT_FOUND)?;
    let parent = canonical.parent().map(|p| p.to_string_lossy().to_string());

    let mut entries: Vec<serde_json::Value> = Vec::new();
    let read_dir = std::fs::read_dir(&canonical).map_err(|_| StatusCode::FORBIDDEN)?;

    for entry in read_dir.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') {
            continue;
        }

        if !meta.is_dir() {
            continue;
        }

        entries.push(serde_json::json!({
            "name": name,
            "path": entry.path().to_string_lossy().to_string(),
        }));
    }

    entries.sort_by(|a, b| {
        let an = a["name"].as_str().unwrap_or("");
        let bn = b["name"].as_str().unwrap_or("");
        an.to_lowercase().cmp(&bn.to_lowercase())
    });

    Ok(Json(serde_json::json!({
        "path": canonical.to_string_lossy().to_string(),
        "parent": parent,
        "entries": entries,
    })))
}
