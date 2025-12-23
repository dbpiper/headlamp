#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

mod normalize;
pub use normalize::normalize;

pub struct ParityBinaries {
    pub ts_cli: PathBuf,
    pub rust_bin: PathBuf,
    pub node_modules: PathBuf,
}

pub fn parity_binaries() -> Option<ParityBinaries> {
    if std::env::var("HEADLAMP_RUN_PARITY").ok().as_deref() != Some("1") {
        return None;
    }

    let ts_cli = PathBuf::from("/Users/david/src/headlamp-original/dist/cli.cjs");
    let rust_bin = PathBuf::from("/Users/david/src/headlamp/target/debug/headlamp");
    let node_modules = PathBuf::from("/Users/david/src/headlamp-original/node_modules");

    ensure_rust_bin(&rust_bin);
    if !(ts_cli.exists() && rust_bin.exists() && node_modules.exists()) {
        return None;
    }

    Some(ParityBinaries {
        ts_cli,
        rust_bin,
        node_modules,
    })
}

pub fn mk_temp_dir(name: &str) -> PathBuf {
    let base = std::env::temp_dir()
        .join("headlamp-parity-fixtures")
        .join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

pub fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

pub fn write_jest_config(repo: &Path, test_match: &str) {
    write_file(
        &repo.join("jest.config.js"),
        &format!("module.exports = {{ testMatch: ['{test_match}'] }};\n"),
    );
}

pub fn symlink_dir(src: &Path, dst: &Path) {
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

pub fn mk_repo(name: &str, node_modules: &Path) -> PathBuf {
    let repo = mk_temp_dir(name);
    symlink_dir(node_modules, &repo.join("node_modules"));
    repo
}

pub fn assert_parity(repo: &Path, binaries: &ParityBinaries) {
    assert_parity_with_args(repo, binaries, &[], &[]);
}

pub fn assert_parity_with_args(
    repo: &Path,
    binaries: &ParityBinaries,
    ts_args: &[&str],
    rs_args: &[&str],
) {
    let (code_ts, out_ts, code_rs, out_rs) =
        run_parity_fixture_with_args(repo, &binaries.ts_cli, &binaries.rust_bin, ts_args, rs_args);
    assert_eq!(code_ts, code_rs);

    let n_ts = normalize(out_ts, repo);
    let n_rs = normalize(out_rs, repo);
    assert_eq!(n_ts, n_rs);
}

fn run_cmd(mut cmd: Command) -> (i32, String) {
    let out = cmd.output().unwrap();
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}");
    let combined = combined.replace("\u{1b}[2K\rRUN ", "");
    let combined = combined.replace("\u{1b}[2K\r", "");
    (code, combined)
}

fn ensure_rust_bin(rust_bin: &Path) {
    if rust_bin.exists() {
        return;
    }
    let _ = Command::new("cargo")
        .current_dir("/Users/david/src/headlamp")
        .args(["build", "-q", "-p", "headlamp"])
        .status()
        .unwrap();
}

pub fn run_parity_fixture_with_args(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    ts_args: &[&str],
    rs_args: &[&str],
) -> (i32, String, i32, String) {
    let mut cmd_ts = Command::new("node");
    cmd_ts.current_dir(repo).arg(ts_cli).arg("--sequential");
    cmd_ts.env("CI", "1");
    cmd_ts.env(
        "HEADLAMP_CACHE_DIR",
        repo.join(".headlamp-cache-ts")
            .to_string_lossy()
            .to_string(),
    );
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let (code_ts, out_ts) = run_cmd(cmd_ts);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    cmd_rs.env("CI", "1");
    cmd_rs.env(
        "HEADLAMP_CACHE_DIR",
        repo.join(".headlamp-cache-rs")
            .to_string_lossy()
            .to_string(),
    );
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let (code_rs, out_rs) = run_cmd(cmd_rs);
    (code_ts, out_ts, code_rs, out_rs)
}

pub fn git_init(repo: &Path) {
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["init", "-q"])
        .status();
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["config", "user.email", "parity@example.com"])
        .status();
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["config", "user.name", "Parity"])
        .status();
}

pub fn git_commit_all(repo: &Path, message: &str) {
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["add", "-A"])
        .status();
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["commit", "-q", "-m", message])
        .status();
}
