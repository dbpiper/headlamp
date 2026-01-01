use std::path::PathBuf;
use std::sync::OnceLock;

use crate::fs::write_file;
use crate::git::git_commit_all;

pub fn shared_threshold_real_runner_repo() -> PathBuf {
    static REPO: OnceLock<PathBuf> = OnceLock::new();
    REPO.get_or_init(|| {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("parity-fixtures")
            .join("real-runner-repo-thresholds");
        let _ = std::fs::create_dir_all(&repo);
        let _lock = super::acquire_repo_init_lock(&repo.with_extension("init-lock"));
        if !super::repo_is_initialized(&repo) {
            super::write_real_runner_repo(&repo);
            write_file(
                &repo.join("tests/sum_fail_test.js"),
                r#"const { sum } = require('../src/sum');

test('test_sum_fails', () => {
  console.log('log-pass');
  console.error('err-fail');
  expect(sum(1, 2)).toBe(3);
});
"#,
            );
            write_file(
                &repo.join("tests/sum_fail_test.rs"),
                "\
use parity_real::sum;\n\
\n\
#[test]\n\
fn test_sum_fails() {\n\
    println!(\"log-pass\");\n\
    eprintln!(\"err-fail\");\n\
    assert_eq!(sum(1, 2), 3);\n\
}\n\
",
            );
            write_file(
                &repo.join("tests/sum_fail_test.py"),
                r#"import sys
from sum import sum_two

def test_sum_fails() -> None:
    print("log-pass")
    sys.stderr.write("err-fail\n")
    assert sum_two(1, 2) == 3
"#,
            );
            write_file(
                &repo.join("headlamp.config.json5"),
                "\
{\n\
  coverage: {\n\
    thresholds: {\n\
      lines: 101,\n\
      functions: 101,\n\
      branches: 101,\n\
    },\n\
  },\n\
}\n\
",
            );
            git_commit_all(&repo, "coverage thresholds");
            super::mark_repo_initialized(&repo);
        }
        repo
    })
    .clone()
}
