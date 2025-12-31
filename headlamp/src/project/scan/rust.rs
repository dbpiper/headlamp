use std::path::Path;

use crate::project::classify::FileKind;

pub fn classify_by_content(abs_path: &Path) -> FileKind {
    let Ok(body) = std::fs::read_to_string(abs_path) else {
        return FileKind::Unknown;
    };
    let markers = crate::rust_parse::classify_rust_file_markers(&body);

    match (markers.has_test_attr, markers.has_cfg_test) {
        (true, true) => FileKind::Mixed,
        (true, false) => FileKind::Test,
        (false, true) => FileKind::Production,
        (false, false) => FileKind::Production,
    }
}
