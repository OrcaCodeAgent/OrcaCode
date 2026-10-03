use std::path::{Component, Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    pub path: PathBuf,
    pub inside_workspace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
}

pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn resolve_path(workspace: &Path, input: &str) -> Result<ResolvedPath, PathError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(PathError::Empty);
    }
    let raw = PathBuf::from(trimmed);
    let joined = if raw.is_absolute() {
        raw
    } else {
        workspace.join(raw)
    };
    let normalized = normalize(&joined);
    let workspace_norm = normalize(workspace);
    let mut inside = is_within(&normalized, &workspace_norm);
    let mut path = normalized;
    if let Ok(workspace_canon) = workspace.canonicalize() {
        if let Ok(canonical) = path.canonicalize() {
            inside = is_within(&canonical, &workspace_canon);
            path = canonical;
        } else if let Some(parent) = nearest_existing(path.as_path()) {
            if let Ok(parent_canon) = parent.canonicalize() {
                if !is_within(&parent_canon, &workspace_canon) {
                    inside = false;
                }
            }
        }
    }
    path = match_unicode_name(path);
    Ok(ResolvedPath {
        path,
        inside_workspace: inside,
    })
}

fn match_unicode_name(path: PathBuf) -> PathBuf {
    if path.exists() {
        return path;
    }
    let Some(parent) = path.parent() else {
        return path;
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return path;
    };
    let wanted: String = name.nfc().collect();
    let Ok(entries) = std::fs::read_dir(parent) else {
        return path;
    };
    for entry in entries.flatten() {
        let found = entry.file_name();
        let found = found.to_string_lossy();
        let folded: String = found.nfc().collect();
        if folded == wanted {
            return parent.join(entry.file_name());
        }
    }
    path
}

fn nearest_existing(path: &Path) -> Option<&Path> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.exists() {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

pub fn is_within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

pub fn is_critical_path(path: &Path) -> bool {
    let normalized = normalize(path);
    if normalized == Path::new("/") {
        return true;
    }
    let critical = [
        "/System",
        "/usr",
        "/bin",
        "/sbin",
        "/etc",
        "/var",
        "/private",
        "/Library",
        "/Applications",
        "/Volumes",
    ];
    if critical.iter().any(|item| normalized == Path::new(item)) {
        return true;
    }
    if let Some(home) = dirs::home_dir() {
        if normalized == normalize(&home) {
            return true;
        }
    }
    false
}

pub fn is_sensitive_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name == ".env" || name.starts_with(".env.") {
        return true;
    }
    if matches!(
        name.as_str(),
        "id_rsa" | "id_ed25519" | "id_ecdsa" | "id_dsa" | "credentials" | "credentials.json"
    ) {
        return true;
    }
    if name.ends_with(".pem") || name.ends_with(".p12") || name.ends_with(".key") {
        return true;
    }
    path.components().any(|component| {
        let text = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            text.as_str(),
            ".ssh" | ".aws" | ".gnupg" | "keychains" | ".netrc"
        )
    })
}

pub fn relative_display(workspace: &Path, path: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub const SKIP_DIRECTORIES: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "target",
    "venv",
    ".venv",
    "__pycache__",
    ".next",
    ".turbo",
    "coverage",
    "out",
    "vendor",
    ".orca",
];

pub fn should_skip_dir(name: &str) -> bool {
    if SKIP_DIRECTORIES.contains(&name) {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".photoslibrary") || lower.ends_with(".app") || lower.ends_with(".bundle")
}

pub fn should_skip_entry(path: &Path, root: &Path) -> bool {
    if path == root {
        return false;
    }
    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        if should_skip_dir(name) {
            return true;
        }
    }
    if home_system_root(root) {
        return false;
    }
    home_system_dir(path)
}

fn home_system_root(path: &Path) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let library = home.join("Library");
    path == library || path.starts_with(&library)
}

fn home_system_dir(path: &Path) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let library = home.join("Library");
    path == library || path.starts_with(&library) || path == home.join("Mail") || path == home.join(".Trash")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_parent_escape() {
        let root = Path::new("/tmp/locus-workspace");
        let resolved = resolve_path(root, "../outside.txt").expect("path");
        assert!(!resolved.inside_workspace);
    }

    #[test]
    fn keeps_relative_path_inside() {
        let root = Path::new("/tmp/locus-workspace");
        let resolved = resolve_path(root, "src/main.ts").expect("path");
        assert!(resolved.inside_workspace);
        assert_eq!(resolved.path, PathBuf::from("/tmp/locus-workspace/src/main.ts"));
    }

    #[test]
    fn absolute_inside_workspace_stays_inside() {
        let root = Path::new("/tmp/locus-workspace");
        let resolved = resolve_path(root, "/tmp/locus-workspace/src/lib.rs").expect("path");
        assert!(resolved.inside_workspace);
    }

    #[test]
    fn absolute_outside_workspace_is_flagged() {
        let root = Path::new("/tmp/locus-workspace");
        let resolved = resolve_path(root, "/etc/hosts").expect("path");
        assert!(!resolved.inside_workspace);
    }

    #[test]
    fn existing_symlink_escape_is_outside() {
        let unique = format!("locus-path-test-{}", std::process::id());
        let root = std::env::temp_dir().join(unique);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).expect("dir");
        let outside = std::env::temp_dir().join(format!("locus-outside-{}", std::process::id()));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).expect("outside");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("nested/link")).expect("symlink");
            let resolved = resolve_path(&root, "nested/link/file.txt").expect("path");
            assert!(!resolved.inside_workspace);
        }
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }
}
