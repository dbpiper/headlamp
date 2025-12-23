use std::path::{Path, PathBuf};

use path_slash::PathExt;

const CANDIDATE_FILENAMES: [&str; 6] = [
    "jest.config.cjs",
    "jest.config.js",
    "jest.config.mjs",
    "jest.config.ts",
    "jest.ts.config.js",
    "jest.ts.config.cjs",
];

pub fn list_all_jest_configs(repo_root: &Path) -> Vec<PathBuf> {
    CANDIDATE_FILENAMES
        .into_iter()
        .map(|name| repo_root.join(name))
        .filter(|p| p.exists())
        .collect()
}

pub fn append_config_arg_if_missing(args: &[String], repo_root: &Path) -> Vec<String> {
    if args.iter().any(|t| t == "--config") {
        return args.to_vec();
    }
    let first = list_all_jest_configs(repo_root).into_iter().next();
    let Some(cfg) = first else {
        return args.to_vec();
    };
    let config_token = cfg
        .strip_prefix(repo_root)
        .ok()
        .and_then(|p| p.to_str())
        .filter(|rel| !rel.starts_with(".."))
        .map(|rel| std::path::Path::new(rel).to_slash_lossy().to_string())
        .unwrap_or_else(|| cfg.to_slash_lossy().to_string());
    args.iter()
        .cloned()
        .chain(["--config".to_string(), config_token])
        .collect()
}
