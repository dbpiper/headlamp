use std::path::Path;
use std::time::{Duration, Instant};

use headlamp::coverage::istanbul_pretty::format_istanbul_pretty_from_lcov_report;
use headlamp::coverage::lcov::read_repo_lcov_filtered;
use headlamp::coverage::model::apply_statement_hits_to_report;
use headlamp::coverage::print::PrintOpts;

fn is_ci() -> bool {
    // Support common CI envs. `CI` is widely used; GitHub Actions also sets `GITHUB_ACTIONS=true`.
    std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok()
}

fn coverage_pipeline_time_budget() -> Duration {
    // This test is intended to catch algorithmic regressions, not to be a strict benchmark.
    // GitHub-hosted runners can be quite noisy, so we allow extra slack there.
    if is_ci() {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(5)
    }
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn mk_fixture_repo(
    repo_root: &Path,
    file_count: usize,
    lcov_lines_per_file: u32,
    llvm_cov_segments_per_file: u32,
) {
    let src_root = repo_root.join("src");
    std::fs::create_dir_all(&src_root).unwrap();

    let abs_paths = (0..file_count)
        .map(|index| src_root.join(format!("file_{index}.rs")))
        .collect::<Vec<_>>();

    abs_paths.iter().for_each(|path| {
        write_file(path, "fn f() { let _x = 1; }\n");
    });

    let lcov_text = abs_paths
        .iter()
        .map(|abs_path| {
            let abs_path = abs_path.to_string_lossy();
            let lines = (1..=lcov_lines_per_file)
                .map(|line| format!("DA:{line},1\n"))
                .collect::<String>();
            format!("TN:\nSF:{abs_path}\n{lines}end_of_record\n")
        })
        .collect::<String>();
    write_file(&repo_root.join("coverage/lcov.info"), &lcov_text);

    let coverage_json = mk_llvm_cov_json(&abs_paths, llvm_cov_segments_per_file);
    write_file(&repo_root.join("coverage/coverage.json"), &coverage_json);
}

fn mk_llvm_cov_json(abs_paths: &[std::path::PathBuf], segments_per_file: u32) -> String {
    let mut out = String::from("{\"data\":[{\"files\":[");
    for (file_index, abs_path) in abs_paths.iter().enumerate() {
        if file_index > 0 {
            out.push(',');
        }
        out.push_str("{\"filename\":\"");
        out.push_str(&escape_json_string(&abs_path.to_string_lossy()));
        out.push_str("\",\"segments\":[");
        for segment_index in 0..segments_per_file {
            if segment_index > 0 {
                out.push(',');
            }
            let line = segment_index + 1;
            out.push_str(&format!("[{line},0,0,true,true,false]"));
        }
        out.push_str("]}");
    }
    out.push_str("]}]}");
    out
}

fn escape_json_string(text: &str) -> String {
    text.chars()
        .flat_map(|character| match character {
            '\\' => ['\\', '\\'].into_iter().collect::<Vec<_>>(),
            '"' => ['\\', '"'].into_iter().collect::<Vec<_>>(),
            _ => vec![character],
        })
        .collect()
}

fn measure<T>(f: impl FnOnce() -> T) -> (Duration, T) {
    let started_at = Instant::now();
    let value = f();
    (started_at.elapsed(), value)
}

#[test]
fn coverage_pipeline_for_250_files_completes_under_time_budget() {
    let repo_root = tempfile::tempdir().expect("tempdir");
    let repo_root = repo_root.path();
    // Keep this fixture large enough to exercise the full pipeline, but not so large that the
    // test becomes flaky on slower machines.
    mk_fixture_repo(repo_root, 250, 50, 3_000);

    let includes = vec!["**/*.rs".to_string()];
    let excludes = vec!["**/target/**".to_string()];

    let (read_lcov_elapsed, report) =
        measure(|| read_repo_lcov_filtered(repo_root, &includes, &excludes).expect("lcov report"));

    let (read_llvm_cov_elapsed, statement_hits_by_path) = measure(|| {
        headlamp::coverage::llvm_cov_json::read_repo_llvm_cov_json_statement_hits(repo_root)
            .expect("llvm-cov json statement hits")
    });

    let (apply_statement_hits_elapsed, report) =
        measure(|| apply_statement_hits_to_report(report, statement_hits_by_path));

    let print_opts = PrintOpts {
        max_files: None,
        max_hotspots: Some(3),
        page_fit: true,
        tty: false,
        editor_cmd: None,
    };

    let (format_pretty_elapsed, _pretty) = measure(|| {
        format_istanbul_pretty_from_lcov_report(
            repo_root,
            report,
            &print_opts,
            &[],
            &includes,
            &excludes,
            None,
        )
    });

    let total = read_lcov_elapsed
        + read_llvm_cov_elapsed
        + apply_statement_hits_elapsed
        + format_pretty_elapsed;

    let budget = coverage_pipeline_time_budget();
    assert!(
        total <= budget,
        "coverage pipeline too slow: total={total:?} budget={budget:?}\nread_lcov={read_lcov_elapsed:?}\nread_llvm_cov_json={read_llvm_cov_elapsed:?}\napply_statement_hits={apply_statement_hits_elapsed:?}\nformat_pretty={format_pretty_elapsed:?}"
    );
}
