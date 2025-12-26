use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::project::markers::{ProjectMarker, ProjectRoot, find_project_root};
use crate::project::rust_manifest::RustManifestPaths;
use crate::project::scan;
use crate::project::ts_js_manifest::TsJsManifestClassifier;
use crate::selection::dependency_language::DependencyLanguageId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Test,
    Production,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ProjectClassifier {
    language: DependencyLanguageId,
    project_root: Option<ProjectRoot>,
    rust_manifest_paths: Option<RustManifestPaths>,
    ts_js_manifest: Option<TsJsManifestClassifier>,
    cache_by_abs: HashMap<PathBuf, FileKind>,
}

impl ProjectClassifier {
    pub fn for_path(language: DependencyLanguageId, any_path: &Path) -> Self {
        let project_root = find_project_root(any_path);
        let rust_manifest_paths = project_root
            .as_ref()
            .filter(|p| matches!(p.marker, ProjectMarker::CargoToml))
            .and_then(|p| RustManifestPaths::read_from(p.root_dir.join("Cargo.toml")).ok());

        let ts_js_manifest = project_root
            .as_ref()
            .filter(|p| matches!(p.marker, ProjectMarker::PackageJson))
            .and_then(|p| TsJsManifestClassifier::read_from(p.root_dir.clone()));

        Self {
            language,
            project_root,
            rust_manifest_paths,
            ts_js_manifest,
            cache_by_abs: HashMap::new(),
        }
    }

    pub fn project_root_dir(&self) -> Option<&Path> {
        self.project_root.as_ref().map(|p| p.root_dir.as_path())
    }

    pub fn classify_abs_path(&mut self, abs_path: &Path) -> FileKind {
        if let Some(cached) = self.cache_by_abs.get(abs_path).copied() {
            return cached;
        }
        let kind = self.classify_uncached(abs_path);
        self.cache_by_abs.insert(abs_path.to_path_buf(), kind);
        kind
    }

    fn classify_uncached(&self, abs_path: &Path) -> FileKind {
        match self.language {
            DependencyLanguageId::Rust => self.classify_rust(abs_path),
            DependencyLanguageId::TsJs => self.classify_ts_js(abs_path),
        }
    }

    fn classify_rust(&self, abs_path: &Path) -> FileKind {
        if abs_path.extension().and_then(|e| e.to_str()) != Some("rs") {
            return FileKind::Unknown;
        }

        let manifest_kind = self
            .rust_manifest_paths
            .as_ref()
            .and_then(|m| m.classify(abs_path));
        if let Some(kind) = manifest_kind {
            return kind;
        }

        scan::rust::classify_by_content(abs_path)
    }

    fn classify_ts_js(&self, abs_path: &Path) -> FileKind {
        let ext = abs_path.extension().and_then(|e| e.to_str());
        let is_ts_js = matches!(
            ext,
            Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts")
        );
        if !is_ts_js {
            return FileKind::Unknown;
        }

        let manifest_kind = self
            .ts_js_manifest
            .as_ref()
            .and_then(|m| m.classify(abs_path));
        manifest_kind.unwrap_or_else(|| scan::ts_js::classify_by_content(abs_path))
    }
}
