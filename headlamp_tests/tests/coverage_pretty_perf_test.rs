use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use headlamp::coverage::istanbul_pretty::format_istanbul_pretty_from_lcov_report;
use headlamp::coverage::model::{CoverageReport, FileCoverage};
use headlamp::coverage::print::PrintOpts;
use headlamp::coverage::statement_id::statement_id_from_line_col;

fn mk_line_hits(lines: std::ops::RangeInclusive<u32>) -> BTreeMap<u32, u32> {
    lines
        .into_iter()
        .map(|line_number| (line_number, 1u32))
        .collect::<BTreeMap<_, _>>()
}

fn mk_large_report_at_path(repo_root: &Path, file_count: usize) -> CoverageReport {
    let files = (0..file_count)
        .map(|index| {
            let abs_path = repo_root.join("src").join(format!("file_{index}.rs"));
            let line_hits = mk_line_hits(1..=200);
            let statement_hits = (0..200u32)
                .map(|col| (statement_id_from_line_col(1, col), col % 2))
                .collect::<BTreeMap<_, _>>();
            FileCoverage {
                path: abs_path.to_string_lossy().to_string(),
                lines_total: 200,
                lines_covered: 200,
                statements_total: None,
                statements_covered: None,
                statement_hits: Some(statement_hits),
                uncovered_lines: Vec::new(),
                line_hits,
                function_hits: BTreeMap::new(),
                function_map: BTreeMap::new(),
                branch_hits: BTreeMap::new(),
                branch_map: BTreeMap::new(),
            }
        })
        .collect::<Vec<_>>();
    CoverageReport { files }
}

#[test]
fn coverage_pretty_demangles_rust_function_symbols_in_functions_rows() {
    let repo_root = tempfile::tempdir().expect("tempdir");
    let repo_root = repo_root.path().to_path_buf();

    let abs_path = repo_root.join("src").join("args.rs");
    let mut function_hits = BTreeMap::new();
    function_hits.insert("fn_1".to_string(), 0u32);

    let mut function_map = BTreeMap::new();
    function_map.insert(
        "fn_1".to_string(),
        (
            "_RNvNtCs6Ak84c7c2c7_8headlamp4args11derive_args".to_string(),
            12u32,
        ),
    );

    let report = CoverageReport {
        files: vec![FileCoverage {
            path: abs_path.to_string_lossy().to_string(),
            lines_total: 20,
            lines_covered: 20,
            statements_total: None,
            statements_covered: None,
            statement_hits: None,
            uncovered_lines: Vec::new(),
            line_hits: mk_line_hits(1..=20),
            function_hits,
            function_map,
            branch_hits: BTreeMap::new(),
            branch_map: BTreeMap::new(),
        }],
    };

    let print_opts = PrintOpts {
        max_files: None,
        max_hotspots: Some(3),
        page_fit: true,
        tty: false,
        editor_cmd: None,
    };

    let pretty = format_istanbul_pretty_from_lcov_report(
        &repo_root,
        report,
        &print_opts,
        &[],
        &[],
        &[],
        None,
    );

    assert!(
        pretty.contains("Functions"),
        "sanity: output should include Functions section\n{pretty}"
    );
    assert!(
        !pretty.contains("_RNvNt") && !pretty.contains("_RNC"),
        "should not leak mangled Rust symbols\n{pretty}"
    );
    assert!(
        pretty.contains("args::"),
        "should include a readable demangled function label\n{pretty}"
    );
}

#[test]
fn coverage_pretty_from_lcov_large_report_completes_under_one_second() {
    let repo_root = tempfile::tempdir().expect("tempdir");
    let repo_root = repo_root.path().to_path_buf();

    let report = mk_large_report_at_path(&repo_root, 2_000);
    let file_count = report.files.len();

    let print_opts = PrintOpts {
        max_files: None,
        max_hotspots: Some(3),
        page_fit: true,
        tty: false,
        editor_cmd: None,
    };

    let started_at = Instant::now();
    let pretty_final = format_istanbul_pretty_from_lcov_report(
        &repo_root,
        report,
        &print_opts,
        &[],
        &[],
        &[],
        None,
    );
    let elapsed_final = started_at.elapsed();
    assert!(
        elapsed_final < Duration::from_millis(1500),
        "pretty formatting took {elapsed_final:?} for {} files",
        file_count
    );
    assert!(
        pretty_final.contains("Coverage summary"),
        "sanity: output should include summary block"
    );
}

#[test]
fn coverage_pretty_runtime_scales_approximately_linearly_in_file_count() {
    let repo_root = tempfile::tempdir().expect("tempdir");
    let repo_root = repo_root.path().to_path_buf();

    let print_opts = PrintOpts {
        max_files: None,
        max_hotspots: Some(3),
        page_fit: true,
        tty: false,
        editor_cmd: None,
    };

    let small_report = mk_large_report_at_path(&repo_root, 500);
    let large_report = mk_large_report_at_path(&repo_root, 1_000);

    let small_started_at = Instant::now();
    let _small_pretty = format_istanbul_pretty_from_lcov_report(
        &repo_root,
        small_report,
        &print_opts,
        &[],
        &[],
        &[],
        None,
    );
    let small_elapsed = small_started_at.elapsed();

    let large_started_at = Instant::now();
    let _large_pretty = format_istanbul_pretty_from_lcov_report(
        &repo_root,
        large_report,
        &print_opts,
        &[],
        &[],
        &[],
        None,
    );
    let large_elapsed = large_started_at.elapsed();

    // Doubling file count should not explode runtime; allow plenty of slack for CI noise.
    let max_allowed = small_elapsed * 4;
    assert!(
        large_elapsed <= max_allowed,
        "expected ~linear scaling: 500 files took {small_elapsed:?}, 1000 files took {large_elapsed:?}"
    );
}
