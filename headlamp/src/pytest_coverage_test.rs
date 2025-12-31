use std::path::Path;

use tempfile::tempdir;

use crate::pytest::coverage::{
    ensure_cov_report_output_directories, extract_lcov_report_paths, should_run_coveragepy_json,
};

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn pytest_coverage_coveragepy_json_is_skipped_when_coverage_file_missing() {
    let dir = tempdir().unwrap();
    let coverage_data_file = dir.path().join(".coverage");
    assert!(!should_run_coveragepy_json(&coverage_data_file));
}

#[test]
fn pytest_coverage_coveragepy_json_is_skipped_when_coverage_file_empty() {
    let dir = tempdir().unwrap();
    let coverage_data_file = dir.path().join(".coverage");
    write_file(&coverage_data_file, &[]);
    assert!(!should_run_coveragepy_json(&coverage_data_file));
}

#[test]
fn pytest_coverage_coveragepy_json_runs_when_coverage_file_non_empty() {
    let dir = tempdir().unwrap();
    let coverage_data_file = dir.path().join(".coverage");
    write_file(&coverage_data_file, b"not-empty");
    assert!(should_run_coveragepy_json(&coverage_data_file));
}

#[test]
fn pytest_coverage_extracts_lcov_report_paths_from_cov_report_args() {
    let args = vec![
        "--cov=src/models".to_string(),
        "--cov-report=term-missing".to_string(),
        "--cov-report=lcov:coverage/lcov.info".to_string(),
    ];
    let paths = extract_lcov_report_paths(&args);
    assert_eq!(paths, vec![Path::new("coverage/lcov.info").to_path_buf()]);
}

#[test]
fn pytest_coverage_creates_output_directories_for_lcov_paths() {
    let dir = tempdir().unwrap();
    let args = vec![
        "--cov=src/models".to_string(),
        "--cov-report=lcov:coverage/lcov.info".to_string(),
        "--cov-report=lcov:coverage/nested/output.info".to_string(),
    ];

    ensure_cov_report_output_directories(dir.path(), &args).unwrap();

    assert!(dir.path().join("coverage").is_dir());
    assert!(dir.path().join("coverage/nested").is_dir());
}

#[test]
fn pytest_coverage_creates_output_directories_for_absolute_lcov_paths() {
    let dir = tempdir().unwrap();
    let abs_out = dir.path().join("abs-coverage").join("output.info");
    let args = vec![
        "--cov=src/models".to_string(),
        format!("--cov-report=lcov:{}", abs_out.to_string_lossy()),
    ];

    ensure_cov_report_output_directories(dir.path(), &args).unwrap();

    assert!(dir.path().join("abs-coverage").is_dir());
}
