use std::path::{Path, PathBuf};

use crate::selection::deps;

pub fn try_resolve_file(candidate_base: &Path) -> Option<PathBuf> {
    deps::ts_js::try_resolve_file(candidate_base)
}

pub fn resolve_import_with_root(from_file: &Path, spec: &str, root_dir: &Path) -> Option<PathBuf> {
    deps::ts_js::resolve_import_with_root(from_file, spec, root_dir)
}
