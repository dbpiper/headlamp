use std::path::{Path, PathBuf};

fn mk_temp_dir(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join("headlamp-core-tests").join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

#[test]
fn rust_import_extraction_completes_under_time_budget_for_large_file() {
    let repo = mk_temp_dir("dep-lang-rust-extract-perf");
    let lib_rs = repo.join("src/lib.rs");

    let mut text = String::new();
    for i in 0..15_000usize {
        text.push_str("use crate::foo::bar::baz;\n");
        if i % 10 == 0 {
            text.push_str("mod a;\n");
        }
    }
    write_file(&lib_rs, &text);

    let start = std::time::Instant::now();
    let _ = headlamp::selection::dependency_language::extract_import_specs(
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &lib_rs,
    );
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "elapsed={elapsed:?}"
    );
}
