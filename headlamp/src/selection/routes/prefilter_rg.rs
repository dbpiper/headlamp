use std::path::{Path, PathBuf};

use duct::cmd as duct_cmd;
use path_slash::PathExt;
use which::which;

const DEFAULT_EXCLUDE_GLOBS: [&str; 4] = [
    "**/node_modules/**",
    "**/dist/**",
    "**/build/**",
    "**/.next/**",
];

pub fn discover_candidate_files(
    repo_root: &Path,
    candidate_file_globs: &[&str],
    fixed_string_tokens: &[&str],
) -> Vec<PathBuf> {
    let Ok(rg) = which("rg") else {
        return vec![];
    };
    if candidate_file_globs.is_empty() || fixed_string_tokens.is_empty() {
        return vec![];
    }

    let mut args: Vec<String> = vec![
        "--no-messages".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "--files-with-matches".to_string(),
        "-F".to_string(),
        "-S".to_string(),
    ];
    candidate_file_globs.iter().for_each(|glob| {
        args.push("-g".to_string());
        args.push((*glob).to_string());
    });
    DEFAULT_EXCLUDE_GLOBS.iter().for_each(|exclude| {
        args.push("-g".to_string());
        args.push(format!("!{exclude}"));
    });
    fixed_string_tokens.iter().for_each(|token| {
        args.push("-e".to_string());
        args.push((*token).to_string());
    });
    args.push(repo_root.to_string_lossy().to_string());

    let Ok(out) = duct_cmd(rg, args)
        .dir(repo_root)
        .env("CI", "1")
        .stderr_capture()
        .stdout_capture()
        .unchecked()
        .run()
    else {
        return vec![];
    };

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|rel_or_abs| repo_root.join(rel_or_abs))
        .map(|p| dunce::canonicalize(&p).unwrap_or(p))
        .collect::<Vec<_>>()
}

pub fn normalize_abs_posix(path: &Path) -> String {
    dunce::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_slash_lossy()
        .to_string()
}
