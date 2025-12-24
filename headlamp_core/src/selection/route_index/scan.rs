use std::path::Path;

use duct::cmd as duct_cmd;
use path_slash::PathExt;
use which::which;

use crate::selection::route_index::normalize;

pub fn discover_tests_for_http_paths(
    repo_root: &Path,
    http_paths: &[String],
    exclude_globs: &[String],
) -> Vec<String> {
    let Ok(rg) = which("rg") else {
        return vec![];
    };
    if http_paths.is_empty() {
        return vec![];
    }
    let tokens = http_paths
        .iter()
        .flat_map(|p| normalize::expand_http_search_tokens(p))
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return vec![];
    }

    let mut args: Vec<String> = vec![
        "--no-messages".to_string(),
        "--line-number".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "--files-with-matches".to_string(),
        "-F".to_string(),
        "-S".to_string(),
    ];
    for g in [
        "**/*.{test,spec}.{ts,tsx,js,jsx}",
        "tests/**/*.{ts,tsx,js,jsx}",
    ] {
        args.push("-g".to_string());
        args.push(g.to_string());
    }
    for ex in exclude_globs {
        args.push("-g".to_string());
        args.push(format!("!{ex}"));
    }
    for token in tokens {
        args.push("-e".to_string());
        args.push(token);
    }
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
        .map(|rel_or_abs| repo_root.join(rel_or_abs).to_slash_lossy().to_string())
        .collect()
}
