use std::io::Write;

use tempfile::NamedTempFile;

#[test]
fn selection_extract_import_specs_from_file() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        "import {{ x }} from './x';\nconst y = require('../y');\nexport * from \"@scope/z\";\n"
    )
    .unwrap();
    let path = temp_file.path().to_path_buf();

    let specs = headlamp_core::selection::import_extract::extract_import_specs(&path);
    assert!(specs.iter().any(|s| s == "./x"));
    assert!(specs.iter().any(|s| s == "../y"));
    assert!(specs.iter().any(|s| s == "@scope/z"));
}
