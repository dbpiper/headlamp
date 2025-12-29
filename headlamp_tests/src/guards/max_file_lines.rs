use std::cmp::Reverse;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::workspace_scan::list_workspace_rust_files;

#[derive(Debug, Clone, Copy)]
pub struct MaxFileLinesGuardConfig {
    pub max_physical_lines: usize,
}

#[derive(Debug, Clone)]
pub struct FileLineViolation {
    pub file_path: PathBuf,
    pub physical_lines: usize,
}

fn count_physical_lines(path: &Path) -> usize {
    let file = File::open(path).unwrap_or_else(|err| panic!("failed opening {path:?}: {err}"));
    let reader = BufReader::new(file);
    reader
        .lines()
        .fold(0usize, |count, line| count + usize::from(line.is_ok()))
}

pub fn find_files_over_max_physical_lines(cfg: MaxFileLinesGuardConfig) -> Vec<FileLineViolation> {
    let mut violations = list_workspace_rust_files()
        .into_iter()
        .map(|file_path| {
            let physical_lines = count_physical_lines(&file_path);
            (file_path, physical_lines)
        })
        .filter(|(_file_path, physical_lines)| *physical_lines > cfg.max_physical_lines)
        .map(|(file_path, physical_lines)| FileLineViolation {
            file_path,
            physical_lines,
        })
        .collect::<Vec<_>>();

    violations.sort_by(|left, right| {
        (Reverse(left.physical_lines), &left.file_path)
            .cmp(&(Reverse(right.physical_lines), &right.file_path))
    });

    violations
}

pub fn format_violation(violation: &FileLineViolation) -> String {
    format!(
        "{} lines -> {}",
        violation.physical_lines,
        violation.file_path.display()
    )
}
