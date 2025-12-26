use std::path::{Path, PathBuf};
use std::process::Command;

fn mk_temp_dir(name: &str) -> PathBuf {
    let base = std::env::temp_dir()
        .join("headlamp-parity-fixtures")
        .join(name);
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

fn symlink_dir(src: &Path, dst: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = std::fs::remove_file(dst);
        let _ = std::fs::remove_dir_all(dst);
        symlink(src, dst).unwrap();
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_dir;
        let _ = std::fs::remove_dir_all(dst);
        symlink_dir(src, dst).unwrap();
    }
}

fn run_cmd(mut cmd: Command) -> (i32, String) {
    let out = cmd.output().unwrap();
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    (code, format!("{stdout}{stderr}"))
}

fn normalize(mut text: String, root: &Path) -> String {
    let root_s = path_slash::PathExt::to_slash_lossy(root).to_string();
    text = text.replace('\\', "/");
    text = text.replace(&root_s, "<ROOT>");
    // Normalize tmp file names and PIDs in our Rust temp paths
    text = regex_replace(&text, r"jest-bridge-[0-9]+\.json", "jest-bridge-<PID>.json");
    // Normalize durations like +12s
    text = regex_replace(&text, r"\\+[0-9]+s\\]", "+<N>s]");
    text
}

fn regex_replace(text: &str, pat: &str, repl: &str) -> String {
    let re = regex::Regex::new(pat).unwrap();
    re.replace_all(text, repl).to_string()
}

// This is the harness; it is ignored until we finish parity.
#[test]
#[ignore]
fn parity_snapshot_js_jest_basic() {
    let repo = mk_temp_dir("js-jest-basic");
    write_file(
        &repo.join("src").join("sum.ts"),
        "export function sum(a: number, b: number) { return a + b; }\n",
    );
    write_file(
        &repo.join("tests").join("sum.test.ts"),
        "import { sum } from '../src/sum';\n\ntest('sum', () => { expect(sum(1,2)).toBe(3); });\n",
    );
    // Keep this fixture JS-only so it can run under a vanilla Jest install with no ts-jest setup.
    // (TS headlamp-original handles much richer repo setups; we add those fixtures later.)
    let _ = std::fs::remove_file(repo.join("src").join("sum.ts"));
    let _ = std::fs::remove_file(repo.join("tests").join("sum.test.ts"));
    write_file(
        &repo.join("src").join("sum.js"),
        "exports.sum = (a, b) => a + b;\n",
    );
    write_file(
        &repo.join("tests").join("sum.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('sum', () => { expect(sum(1,2)).toBe(3); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'] };\n",
    );

    // Provide jest via symlink to existing headlamp-original node_modules
    let node_modules_src = PathBuf::from("/Users/david/src/headlamp-original/node_modules");
    symlink_dir(&node_modules_src, &repo.join("node_modules"));

    let ts_cli = PathBuf::from("/Users/david/src/headlamp-original/dist/cli.cjs");
    let rust_bin = PathBuf::from("/Users/david/src/headlamp/target/debug/headlamp");

    // Ensure Rust binary is built
    if !rust_bin.exists() {
        let _ = Command::new("cargo")
            .current_dir("/Users/david/src/headlamp")
            .args(["build", "-q", "-p", "headlamp"])
            .status()
            .unwrap();
    }

    let mut cmd_ts = Command::new("node");
    cmd_ts
        .current_dir(&repo)
        .arg(ts_cli)
        .arg("--coverage")
        .arg("--sequential");
    let (code_ts, out_ts) = run_cmd(cmd_ts);

    let mut cmd_rs = Command::new(&rust_bin);
    cmd_rs
        .current_dir(&repo)
        .arg("--runner=jest")
        .arg("--coverage")
        .arg("--sequential");
    let (code_rs, out_rs) = run_cmd(cmd_rs);

    assert_eq!(code_ts, code_rs);
    let n_ts = normalize(out_ts, &repo);
    let n_rs = normalize(out_rs, &repo);
    assert_eq!(n_ts, n_rs);
}
