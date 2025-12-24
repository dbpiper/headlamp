use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn repo_root_from_crate_manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("headlamp crate should be nested one level under repo root")
        .to_path_buf()
}

fn read_json_file(file_path: &Path) -> Value {
    let raw = fs::read_to_string(file_path)
        .unwrap_or_else(|err| panic!("failed to read {file_path:?}: {err}"));
    serde_json::from_str::<Value>(&raw)
        .unwrap_or_else(|err| panic!("failed to parse json {file_path:?}: {err}"))
}

fn read_text_file(file_path: &Path) -> String {
    fs::read_to_string(file_path)
        .unwrap_or_else(|err| panic!("failed to read {file_path:?}: {err}"))
}

fn assert_string_field(json: &Value, field_path: &[&str], expected: &str) {
    let value = field_path
        .iter()
        .fold(Some(json), |current, key| current.and_then(|v| v.get(*key)))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string field {:?}", field_path));
    assert_eq!(value, expected);
}

fn assert_array_contains_strings(json: &Value, field: &str, expected_values: &[&str]) {
    let values = json
        .get(field)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("missing array field {:?}", field))
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();

    expected_values.iter().for_each(|expected| {
        assert!(
            values.contains(expected),
            "expected {field:?} to contain {expected:?}, got {values:?}"
        );
    });
}

#[test]
fn npm_headlamp_package_is_publishable() {
    let repo_root = repo_root_from_crate_manifest_dir();
    let npm_package_root = repo_root.join("npm").join("headlamp");
    let package_json_path = npm_package_root.join("package.json");

    let package_json = read_json_file(&package_json_path);

    assert_string_field(&package_json, &["name"], "headlamp");
    assert_string_field(&package_json, &["bin", "headlamp"], "dist/cli.cjs");
    assert_string_field(
        &package_json,
        &["scripts", "postinstall"],
        "node dist/postinstall.cjs",
    );
    assert_array_contains_strings(
        &package_json,
        "files",
        &["dist", "bin", "README.md", "LICENSE"],
    );

    let cli_path = npm_package_root.join("dist").join("cli.cjs");
    let postinstall_path = npm_package_root.join("dist").join("postinstall.cjs");
    assert!(cli_path.is_file(), "missing cli entry: {cli_path:?}");
    assert!(
        postinstall_path.is_file(),
        "missing postinstall entry: {postinstall_path:?}"
    );

    let postinstall_source = read_text_file(&postinstall_path);
    assert!(
        postinstall_source.contains("releases/download/v"),
        "postinstall should download from GitHub releases when binary is missing"
    );
    assert!(
        postinstall_source.contains("${platformKey}-${getBinaryFileName()}.gz"),
        "postinstall should use `<platformKey>-<binary>.gz` naming for release assets"
    );
}
