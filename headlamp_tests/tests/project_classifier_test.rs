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
fn rust_manifest_test_path_not_under_tests_is_detected_as_test() {
    let repo = mk_temp_dir("project-classifier-rust-manifest");
    write_file(
        &repo.join("Cargo.toml"),
        "[package]\nname = \"demo_pkg\"\nversion = \"0.1.0\"\n\n[[test]]\nname = \"custom\"\npath = \"qa/custom_test.rs\"\n",
    );
    write_file(&repo.join("src/lib.rs"), "pub fn lib() {}\n");
    write_file(
        &repo.join("qa/custom_test.rs"),
        "#[test]\nfn ok() { assert_eq!(1, 1); }\n",
    );

    let mut classifier = headlamp::project::classify::ProjectClassifier::for_path(
        headlamp::selection::dependency_language::DependencyLanguageId::Rust,
        &repo,
    );
    let kind = classifier.classify_abs_path(&repo.join("qa/custom_test.rs"));
    assert!(matches!(
        kind,
        headlamp::project::classify::FileKind::Test | headlamp::project::classify::FileKind::Mixed
    ));
}

#[test]
fn ts_js_content_scan_detects_test_file_without_path_convention() {
    let repo = mk_temp_dir("project-classifier-tsjs-content");
    write_file(&repo.join("package.json"), "{\"name\":\"demo\"}\n");
    write_file(
        &repo.join("qa/specimens/example.js"),
        "describe('x', () => { test('y', () => {}); });\n",
    );

    let mut classifier = headlamp::project::classify::ProjectClassifier::for_path(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &repo,
    );
    let kind = classifier.classify_abs_path(&repo.join("qa/specimens/example.js"));
    assert!(matches!(
        kind,
        headlamp::project::classify::FileKind::Test | headlamp::project::classify::FileKind::Mixed
    ));
}

#[test]
fn ts_js_package_json_jest_test_match_marks_test_without_test_calls() {
    let repo = mk_temp_dir("project-classifier-tsjs-manifest");
    write_file(
        &repo.join("package.json"),
        "{\"name\":\"demo\",\"jest\":{\"testMatch\":[\"qa/**/*_case.js\"]}}\n",
    );
    write_file(
        &repo.join("qa/specimens/alpha_case.js"),
        "export const x = 1;\n",
    );

    let mut classifier = headlamp::project::classify::ProjectClassifier::for_path(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &repo,
    );
    let kind = classifier.classify_abs_path(&repo.join("qa/specimens/alpha_case.js"));
    assert!(matches!(
        kind,
        headlamp::project::classify::FileKind::Test | headlamp::project::classify::FileKind::Mixed
    ));
}

#[test]
fn ts_js_package_json_jest_ignore_patterns_override_test_match() {
    let repo = mk_temp_dir("project-classifier-tsjs-ignore");
    write_file(
        &repo.join("package.json"),
        r#"{"name":"demo","jest":{"testMatch":["qa/**/*_case.js"],"testPathIgnorePatterns":["qa/specimens"]}}
"#,
    );
    write_file(
        &repo.join("qa/specimens/alpha_case.js"),
        "test('x', () => {});\n",
    );

    let mut classifier = headlamp::project::classify::ProjectClassifier::for_path(
        headlamp::selection::dependency_language::DependencyLanguageId::TsJs,
        &repo,
    );
    let kind = classifier.classify_abs_path(&repo.join("qa/specimens/alpha_case.js"));
    assert!(matches!(
        kind,
        headlamp::project::classify::FileKind::Production
    ));
}

#[test]
fn marker_inference_finds_nearest_project_root() {
    let repo = mk_temp_dir("project-marker-inference");
    write_file(&repo.join("package.json"), "{\"name\":\"demo\"}\n");
    write_file(&repo.join("deep/nested/file.js"), "export const x = 1;\n");

    let found =
        headlamp::project::markers::find_project_root(&repo.join("deep/nested/file.js")).unwrap();
    assert_eq!(
        found.root_dir.to_string_lossy().replace('\\', "/"),
        repo.to_string_lossy().replace('\\', "/")
    );
}
