mod parity_support;

use std::ffi::OsString;

use parity_support::{
    mk_repo, mk_temp_dir, normalize, parity_binaries, run_parity_fixture_with_args, write_file,
    write_jest_config,
};

struct EnvVarGuard {
    key: &'static str,
    prev: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, prev }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(value) => unsafe {
                std::env::set_var(self.key, value);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
}

#[test]
fn parity_fast_related_cache_hit_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("fast-related-cache-hit", &binaries.node_modules);
    write_file(&repo.join("src/a.js"), "exports.a = () => 1;\n");
    write_file(
        &repo.join("tests/a.test.js"),
        "const { a } = require('../src/a');\n\ntest('a', () => { expect(a()).toBe(1); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let cache_root = mk_temp_dir("fast-related-cache-dir");
    let _guard = EnvVarGuard::set("HEADLAMP_CACHE_DIR", &cache_root.to_string_lossy());

    let (_spec_1, code_ts_1, out_ts_1, code_rs_1, out_rs_1) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &["src/a.js"],
        &["src/a.js"],
    );
    assert_eq!(code_ts_1, code_rs_1);
    let n_ts_1 = normalize(out_ts_1, &repo);
    let n_rs_1 = normalize(out_rs_1, &repo);
    assert_eq!(n_ts_1, n_rs_1);

    let (_spec_2, code_ts_2, out_ts_2, code_rs_2, out_rs_2) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &["src/a.js"],
        &["src/a.js"],
    );
    assert_eq!(code_ts_2, code_rs_2);
    let n_ts_2 = normalize(out_ts_2, &repo);
    let n_rs_2 = normalize(out_rs_2, &repo);
    assert_eq!(n_ts_2, n_rs_2);
    assert_eq!(n_ts_1, n_ts_2);
    assert_eq!(n_rs_1, n_rs_2);

    let found = std::fs::read_dir(&cache_root)
        .ok()
        .into_iter()
        .flat_map(|iter| iter.flatten())
        .map(|entry| entry.path().join("relevant-tests.json"))
        .any(|path| path.exists());
    assert!(found);
}
