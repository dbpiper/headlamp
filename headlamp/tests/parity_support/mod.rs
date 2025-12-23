#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

mod normalize;
pub use normalize::normalize;
pub use normalize::normalize_tty_ui;

pub fn assert_parity_normalized_outputs(
    repo: &Path,
    case: &str,
    code_ts: i32,
    out_ts: &str,
    code_rs: i32,
    out_rs: &str,
) {
    if code_ts == code_rs && out_ts == out_rs {
        return;
    }

    let safe = case
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();

    let repo_key = repo.file_name().unwrap_or_default();
    let dump_dir = std::env::temp_dir()
        .join("headlamp-parity-dumps")
        .join(repo_key);
    let _ = std::fs::create_dir_all(&dump_dir);
    let ts_path = dump_dir.join(format!("{safe}-ts.txt"));
    let rs_path = dump_dir.join(format!("{safe}-rs.txt"));
    let diff_path = dump_dir.join(format!("{safe}-diff.txt"));

    let _ = std::fs::write(&ts_path, out_ts);
    let _ = std::fs::write(&rs_path, out_rs);

    let diff = similar_asserts::SimpleDiff::from_str(out_ts, out_rs, "ts", "rs").to_string();
    let _ = std::fs::write(&diff_path, &diff);

    panic!(
        "parity mismatch ({case}) repo={}: ts_exit={code_ts} rs_exit={code_rs}\nDIFF: {}\nTS: {}\nRS: {}",
        repo.display(),
        diff_path.display(),
        ts_path.display(),
        rs_path.display(),
    );
}

pub struct ParityBinaries {
    pub ts_cli: PathBuf,
    pub rust_bin: PathBuf,
    pub node_modules: PathBuf,
}

fn env_path_or_default(var: &str, default: &str) -> PathBuf {
    std::env::var(var)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

pub fn parity_binaries() -> Option<ParityBinaries> {
    if std::env::var("HEADLAMP_RUN_PARITY").ok().as_deref() != Some("1") {
        return None;
    }

    let ts_cli = env_path_or_default(
        "HEADLAMP_PARITY_TS_CLI",
        "/Users/david/src/headlamp-original/dist/cli.cjs",
    );
    let rust_bin = env_path_or_default(
        "HEADLAMP_PARITY_RS_BIN",
        "/Users/david/src/headlamp/target/debug/headlamp",
    );
    let node_modules = env_path_or_default(
        "HEADLAMP_PARITY_NODE_MODULES",
        "/Users/david/src/headlamp-original/node_modules",
    );

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

    let n_ts = normalize(out_ts, repo);
    let n_rs = normalize(out_rs, repo);
    assert_parity_normalized_outputs(repo, "fixture", code_ts, &n_ts, code_rs, &n_rs);
}

pub fn assert_parity_tty_ui_with_args(
    repo: &Path,
    binaries: &ParityBinaries,
    case: &str,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
) {
    let (code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        columns,
        ts_args,
        rs_args,
    );

    let n_ts = normalize_tty_ui(out_ts, repo);
    let n_rs = normalize_tty_ui(out_rs, repo);
    assert_parity_normalized_outputs(repo, case, code_ts, &n_ts, code_rs, &n_rs);
}

pub fn extract_coverage_ui_block(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .rposition(|ln| ln.trim_start().starts_with('┌') && ln.contains('┬'))
        .or_else(|| {
            lines
                .iter()
                .rposition(|ln| ln.contains('┌') && ln.contains('┬'))
        })
        .unwrap_or(0);

    let end = lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, ln)| {
            ln.starts_with(
                "================================================================================",
            )
            .then_some(index)
        })
        .unwrap_or(lines.len().saturating_sub(1));

    lines.get(start..=end).unwrap_or(&lines[..]).join("\n")
}

pub fn extract_istanbul_text_table_block(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let header_idx = lines
        .iter()
        .rposition(|line| {
            headlamp_core::format::stacks::strip_ansi_simple(line).contains("Uncovered Line #s")
        })
        .unwrap_or(0);

    let start = (0..=header_idx)
        .rev()
        .find(|&index| is_istanbul_dash_line(lines[index]))
        .unwrap_or(header_idx);

    let end = (header_idx..lines.len())
        .filter(|&index| is_istanbul_dash_line(lines[index]))
        .last()
        .unwrap_or(lines.len().saturating_sub(1));

    lines.get(start..=end).unwrap_or(&lines[..]).join("\n")
}

fn is_istanbul_dash_line(line: &str) -> bool {
    let stripped = headlamp_core::format::stacks::strip_ansi_simple(line);
    stripped.contains("|---------|") && stripped.chars().all(|c| c == '-' || c == '|')
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

fn run_cmd_tty(mut cmd: Command, columns: usize) -> (i32, String) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("FORCE_COLOR", "1");
    cmd.env("CI", "1");
    cmd.env_remove("NO_COLOR");

    let mut script = Command::new("script");
    let capture_path = std::env::temp_dir().join(format!(
        "headlamp-tty-capture-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&capture_path);
    script.arg("-q").arg(&capture_path);
    script
        .arg("sh")
        .arg("-lc")
        .arg(build_tty_shell_command(&cmd, columns));
    script.current_dir(
        cmd.get_current_dir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    );
    cmd.get_envs().for_each(|(key, value)| match value {
        Some(v) => {
            script.env(key, v);
        }
        None => {
            script.env_remove(key);
        }
    });

    let out = script.output().unwrap();
    let code = out.status.code().unwrap_or(1);
    let bytes = std::fs::read(&capture_path).unwrap_or_default();
    let mut combined = String::from_utf8_lossy(&bytes).to_string();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    let combined = combined
        .replace('\u{0008}', "")
        .replace('\u{0004}', "")
        .replace("^D", "");
    let _ = std::fs::remove_file(&capture_path);
    (code, combined)
}

fn run_cmd_tty_stdout_piped(mut cmd: Command, columns: usize) -> (i32, String) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("CI", "1");
    cmd.env_remove("NO_COLOR");
    cmd.env_remove("FORCE_COLOR");

    let stdout_capture_path = std::env::temp_dir().join(format!(
        "headlamp-tty-stdout-capture-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&stdout_capture_path);

    let mut script = Command::new("script");
    let tty_capture_path = std::env::temp_dir().join(format!(
        "headlamp-tty-capture-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&tty_capture_path);
    script.arg("-q").arg(&tty_capture_path);
    script
        .arg("sh")
        .arg("-lc")
        .arg(build_tty_shell_command_stdout_redirect(
            &cmd,
            columns,
            &stdout_capture_path,
        ));
    script.current_dir(
        cmd.get_current_dir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    );
    cmd.get_envs().for_each(|(key, value)| match value {
        Some(v) => {
            script.env(key, v);
        }
        None => {
            script.env_remove(key);
        }
    });

    let out = script.output().unwrap();
    let code = out.status.code().unwrap_or(1);
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(
        &std::fs::read(&stdout_capture_path).unwrap_or_default(),
    ));
    combined.push_str(&String::from_utf8_lossy(
        &std::fs::read(&tty_capture_path).unwrap_or_default(),
    ));
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    let combined = combined
        .replace('\u{0008}', "")
        .replace('\u{0004}', "")
        .replace("^D", "");
    let _ = std::fs::remove_file(&stdout_capture_path);
    let _ = std::fs::remove_file(&tty_capture_path);
    (code, combined)
}

fn build_tty_shell_command(cmd: &Command, columns: usize) -> String {
    let exe = shell_escape(cmd.get_program().to_string_lossy().as_ref());
    let args = cmd
        .get_args()
        .map(|a| shell_escape(a.to_string_lossy().as_ref()))
        .collect::<Vec<_>>()
        .join(" ");
    format!("stty cols {columns} rows 40 2>/dev/null || true; exec {exe} {args}")
}

fn build_tty_shell_command_stdout_redirect(
    cmd: &Command,
    columns: usize,
    stdout_path: &Path,
) -> String {
    let exe = shell_escape(cmd.get_program().to_string_lossy().as_ref());
    let args = cmd
        .get_args()
        .map(|a| shell_escape(a.to_string_lossy().as_ref()))
        .collect::<Vec<_>>()
        .join(" ");
    let stdout_capture = shell_escape(stdout_path.to_string_lossy().as_ref());
    format!("stty cols {columns} rows 40 2>/dev/null || true; exec {exe} {args} > {stdout_capture}")
}

fn shell_escape(text: &str) -> String {
    let safe = text.replace('\'', r"'\''");
    format!("'{safe}'")
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

pub fn run_parity_fixture_with_args_tty(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
) -> (i32, String, i32, String) {
    let mut cmd_ts = Command::new("node");
    cmd_ts.current_dir(repo).arg(ts_cli).arg("--sequential");
    cmd_ts.env(
        "HEADLAMP_CACHE_DIR",
        repo.join(".headlamp-cache-ts")
            .to_string_lossy()
            .to_string(),
    );
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let (code_ts, out_ts) = run_cmd_tty(cmd_ts, columns);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    cmd_rs.env(
        "HEADLAMP_CACHE_DIR",
        repo.join(".headlamp-cache-rs")
            .to_string_lossy()
            .to_string(),
    );
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let (code_rs, out_rs) = run_cmd_tty(cmd_rs, columns);
    (code_ts, out_ts, code_rs, out_rs)
}

pub fn run_parity_fixture_with_args_tty_stdout_piped(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
) -> (i32, String, i32, String) {
    let mut cmd_ts = Command::new("node");
    cmd_ts.current_dir(repo).arg(ts_cli).arg("--sequential");
    cmd_ts.env(
        "HEADLAMP_CACHE_DIR",
        repo.join(".headlamp-cache-ts")
            .to_string_lossy()
            .to_string(),
    );
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let (code_ts, out_ts) = run_cmd_tty_stdout_piped(cmd_ts, columns);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    cmd_rs.env(
        "HEADLAMP_CACHE_DIR",
        repo.join(".headlamp-cache-rs")
            .to_string_lossy()
            .to_string(),
    );
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let (code_rs, out_rs) = run_cmd_tty_stdout_piped(cmd_rs, columns);
    (code_ts, out_ts, code_rs, out_rs)
}

pub fn run_rust_fixture_with_args_tty_stdout_piped(
    repo: &Path,
    rust_bin: &Path,
    columns: usize,
    rs_args: &[&str],
) -> (i32, String) {
    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    cmd_rs.env(
        "HEADLAMP_CACHE_DIR",
        repo.join(".headlamp-cache-rs")
            .to_string_lossy()
            .to_string(),
    );
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    run_cmd_tty_stdout_piped(cmd_rs, columns)
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
