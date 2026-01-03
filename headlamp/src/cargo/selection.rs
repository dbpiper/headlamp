use std::path::{Path, PathBuf};

use headlamp_core::args::ParsedArgs;
use headlamp_core::config::ChangedMode;

use crate::cargo_select::{changed_rust_seeds, filter_rust_tests_by_seeds, list_rust_test_files};

#[derive(Debug, Clone)]
pub(crate) struct CargoSelection {
    pub(crate) extra_cargo_args: Vec<String>,
    pub(crate) changed_selection_attempted: bool,
    pub(crate) selected_test_count: Option<usize>,
}

pub(crate) fn derive_cargo_selection(
    repo_root: &Path,
    args: &ParsedArgs,
    changed: &[PathBuf],
) -> CargoSelection {
    if !args.selection_paths.is_empty() {
        return derive_selection_from_selection_paths(repo_root, &args.selection_paths);
    }

    if changed.is_empty() {
        return CargoSelection {
            extra_cargo_args: vec![],
            changed_selection_attempted: false,
            selected_test_count: None,
        };
    }

    let tests = list_rust_test_files(repo_root);
    if tests.is_empty() {
        return CargoSelection {
            extra_cargo_args: vec![],
            changed_selection_attempted: true,
            selected_test_count: None,
        };
    }

    let seeds = changed_rust_seeds(repo_root, changed);
    let kept = filter_rust_tests_by_seeds(&tests, &seeds);
    let test_targets = kept
        .iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let selected_count = test_targets.len();
    CargoSelection {
        extra_cargo_args: build_test_target_args(&test_targets),
        changed_selection_attempted: true,
        selected_test_count: if selected_count == 0 {
            None
        } else {
            Some(selected_count)
        },
    }
}

fn derive_selection_from_selection_paths(
    repo_root: &Path,
    selection_paths: &[String],
) -> CargoSelection {
    let abs = selection_paths
        .iter()
        .map(|p| repo_root.join(p))
        .filter(|p| p.exists())
        .collect::<Vec<_>>();
    if abs.is_empty() {
        return CargoSelection {
            extra_cargo_args: vec![],
            changed_selection_attempted: false,
            selected_test_count: None,
        };
    }

    let direct_test_stems = abs
        .iter()
        .filter(|p| is_rust_test_file(p))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    if !direct_test_stems.is_empty() {
        return CargoSelection {
            extra_cargo_args: build_test_target_args(&direct_test_stems),
            changed_selection_attempted: false,
            selected_test_count: Some(direct_test_stems.len()),
        };
    }

    let test_targets = derive_test_targets_from_seeds(repo_root, &abs);
    CargoSelection {
        extra_cargo_args: build_test_target_args(&test_targets),
        changed_selection_attempted: false,
        selected_test_count: Some(test_targets.len()),
    }
}

pub(crate) fn changed_mode_to_cli_string(mode: ChangedMode) -> &'static str {
    match mode {
        ChangedMode::All => "all",
        ChangedMode::Staged => "staged",
        ChangedMode::Unstaged => "unstaged",
        ChangedMode::Branch => "branch",
        ChangedMode::LastCommit => "lastCommit",
        ChangedMode::LastRelease => "lastRelease",
    }
}

fn derive_test_targets_from_seeds(repo_root: &Path, seeds_input: &[PathBuf]) -> Vec<String> {
    let tests = list_rust_test_files(repo_root);
    if tests.is_empty() {
        return vec![];
    }
    let seeds = changed_rust_seeds(repo_root, seeds_input);
    let kept = filter_rust_tests_by_seeds(&tests, &seeds);
    let mut stems = kept
        .iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    stems.sort();
    stems.dedup();
    stems
}

fn is_rust_test_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("rs")
        && path
            .components()
            .any(|c| c.as_os_str().to_string_lossy() == "tests")
}

fn build_test_target_args(test_targets: &[String]) -> Vec<String> {
    let mut sorted = test_targets.to_vec();
    sorted.sort();
    sorted.dedup();

    sorted
        .into_iter()
        .flat_map(|stem| ["--test".to_string(), stem])
        .collect::<Vec<_>>()
}
