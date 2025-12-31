mod attrs;
mod imports;
mod lex;
mod types;
mod util;

pub use self::types::RustFileMarkers;

pub fn extract_import_specs_from_source(source: &str) -> std::collections::BTreeSet<String> {
    imports::extract_import_specs_from_source(source)
}

pub fn classify_rust_file_markers(source: &str) -> RustFileMarkers {
    attrs::classify_rust_file_markers(source)
}
