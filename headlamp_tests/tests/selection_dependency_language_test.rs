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

fn rg_available() -> bool {
    which::which("rg").is_ok()
}

#[test]
fn ts_js_import_extraction_finds_common_import_forms() {
    if !rg_available() {
        return;
    }

    let repo = mk_temp_dir("dep-lang-tsjs-extract");
    let file = repo.join("src/a.js");
    write_file(
        &file,
        "import { x } from './b';\nconst y = require('../c');\nexport { z } from \"./d\";\n",
    );

    let specs = headlamp::selection::dependency_language::extract_import_specs(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &file,
    );
    assert!(specs.iter().any(|s| s == "./b"));
    assert!(specs.iter().any(|s| s == "../c"));
    assert!(specs.iter().any(|s| s == "./d"));
}

#[test]
fn ts_js_import_resolution_resolves_extensions_and_index() {
    let repo = mk_temp_dir("dep-lang-tsjs-resolve");
    write_file(&repo.join("src/a.ts"), "export const a = 1;\n");
    write_file(&repo.join("src/c.js"), "exports.c = () => 1;\n");
    write_file(&repo.join("src/b/index.ts"), "export const b = 1;\n");

    let from = repo.join("src/a.ts");
    let c = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &from,
        "./c",
        &repo,
    )
    .unwrap();
    assert!(c.to_string_lossy().ends_with("src/c.js"));

    let b = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &from,
        "./b",
        &repo,
    )
    .unwrap();
    assert!(b.to_string_lossy().ends_with("src/b/index.ts"));
}

#[test]
fn rust_import_extraction_finds_mod_decls() {
    let repo = mk_temp_dir("dep-lang-rust-extract");
    let lib_rs = repo.join("src/lib.rs");
    write_file(&lib_rs, "mod a;\n\npub mod b;\n\npub fn entry() {}\n");
    write_file(&repo.join("src/a.rs"), "pub fn a() {}\n");
    write_file(&repo.join("src/b/mod.rs"), "pub fn b() {}\n");

    let specs = headlamp::selection::dependency_language::extract_import_specs(
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &lib_rs,
    );
    assert!(specs.iter().any(|s| s == "self::a"));
    assert!(specs.iter().any(|s| s == "self::b"));
}

#[test]
fn rust_import_extraction_supports_use_groups_renames_and_globs() {
    let repo = mk_temp_dir("dep-lang-rust-use-tree");
    let lib_rs = repo.join("src/lib.rs");
    write_file(
        &lib_rs,
        "use crate::{a, b::{c, d as e}, *};\n\npub fn entry() {}\n",
    );

    let specs = headlamp::selection::dependency_language::extract_import_specs(
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &lib_rs,
    );
    assert!(specs.iter().any(|s| s == "crate::a"));
    assert!(specs.iter().any(|s| s == "crate::b::c"));
    assert!(specs.iter().any(|s| s == "crate::b::d"));
    assert!(specs.iter().any(|s| s == "crate"));
}

#[test]
fn rust_import_extraction_supports_path_attribute_mod() {
    let repo = mk_temp_dir("dep-lang-rust-path-attr");
    let lib_rs = repo.join("src/lib.rs");
    write_file(&lib_rs, "#[path = \"alt/a.rs\"]\nmod a;\n");
    write_file(&repo.join("src/alt/a.rs"), "pub fn a() {}\n");

    let specs = headlamp::selection::dependency_language::extract_import_specs(
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &lib_rs,
    );
    assert!(specs.iter().any(|s| s == "path:alt/a.rs"));
}

#[test]
fn rust_import_resolution_resolves_mod_files() {
    let repo = mk_temp_dir("dep-lang-rust-resolve");
    write_file(&repo.join("src/lib.rs"), "mod a;\nmod b;\n");
    write_file(&repo.join("src/a.rs"), "pub fn a() {}\n");
    write_file(&repo.join("src/b/mod.rs"), "pub fn b() {}\n");

    let from = repo.join("src/lib.rs");
    let a = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &from,
        "a",
        &repo,
    )
    .unwrap();
    assert!(a.to_string_lossy().ends_with("src/a.rs"));

    let b = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &from,
        "b",
        &repo,
    )
    .unwrap();
    assert!(b.to_string_lossy().ends_with("src/b/mod.rs"));
}

#[test]
fn transitive_seed_refine_respects_language_specific_seed_terms() {
    let repo = mk_temp_dir("dep-lang-transitive-seed");
    let seed = repo.join("src/a.rs");
    let test = repo.join("tests/uses_a.rs");

    write_file(&seed, "pub fn a() {}\n");
    write_file(&test, "fn uses_a() { let _ = \"a\"; }\n");

    let seed_abs = seed.to_string_lossy().to_string();
    let test_abs = test.to_string_lossy().to_string();

    let kept_tsjs = headlamp::selection::transitive_seed_refine::filter_tests_by_transitive_seed(
        &repo,
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        std::slice::from_ref(&test_abs),
        std::slice::from_ref(&seed_abs),
        headlamp::selection::transitive_seed_refine::MaxDepth(0),
    );
    assert!(kept_tsjs.is_empty());

    let kept_rust = headlamp::selection::transitive_seed_refine::filter_tests_by_transitive_seed(
        &repo,
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        std::slice::from_ref(&test_abs),
        std::slice::from_ref(&seed_abs),
        headlamp::selection::transitive_seed_refine::MaxDepth(0),
    );
    assert!(kept_rust.iter().any(|p| p.ends_with("tests/uses_a.rs")));
}

#[test]
fn related_tests_rust_finds_integration_test_importing_crate_by_name() {
    let repo = mk_temp_dir("dep-lang-related-rust");

    write_file(
        &repo.join("Cargo.toml"),
        "[package]\nname = \"demo_pkg\"\nversion = \"0.1.0\"\n",
    );
    write_file(&repo.join("src/lib.rs"), "pub mod a;\n");
    write_file(&repo.join("src/a.rs"), "pub fn a() -> i32 { 1 }\n");
    write_file(
        &repo.join("tests/a_test.rs"),
        "use demo_pkg::a;\n\n#[test]\nfn ok() { assert_eq!(a::a(), 1); }\n",
    );

    let extracted = headlamp::selection::dependency_language::extract_import_specs(
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &repo.join("tests/a_test.rs"),
    );
    assert!(extracted.iter().any(|s| s == "demo_pkg::a"));

    let resolved = headlamp::selection::dependency_language::resolve_import_with_root(
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &repo.join("tests/a_test.rs"),
        "demo_pkg::a",
        &repo,
    )
    .unwrap();
    assert!(resolved.to_string_lossy().ends_with("src/a.rs"));

    let seed = repo.join("src/a.rs").to_string_lossy().to_string();
    let selection = headlamp::selection::related_tests::select_related_tests(
        &repo,
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &[seed],
        &[],
    );
    let selected = selection.selected_test_paths_abs.join("\n");
    assert!(
        selected.contains("tests/a_test.rs"),
        "selected was:\n{selected}"
    );
}
