use std::path::{Path, PathBuf};

use crate::selection::deps;

#[derive(Debug, Default)]
pub struct DependencyResolveCache {
    ts_js: deps::ts_js_resolver::TsJsResolveCache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyLanguageId {
    TsJs,
    Rust,
}

impl DependencyLanguageId {
    pub fn parse(text: &str) -> Option<Self> {
        let normalized = text.trim().to_ascii_lowercase().replace('_', "-");
        match normalized.as_str() {
            "ts-js" | "tsjs" | "js-ts" | "jsts" | "javascript" | "typescript" | "js" | "ts" => {
                Some(Self::TsJs)
            }
            "rust" | "rs" => Some(Self::Rust),
            _ => None,
        }
    }
}

pub fn extract_import_specs(language: DependencyLanguageId, abs_path: &Path) -> Vec<String> {
    match language {
        DependencyLanguageId::TsJs => deps::ts_js::extract_import_specs(abs_path),
        DependencyLanguageId::Rust => deps::rust::extract_import_specs(abs_path),
    }
}

pub fn resolve_import_with_root(
    language: DependencyLanguageId,
    from_file: &Path,
    spec: &str,
    root_dir: &Path,
) -> Option<PathBuf> {
    let mut cache = DependencyResolveCache::default();
    resolve_import_with_root_cached(language, from_file, spec, root_dir, &mut cache)
}

pub fn resolve_import_with_root_cached(
    language: DependencyLanguageId,
    from_file: &Path,
    spec: &str,
    root_dir: &Path,
    cache: &mut DependencyResolveCache,
) -> Option<PathBuf> {
    match language {
        DependencyLanguageId::TsJs => deps::ts_js_resolver::TsJsImportResolver::new(root_dir)
            .resolve_import(from_file, spec, &mut cache.ts_js),
        DependencyLanguageId::Rust => {
            deps::rust::resolve_import_with_root(from_file, spec, root_dir)
        }
    }
}

pub fn looks_like_source_file(language: DependencyLanguageId, path: &Path) -> bool {
    match language {
        DependencyLanguageId::TsJs => deps::ts_js::looks_like_source_file(path),
        DependencyLanguageId::Rust => deps::rust::looks_like_source_file(path),
    }
}

pub fn build_seed_terms(
    language: DependencyLanguageId,
    repo_root: &Path,
    production_selection_paths_abs: &[String],
) -> Vec<String> {
    match language {
        DependencyLanguageId::TsJs => {
            deps::ts_js::build_seed_terms(repo_root, production_selection_paths_abs)
        }
        DependencyLanguageId::Rust => {
            deps::rust::build_seed_terms(repo_root, production_selection_paths_abs)
        }
    }
}
