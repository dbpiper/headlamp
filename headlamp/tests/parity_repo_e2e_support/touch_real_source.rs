use std::path::{Path, PathBuf};

use crate::parity_repo_e2e_support_common::TempFile;

pub fn touch_real_source_file_used_by_tests(repo: &Path) -> Option<TempFile> {
    let test_roots = [repo.join("tests"), repo.join("__tests__")];
    let test_files = test_roots
        .iter()
        .filter(|path| path.exists())
        .flat_map(|root| {
            std::fs::read_dir(root)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.contains(".test.") || name.contains(".spec."))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    for test_file in test_files {
        let Ok(text) = std::fs::read_to_string(&test_file) else {
            continue;
        };

        for import_path in extract_relative_imports(&text) {
            let Some(resolved) = resolve_relative_module(repo, test_file.parent()?, &import_path)
            else {
                continue;
            };
            if resolved.exists() && resolved.is_file() {
                let Ok(existing) = std::fs::read(&resolved) else {
                    continue;
                };
                let mut next = existing;
                next.extend_from_slice(b"\n");
                return TempFile::create_or_replace(resolved, &next);
            }
        }
    }
    None
}

fn extract_relative_imports(text: &str) -> Vec<String> {
    let mut out = vec![];
    for needle in ["from '", "from \"", "require('", "require(\""] {
        let mut rest = text;
        while let Some(i) = rest.find(needle) {
            rest = &rest[i + needle.len()..];
            let end_quote = if needle.ends_with('\"') { '\"' } else { '\'' };
            let Some(j) = rest.find(end_quote) else { break };
            let candidate = &rest[..j];
            if candidate.starts_with("./") || candidate.starts_with("../") {
                out.push(candidate.to_string());
            }
            rest = &rest[j + 1..];
        }
    }
    out
}

fn resolve_relative_module(repo: &Path, base_dir: &Path, import_path: &str) -> Option<PathBuf> {
    let joined = base_dir.join(import_path);
    let joined = joined
        .strip_prefix(repo)
        .ok()
        .map(|p| repo.join(p))
        .unwrap_or(joined);

    if joined.extension().is_some() {
        return Some(joined);
    }

    for ext in ["js", "jsx", "ts", "tsx", "cjs", "mjs"] {
        let candidate = joined.with_extension(ext);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    for ext in ["js", "jsx", "ts", "tsx", "cjs", "mjs"] {
        let candidate = joined.join(format!("index.{ext}"));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    Some(joined)
}
