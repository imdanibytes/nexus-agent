/// Map file extension to LSP language ID.
pub fn language_id_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("rust"),
        "py" | "pyi" => Some("python"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" => Some("typescriptreact"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("javascriptreact"),
        "go" => Some("go"),
        "c" | "h" => Some("c"),
        "cpp" | "cxx" | "cc" | "hpp" | "hxx" => Some("cpp"),
        "lua" => Some("lua"),
        "java" => Some("java"),
        "rb" => Some("ruby"),
        "swift" => Some("swift"),
        "kt" | "kts" => Some("kotlin"),
        "zig" => Some("zig"),
        "ex" | "exs" => Some("elixir"),
        _ => None,
    }
}

/// Map file path to language ID based on extension.
pub fn language_id_for_path(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())?;
    language_id_for_extension(ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_files() {
        assert_eq!(language_id_for_extension("rs"), Some("rust"));
    }

    #[test]
    fn typescript_variants() {
        assert_eq!(language_id_for_extension("ts"), Some("typescript"));
        assert_eq!(language_id_for_extension("tsx"), Some("typescriptreact"));
        assert_eq!(language_id_for_extension("mts"), Some("typescript"));
    }

    #[test]
    fn javascript_variants() {
        assert_eq!(language_id_for_extension("js"), Some("javascript"));
        assert_eq!(language_id_for_extension("jsx"), Some("javascriptreact"));
        assert_eq!(language_id_for_extension("cjs"), Some("javascript"));
    }

    #[test]
    fn python_files() {
        assert_eq!(language_id_for_extension("py"), Some("python"));
        assert_eq!(language_id_for_extension("pyi"), Some("python"));
    }

    #[test]
    fn cpp_variants() {
        assert_eq!(language_id_for_extension("cpp"), Some("cpp"));
        assert_eq!(language_id_for_extension("hpp"), Some("cpp"));
        assert_eq!(language_id_for_extension("h"), Some("c"));
    }

    #[test]
    fn unknown_extension_returns_none() {
        assert_eq!(language_id_for_extension("txt"), None);
        assert_eq!(language_id_for_extension("md"), None);
        assert_eq!(language_id_for_extension("json"), None);
    }

    #[test]
    fn path_to_language_id() {
        assert_eq!(language_id_for_path("/home/user/src/main.rs"), Some("rust"));
        assert_eq!(language_id_for_path("app.tsx"), Some("typescriptreact"));
        assert_eq!(language_id_for_path("README.md"), None);
    }

    #[test]
    fn path_without_extension_returns_none() {
        assert_eq!(language_id_for_path("Makefile"), None);
        assert_eq!(language_id_for_path("/bin/bash"), None);
    }
}
