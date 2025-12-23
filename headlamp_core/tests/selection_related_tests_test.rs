use std::path::{Path, PathBuf};

fn rg_available() -> bool {
    which::which("rg").is_ok()
}

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
fn select_related_tests_finds_transitive_importer_tests() {
    if !rg_available() {
        return;
    }

    let repo = mk_temp_dir("select-related-transitive");
    write_file(&repo.join("src/a.js"), "exports.a = () => 1;\n");
    write_file(
        &repo.join("src/index.js"),
        "const { a } = require('./a');\nexports.run = () => a();\n",
    );
    write_file(
        &repo.join("tests/index.test.js"),
        "const { run } = require('../src/index');\n\ntest('ok', () => { expect(run()).toBe(1); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );

    let seed = repo.join("src/a.js").to_string_lossy().to_string();
    let selection =
        headlamp_core::selection::related_tests::select_related_tests(&repo, &[seed], &[]);

    let selected = selection.selected_test_paths_abs.join("\n");
    assert!(selected.contains("tests/index.test.js"));
    assert!(!selected.contains("tests/unrelated.test.js"));
}

#[test]
fn select_related_tests_augments_with_http_route_tests() {
    if !rg_available() {
        return;
    }

    let repo = mk_temp_dir("select-related-http-augment");
    write_file(
        &repo.join("server/routes.js"),
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/hello', (_req, res) => res.send('ok'));\nmodule.exports = router;\n",
    );
    write_file(
        &repo.join("tests/http.test.js"),
        "test('http', () => { expect('/hello').toContain('hello'); });\n",
    );
    write_file(
        &repo.join("tests/nope.test.js"),
        "test('nope', () => { expect('/bye').toContain('bye'); });\n",
    );

    let seed = repo.join("server/routes.js").to_string_lossy().to_string();
    let selection =
        headlamp_core::selection::related_tests::select_related_tests(&repo, &[seed], &[]);

    let selected = selection.selected_test_paths_abs.join("\n");
    assert!(selected.contains("tests/http.test.js"));
    assert!(!selected.contains("tests/nope.test.js"));
}
