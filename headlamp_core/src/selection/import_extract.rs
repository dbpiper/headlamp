use std::path::Path;

use crate::selection::deps;

pub fn extract_import_specs(abs_path: &Path) -> Vec<String> {
    deps::ts_js::extract_import_specs(abs_path)
}
