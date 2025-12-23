use std::path::{Path, PathBuf};

const FILE_EXTS: [&str; 10] = [
    "", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json",
];

pub fn try_resolve_file(candidate_base: &Path) -> Option<PathBuf> {
    for ext in FILE_EXTS {
        let full = if ext.is_empty() {
            candidate_base.to_path_buf()
        } else {
            PathBuf::from(format!("{}{}", candidate_base.to_string_lossy(), ext))
        };
        if full.exists() {
            if let Ok(meta) = std::fs::metadata(&full) {
                if meta.is_file() {
                    let canonical = dunce::canonicalize(&full).unwrap_or_else(|_| full.clone());
                    return Some(canonical);
                }
            }
        }
    }

    for ext in FILE_EXTS {
        let index_name = if ext.is_empty() {
            "index".to_string()
        } else {
            format!("index{ext}")
        };
        let full = candidate_base.join(index_name);
        if full.exists() {
            if let Ok(meta) = std::fs::metadata(&full) {
                if meta.is_file() {
                    let canonical = dunce::canonicalize(&full).unwrap_or_else(|_| full.clone());
                    return Some(canonical);
                }
            }
        }
    }
    None
}

pub fn resolve_import_with_root(from_file: &Path, spec: &str, root_dir: &Path) -> Option<PathBuf> {
    let raw = spec.trim();
    if raw.is_empty() {
        return None;
    }
    if !(raw.starts_with('.') || raw.starts_with('/')) {
        return None;
    }

    let base_dir = from_file.parent().unwrap_or(root_dir);
    let candidate_base = if raw.starts_with('/') {
        root_dir.join(raw.trim_start_matches('/'))
    } else {
        base_dir.join(raw)
    };
    try_resolve_file(&candidate_base)
}
