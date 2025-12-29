use std::collections::BTreeMap;
use std::path::Path;

use path_slash::PathExt;

use headlamp_core::args::ParsedArgs;
use headlamp_core::test_model::TestRunModel;

pub(super) fn config_token(repo_root: &Path, cfg: &Path) -> String {
    cfg.strip_prefix(repo_root)
        .ok()
        .and_then(|p| p.to_str())
        .filter(|rel| !rel.starts_with(".."))
        .map(|rel| std::path::Path::new(rel).to_slash_lossy().to_string())
        .unwrap_or_else(|| cfg.to_slash_lossy().to_string())
}

pub(super) fn merge_bridge_json(
    items: &[TestRunModel],
    rank_by_abs_path: &BTreeMap<String, i64>,
) -> Option<TestRunModel> {
    if items.is_empty() {
        return None;
    }
    if items.len() == 1 {
        let mut only = items[0].clone();
        reorder_test_results_original_style(&mut only.test_results, rank_by_abs_path);
        return Some(only);
    }

    let start_time = items.iter().map(|b| b.start_time).min().unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    });

    let sum_u64 = |f: fn(&headlamp_core::test_model::TestRunAggregated) -> u64| -> u64 {
        items.iter().map(|b| f(&b.aggregated)).sum::<u64>()
    };
    let sum_opt_u64 =
        |f: fn(&headlamp_core::test_model::TestRunAggregated) -> Option<u64>| -> Option<u64> {
            let total = items
                .iter()
                .map(|b| f(&b.aggregated).unwrap_or(0))
                .sum::<u64>();
            Some(total)
        };

    let mut test_results = items
        .iter()
        .flat_map(|b| b.test_results.iter().cloned())
        .collect::<Vec<_>>();
    reorder_test_results_original_style(&mut test_results, rank_by_abs_path);

    let aggregated = headlamp_core::test_model::TestRunAggregated {
        num_total_test_suites: sum_u64(|a| a.num_total_test_suites),
        num_passed_test_suites: sum_u64(|a| a.num_passed_test_suites),
        num_failed_test_suites: sum_u64(|a| a.num_failed_test_suites),
        num_total_tests: sum_u64(|a| a.num_total_tests),
        num_passed_tests: sum_u64(|a| a.num_passed_tests),
        num_failed_tests: sum_u64(|a| a.num_failed_tests),
        num_pending_tests: sum_u64(|a| a.num_pending_tests),
        num_todo_tests: sum_u64(|a| a.num_todo_tests),
        num_timed_out_tests: sum_opt_u64(|a| a.num_timed_out_tests),
        num_timed_out_test_suites: sum_opt_u64(|a| a.num_timed_out_test_suites),
        start_time,
        success: items.iter().all(|b| b.aggregated.success),
        run_time_ms: Some(
            items
                .iter()
                .map(|b| b.aggregated.run_time_ms.unwrap_or(0))
                .sum(),
        ),
    };

    Some(TestRunModel {
        start_time,
        test_results,
        aggregated,
    })
}

fn reorder_test_results_original_style(
    test_results: &mut [headlamp_core::test_model::TestSuiteResult],
    rank_by_abs_path: &BTreeMap<String, i64>,
) {
    let rank_or_inf = |abs_path: &str| -> i64 {
        rank_by_abs_path
            .get(&normalize_abs_posix(abs_path))
            .copied()
            .unwrap_or(i64::MAX)
    };

    let file_failed = |file: &headlamp_core::test_model::TestSuiteResult| -> bool {
        file.status == "failed"
            || file
                .test_results
                .iter()
                .any(|assertion| assertion.status == "failed")
    };

    if rank_by_abs_path.is_empty() && test_results.iter().all(|file| !file_failed(file)) {
        test_results.reverse();
        return;
    }

    test_results.sort_by(|left, right| {
        rank_or_inf(&left.test_file_path)
            .cmp(&rank_or_inf(&right.test_file_path))
            .then_with(|| {
                normalize_abs_posix(&left.test_file_path)
                    .cmp(&normalize_abs_posix(&right.test_file_path))
            })
    });
}

fn normalize_abs_posix(input_path: &str) -> String {
    let posix = input_path.replace('\\', "/");
    if std::path::Path::new(&posix).is_absolute() {
        return posix;
    }
    std::env::current_dir()
        .ok()
        .map(|cwd| {
            cwd.join(&posix)
                .to_string_lossy()
                .to_string()
                .replace('\\', "/")
        })
        .unwrap_or(posix)
}

pub(super) fn filter_bridge_for_name_pattern_only(mut bridge: TestRunModel) -> TestRunModel {
    let mut kept: Vec<headlamp_core::test_model::TestSuiteResult> = vec![];
    for mut file in bridge.test_results.into_iter() {
        let suite_has_failure =
            !file.failure_message.trim().is_empty() || file.test_exec_error.is_some();
        file.test_results
            .retain(|a| a.status == "passed" || a.status == "failed");
        if !file.test_results.is_empty() || suite_has_failure {
            kept.push(file);
        }
    }

    let num_failed_tests = kept
        .iter()
        .flat_map(|f| f.test_results.iter())
        .filter(|a| a.status == "failed")
        .count() as u64;
    let num_passed_tests = kept
        .iter()
        .flat_map(|f| f.test_results.iter())
        .filter(|a| a.status == "passed")
        .count() as u64;
    let num_total_tests = num_failed_tests + num_passed_tests;

    let num_failed_suites = kept
        .iter()
        .filter(|f| {
            !f.failure_message.trim().is_empty()
                || f.test_exec_error.is_some()
                || f.test_results.iter().any(|a| a.status == "failed")
        })
        .count() as u64;
    let num_passed_suites = (kept.len() as u64).saturating_sub(num_failed_suites);
    let success = num_failed_tests == 0 && num_failed_suites == 0;

    bridge.test_results = kept;
    bridge.aggregated.num_total_test_suites = bridge.test_results.len() as u64;
    bridge.aggregated.num_passed_test_suites = num_passed_suites;
    bridge.aggregated.num_failed_test_suites = num_failed_suites;
    bridge.aggregated.num_total_tests = num_total_tests;
    bridge.aggregated.num_passed_tests = num_passed_tests;
    bridge.aggregated.num_failed_tests = num_failed_tests;
    bridge.aggregated.num_pending_tests = 0;
    bridge.aggregated.num_todo_tests = 0;
    bridge.aggregated.success = success;
    bridge
}

pub(super) fn should_skip_run_tests_by_path_for_name_pattern_only(
    args: &ParsedArgs,
    selection_paths_abs: &[String],
) -> bool {
    if !args.selection_specified {
        return false;
    }
    if args.changed.is_some() {
        return false;
    }
    if !selection_paths_abs.is_empty() || !args.selection_paths.is_empty() {
        return false;
    }
    args.runner_args.iter().any(|tok| {
        tok == "-t" || tok == "--testNamePattern" || tok.starts_with("--testNamePattern=")
    })
}

pub(super) fn looks_sparse(pretty: &str) -> bool {
    let simple = headlamp_core::format::stacks::strip_ansi_simple(pretty);
    let lines = simple.lines().collect::<Vec<_>>();

    if missing_fail_header_code_frame(&lines) && looks_like_assertion_failure(&simple) {
        return true;
    }

    let has_error_blank = lines
        .windows(2)
        .any(|w| w[0].trim() == "Error:" && w[1].trim().is_empty());
    if !has_error_blank {
        return false;
    }
    !["Message:", "Thrown:", "Events:", "Console errors:"]
        .into_iter()
        .any(|needle| simple.contains(needle))
}

fn looks_like_assertion_failure(text: &str) -> bool {
    ["Expected", "Received", "Assertion:"]
        .into_iter()
        .any(|needle| text.contains(needle))
}

fn missing_fail_header_code_frame(lines: &[&str]) -> bool {
    let fail_i = lines.iter().position(|line| {
        let t = line.trim_start();
        t.starts_with("FAIL  ") || t.starts_with(" FAIL  ")
    });
    let Some(fail_i) = fail_i else {
        return false;
    };
    let mut window = lines.iter().skip(fail_i.saturating_add(1)).take(8);
    let has_code_frame = window.any(|line| {
        let t = line.trim_start();
        t.contains('|') && t.chars().any(|c| c.is_ascii_digit())
    });
    !has_code_frame
}

pub(super) fn merge_sparse_bridge_and_raw(bridge_pretty: &str, raw_pretty: &str) -> String {
    let (bridge_body, bridge_footer) = split_footer(bridge_pretty);
    let (raw_body, _raw_footer) = split_footer(raw_pretty);
    [
        bridge_body.trim_end(),
        raw_body.trim_end(),
        bridge_footer.trim_end(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
}

fn split_footer(text: &str) -> (String, String) {
    let lines = text.lines().collect::<Vec<_>>();
    let Some(i) = lines.iter().rposition(|ln| ln.starts_with("Test Files ")) else {
        return (text.to_string(), String::new());
    };
    let (body, footer) = lines.split_at(i);
    (body.join("\n"), footer.join("\n"))
}
