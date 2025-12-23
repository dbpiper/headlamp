use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use duct::cmd as duct_cmd;
use path_slash::PathExt;
use tempfile::NamedTempFile;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::istanbul::{merge_istanbul_reports, read_istanbul_coverage_tree};
use headlamp_core::coverage::istanbul_pretty::format_istanbul_pretty;
use headlamp_core::coverage::lcov::{merge_reports, read_lcov_file, resolve_lcov_paths_to_root};
use headlamp_core::coverage::print::{
    PrintOpts, filter_report, format_compact, format_hotspots, format_summary,
};
use headlamp_core::format::bridge::BridgeConsoleEntry;
use headlamp_core::format::bridge::BridgeJson;
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::vitest::render_vitest_from_jest_json;
use headlamp_core::selection::related_tests::select_related_tests;
use headlamp_core::selection::relevance::augment_rank_with_priority_paths;
use headlamp_core::selection::route_index::{discover_tests_for_http_paths, get_route_index};
use headlamp_core::selection::transitive_seed_refine::{
    filter_tests_by_transitive_seed, max_depth_from_args,
};
use indexmap::IndexSet;

use crate::fast_related::{
    DEFAULT_TEST_GLOBS, FAST_RELATED_TIMEOUT, cached_related, find_related_tests_fast,
};
use crate::git::changed_files;
use crate::jest_config::list_all_jest_configs;
use crate::jest_discovery::{
    JEST_LIST_TESTS_TIMEOUT, args_for_discovery, discover_jest_list_tests_cached_with_timeout,
    jest_bin,
};
use crate::jest_ownership::filter_candidates_for_project;
use crate::live_progress::{LiveProgress, should_enable_live_progress};
use crate::parallel_stride::run_parallel_stride;
use crate::run::{RunError, run_bootstrap};

const JEST_REPORTER_BYTES: &[u8] = include_bytes!("../assets/jest/reporter.cjs");
const JEST_SETUP_BYTES: &[u8] = include_bytes!("../assets/jest/setup.cjs");

pub fn run_jest(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    if let Some(cmd) = args
        .bootstrap_command
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        run_bootstrap(repo_root, cmd)?;
    }

    let jest_bin = jest_bin(repo_root);
    if !jest_bin.exists() {
        return Err(RunError::MissingRunner {
            runner: "jest".to_string(),
            hint: format!("expected {}", jest_bin.display()),
        });
    }

    let selection_paths_abs = {
        let mut selected_abs_paths: IndexSet<String> = IndexSet::new();
        args.selection_paths
            .iter()
            .map(|p| repo_root.join(p))
            .filter(|p| p.exists())
            .map(|p| p.to_slash_lossy().to_string())
            .for_each(|abs| {
                selected_abs_paths.insert(abs);
            });

        args.changed
            .map(|mode| changed_files(repo_root, mode))
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.exists())
            .map(|p| p.to_slash_lossy().to_string())
            .for_each(|abs| {
                selected_abs_paths.insert(abs);
            });

        selected_abs_paths.into_iter().collect::<Vec<_>>()
    };

    let discovery_args = args_for_discovery(&args.runner_args);

    let discovered_project_configs = list_all_jest_configs(repo_root);
    let project_configs = (!discovered_project_configs.is_empty())
        .then(|| discovered_project_configs)
        .unwrap_or_else(|| vec![repo_root.to_path_buf()]);

    let selection_is_tests_only = !selection_paths_abs.is_empty()
        && selection_paths_abs
            .iter()
            .all(|abs| looks_like_test_path(abs));

    let production_seeds = selection_paths_abs
        .iter()
        .filter(|abs| !looks_like_test_path(abs))
        .cloned()
        .collect::<Vec<_>>();

    let selection_key = (!selection_paths_abs.is_empty() && !selection_is_tests_only).then(|| {
        production_seeds
            .iter()
            .map(|abs| {
                Path::new(abs)
                    .strip_prefix(repo_root)
                    .ok()
                    .map(|p| p.to_slash_lossy().to_string())
                    .unwrap_or_else(|| Path::new(abs).to_slash_lossy().to_string())
            })
            .collect::<Vec<_>>()
            .join("|")
    });

    let related_selection = if selection_is_tests_only {
        headlamp_core::selection::related_tests::RelatedTestSelection {
            selected_test_paths_abs: selection_paths_abs.clone(),
            rank_by_abs_path: BTreeMap::new(),
        }
    } else {
        selection_key
            .as_ref()
            .map(|key| {
                cached_related(repo_root, key, || {
                    find_related_tests_fast(
                        repo_root,
                        &production_seeds,
                        &DEFAULT_TEST_GLOBS,
                        &args.exclude_globs,
                        FAST_RELATED_TIMEOUT,
                    )
                })
                .map(|fast_tests| {
                    if !fast_tests.is_empty() {
                        let augmented = augment_with_http_tests(
                            repo_root,
                            &production_seeds,
                            &args.exclude_globs,
                            fast_tests,
                        );
                        if args.changed.is_some() || args.changed_depth.is_some() {
                            return refine_by_transitive_seed_scan(
                                repo_root,
                                &project_configs,
                                &jest_bin,
                                &discovery_args,
                                &production_seeds,
                                augmented,
                                max_depth_from_args(args.changed_depth),
                            );
                        }
                        headlamp_core::selection::related_tests::RelatedTestSelection {
                            selected_test_paths_abs: augmented,
                            rank_by_abs_path: BTreeMap::new(),
                        }
                    } else {
                        if args.changed.is_some() || args.changed_depth.is_some() {
                            return refine_by_transitive_seed_scan(
                                repo_root,
                                &project_configs,
                                &jest_bin,
                                &discovery_args,
                                &production_seeds,
                                vec![],
                                max_depth_from_args(args.changed_depth),
                            );
                        }
                        select_related_tests(repo_root, &production_seeds, &args.exclude_globs)
                    }
                })
            })
            .transpose()?
            .unwrap_or_else(
                || headlamp_core::selection::related_tests::RelatedTestSelection {
                    selected_test_paths_abs: vec![],
                    rank_by_abs_path: BTreeMap::new(),
                },
            )
    };

    let directness_rank_base =
        compute_directness_rank_base(repo_root, &selection_paths_abs, &args.exclude_globs)?;
    let directness_rank = augment_rank_with_priority_paths(
        &directness_rank_base,
        &related_selection.selected_test_paths_abs,
    );

    let tmp = std::env::temp_dir().join("headlamp").join("jest");
    let reporter_path = write_asset(&tmp.join("reporter.cjs"), JEST_REPORTER_BYTES)?;
    let setup_path = write_asset(&tmp.join("setup.cjs"), JEST_SETUP_BYTES)?;
    let out_json_base = tmp.join(format!("jest-bridge-{}", std::process::id()));

    let base_cmd_args: Vec<String> = vec![
        "--testLocationInResults".to_string(),
        "--setupFilesAfterEnv".to_string(),
        setup_path.to_string_lossy().to_string(),
        "--colors".to_string(),
        "--passWithNoTests".to_string(),
        "--verbose".to_string(),
        "--reporters".to_string(),
        reporter_path.to_string_lossy().to_string(),
        "--reporters".to_string(),
        "default".to_string(),
        "--runTestsByPath".to_string(),
    ];

    let live_progress_enabled = should_enable_live_progress(std::io::stdout().is_terminal());

    #[derive(Debug)]
    struct ProjectRunOutput {
        exit_code: i32,
        bridge: Option<BridgeJson>,
        captured_stdout: Vec<String>,
        captured_stderr: Vec<String>,
    }

    let stride = if args.sequential { 1 } else { 3 };
    let live_progress = LiveProgress::start(project_configs.len(), live_progress_enabled);
    let per_project_results = run_parallel_stride(&project_configs, stride, |cfg_path, index| {
        let cfg_token = config_token(repo_root, cfg_path);
        live_progress.set_current_label(cfg_token.clone());

        let tests_for_project = if selection_paths_abs.is_empty() {
            let mut list_args = discovery_args.clone();
            list_args.extend(["--config".to_string(), cfg_token.clone()]);
            discover_jest_list_tests_cached_with_timeout(
                cfg_path.parent().unwrap_or(repo_root),
                &jest_bin,
                &list_args,
                JEST_LIST_TESTS_TIMEOUT,
            )?
        } else {
            filter_candidates_for_project(
                repo_root,
                &jest_bin,
                &discovery_args,
                cfg_path,
                &related_selection.selected_test_paths_abs,
            )?
        };

        if selection_paths_abs.is_empty() && tests_for_project.is_empty() {
            live_progress.increment_done(1);
            return Ok(ProjectRunOutput {
                exit_code: 0,
                bridge: None,
                captured_stdout: vec![],
                captured_stderr: vec![],
            });
        }

        if !selection_paths_abs.is_empty() && tests_for_project.is_empty() {
            live_progress.increment_done(1);
            return Ok(ProjectRunOutput {
                exit_code: 0,
                bridge: None,
                captured_stdout: vec![],
                captured_stderr: vec![],
            });
        }

        let out_json = out_json_base.with_extension(format!("{index}.json"));

        let mut cmd_args = base_cmd_args.clone();
        cmd_args.extend(["--config".to_string(), cfg_token.clone()]);
        cmd_args.extend(args.runner_args.iter().cloned());
        if args.show_logs {
            cmd_args.push("--no-silent".to_string());
        }
        if !tests_for_project.is_empty() {
            cmd_args.extend(tests_for_project);
        } else {
            cmd_args.extend(args.selection_paths.iter().cloned());
        }

        let out = duct_cmd(&jest_bin, cmd_args)
            .dir(repo_root)
            .env("NODE_ENV", "test")
            .env("FORCE_COLOR", "3")
            .env("JEST_BRIDGE_OUT", out_json.to_string_lossy().to_string())
            .stderr_capture()
            .stdout_capture()
            .unchecked()
            .run()
            .map_err(|e| {
                RunError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;

        let exit_code = out.status.code().unwrap_or(1);

        let extra_bridge_entries_by_test_path =
            collect_bridge_entries_from_bridge_events(&out.stdout, &out.stderr);
        let captured_stdout = split_non_event_lines(&out.stdout);
        let captured_stderr = split_non_event_lines(&out.stderr);

        let bridge = std::fs::read_to_string(&out_json)
            .ok()
            .and_then(|raw| serde_json::from_str::<BridgeJson>(&raw).ok())
            .map(|mut bridge| {
                merge_console_entries_into_bridge_json(
                    &mut bridge,
                    &extra_bridge_entries_by_test_path,
                );
                bridge
            });

        Ok(ProjectRunOutput {
            exit_code,
            bridge,
            captured_stdout,
            captured_stderr,
        })
    })?;
    live_progress.finish();

    let mut exit_codes: Vec<i32> = vec![];
    let mut bridges: Vec<BridgeJson> = vec![];
    let mut captured_stdout_all: Vec<String> = vec![];
    let mut captured_stderr_all: Vec<String> = vec![];
    for result in per_project_results {
        exit_codes.push(result.exit_code);
        captured_stdout_all.extend(result.captured_stdout);
        captured_stderr_all.extend(result.captured_stderr);
        if let Some(bridge) = result.bridge {
            bridges.push(bridge);
        }
    }

    let exit_code = exit_codes.into_iter().max().unwrap_or(1);

    if let Some(merged) = merge_bridge_json(&bridges, &directness_rank) {
        let ctx = make_ctx(
            repo_root,
            None,
            exit_code != 0,
            args.show_logs,
            args.editor_cmd.clone(),
        );
        let pretty = render_vitest_from_jest_json(&merged, &ctx, args.only_failures);
        if !pretty.trim().is_empty() {
            println!("{pretty}");
        }
    } else {
        for line in captured_stdout_all {
            println!("{line}");
        }
        for line in captured_stderr_all {
            eprintln!("{line}");
        }
    }

    if args.coverage_abort_on_failure && exit_code != 0 {
        return Ok(exit_code);
    }

    if args.collect_coverage {
        let jest_cov_dir = repo_root.join("coverage").join("jest");
        let json_tree = read_istanbul_coverage_tree(&jest_cov_dir);
        let json_reports = json_tree.into_iter().map(|(_, r)| r).collect::<Vec<_>>();
        let merged_json =
            (!json_reports.is_empty()).then(|| merge_istanbul_reports(&json_reports, repo_root));

        let lcov_candidates = [
            repo_root.join("coverage").join("lcov.info"),
            repo_root.join("coverage").join("jest").join("lcov.info"),
        ];
        let reports = lcov_candidates
            .iter()
            .filter(|p| p.exists())
            .filter_map(|p| read_lcov_file(p).ok())
            .collect::<Vec<_>>();

        let resolved = merged_json.or_else(|| {
            (!reports.is_empty()).then(|| {
                let merged = merge_reports(&reports, repo_root);
                resolve_lcov_paths_to_root(merged, repo_root)
            })
        });

        let print_opts = PrintOpts {
            max_files: args.coverage_max_files,
            max_hotspots: args.coverage_max_hotspots,
            page_fit: args.coverage_page_fit,
            tty: std::io::stdout().is_terminal(),
            editor_cmd: args.editor_cmd.clone(),
        };

        if let Some(pretty) =
            format_istanbul_pretty(repo_root, &jest_cov_dir, &print_opts, &selection_paths_abs)
        {
            println!("{pretty}");
        } else if let Some(resolved) = resolved {
            let filtered = filter_report(
                resolved,
                repo_root,
                &args.include_globs,
                &args.exclude_globs,
            );
            println!("{}", format_summary(&filtered));
            println!("{}", format_compact(&filtered, &print_opts, repo_root));
            if let Some(detail) = args.coverage_detail {
                if detail != headlamp_core::args::CoverageDetail::Auto {
                    let hs = format_hotspots(&filtered, &print_opts, repo_root);
                    if !hs.trim().is_empty() {
                        println!("{hs}");
                    }
                }
            }
        }
    }

    Ok(exit_code)
}

fn augment_with_http_tests(
    repo_root: &Path,
    production_seeds_abs: &[String],
    exclude_globs: &[String],
    related_tests_abs: Vec<String>,
) -> Vec<String> {
    let route_index = get_route_index(repo_root);
    let http_paths = production_seeds_abs
        .iter()
        .flat_map(|seed| route_index.http_routes_for_source(seed))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut route_tests = discover_tests_for_http_paths(repo_root, &http_paths, exclude_globs);
    route_tests.sort();

    let mut combined: IndexSet<String> = IndexSet::new();
    related_tests_abs.into_iter().for_each(|t| {
        combined.insert(t);
    });
    route_tests.into_iter().for_each(|t| {
        combined.insert(t);
    });
    combined.into_iter().collect::<Vec<_>>()
}

fn refine_by_transitive_seed_scan(
    repo_root: &Path,
    project_configs: &[PathBuf],
    jest_bin: &Path,
    discovery_args: &[String],
    production_seeds_abs: &[String],
    candidate_tests_abs: Vec<String>,
    max_depth: headlamp_core::selection::transitive_seed_refine::MaxDepth,
) -> headlamp_core::selection::related_tests::RelatedTestSelection {
    if !candidate_tests_abs.is_empty() {
        return headlamp_core::selection::related_tests::RelatedTestSelection {
            selected_test_paths_abs: candidate_tests_abs,
            rank_by_abs_path: BTreeMap::new(),
        };
    }

    let all_tests = project_configs
        .iter()
        .filter_map(|cfg_path| {
            let cfg_token = config_token(repo_root, cfg_path);
            let mut list_args = discovery_args.to_vec();
            list_args.extend(["--config".to_string(), cfg_token.clone()]);
            discover_jest_list_tests_cached_with_timeout(
                cfg_path.parent().unwrap_or(repo_root),
                jest_bin,
                &list_args,
                JEST_LIST_TESTS_TIMEOUT,
            )
            .ok()
        })
        .flatten()
        .collect::<IndexSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut kept =
        filter_tests_by_transitive_seed(repo_root, &all_tests, production_seeds_abs, max_depth);
    kept.sort();
    let rank_by_abs_path = kept
        .iter()
        .enumerate()
        .fold(BTreeMap::new(), |mut acc, (idx, abs)| {
            acc.insert(normalize_abs_posix(abs), idx as i64);
            acc
        });
    headlamp_core::selection::related_tests::RelatedTestSelection {
        selected_test_paths_abs: kept,
        rank_by_abs_path,
    }
}

fn config_token(repo_root: &Path, cfg: &Path) -> String {
    cfg.strip_prefix(repo_root)
        .ok()
        .and_then(|p| p.to_str())
        .filter(|rel| !rel.starts_with(".."))
        .map(|rel| std::path::Path::new(rel).to_slash_lossy().to_string())
        .unwrap_or_else(|| cfg.to_slash_lossy().to_string())
}

fn merge_bridge_json(
    items: &[BridgeJson],
    rank_by_abs_path: &BTreeMap<String, i64>,
) -> Option<BridgeJson> {
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

    let sum_u64 = |f: fn(&headlamp_core::format::bridge::BridgeAggregated) -> u64| -> u64 {
        items.iter().map(|b| f(&b.aggregated)).sum::<u64>()
    };
    let sum_opt_u64 =
        |f: fn(&headlamp_core::format::bridge::BridgeAggregated) -> Option<u64>| -> Option<u64> {
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

    let aggregated = headlamp_core::format::bridge::BridgeAggregated {
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

    Some(BridgeJson {
        start_time,
        test_results,
        aggregated,
    })
}

fn reorder_test_results_original_style(
    test_results: &mut Vec<headlamp_core::format::bridge::BridgeFileResult>,
    rank_by_abs_path: &BTreeMap<String, i64>,
) {
    let rank_or_inf = |abs_path: &str| -> i64 {
        rank_by_abs_path
            .get(&normalize_abs_posix(abs_path))
            .copied()
            .unwrap_or(i64::MAX)
    };
    let file_failed = |file: &headlamp_core::format::bridge::BridgeFileResult| -> bool {
        file.status == "failed"
            || file
                .test_results
                .iter()
                .any(|assertion| assertion.status == "failed")
    };

    let has_any_failure = test_results.iter().any(file_failed);
    if !has_any_failure && rank_by_abs_path.is_empty() {
        test_results.reverse();
        return;
    }

    test_results.sort_by(|left, right| {
        let left_failed = file_failed(left);
        let right_failed = file_failed(right);
        right_failed
            .cmp(&left_failed)
            .then_with(|| {
                rank_or_inf(&left.test_file_path).cmp(&rank_or_inf(&right.test_file_path))
            })
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

fn compute_directness_rank_base(
    repo_root: &Path,
    selection_paths_abs: &[String],
    exclude_globs: &[String],
) -> Result<BTreeMap<String, i64>, RunError> {
    let production_seeds = selection_paths_abs
        .iter()
        .filter(|abs| !looks_like_test_path(abs))
        .cloned()
        .collect::<Vec<_>>();
    if production_seeds.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut selection_key_parts = production_seeds
        .iter()
        .filter_map(|abs| {
            Path::new(abs)
                .strip_prefix(repo_root)
                .ok()
                .map(|p| p.to_slash_lossy().to_string())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    selection_key_parts.sort();
    let selection_key = selection_key_parts.join("|");

    let related = cached_related(repo_root, &selection_key, || {
        find_related_tests_fast(
            repo_root,
            &production_seeds,
            &DEFAULT_TEST_GLOBS,
            exclude_globs,
            FAST_RELATED_TIMEOUT,
        )
    })?;

    let route_index = get_route_index(repo_root);
    let http_paths = production_seeds
        .iter()
        .flat_map(|seed| route_index.http_routes_for_source(seed))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let route_tests = discover_tests_for_http_paths(repo_root, &http_paths, exclude_globs);

    let existing = related
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let augmented = related
        .into_iter()
        .chain(route_tests.into_iter().filter(|t| !existing.contains(t)))
        .collect::<Vec<_>>();

    Ok(augmented
        .into_iter()
        .enumerate()
        .fold(BTreeMap::new(), |mut acc, (index, abs)| {
            acc.insert(normalize_abs_posix(&abs), index as i64);
            acc
        }))
}

fn looks_like_test_path(candidate_path: &str) -> bool {
    let p = candidate_path.replace('\\', "/");
    p.contains("/__tests__/")
        || p.contains("/tests/")
        || p.contains(".test.")
        || p.contains(".spec.")
}

fn write_asset(path: &Path, bytes: &[u8]) -> Result<PathBuf, RunError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(RunError::Io)?;
        let mut tmp = NamedTempFile::new_in(parent).map_err(RunError::Io)?;
        use std::io::Write;
        tmp.write_all(bytes).map_err(RunError::Io)?;
        tmp.flush().map_err(RunError::Io)?;
        let _ = std::fs::remove_file(path);
        tmp.persist(path).map_err(|e| RunError::Io(e.error))?;
        return Ok(path.to_path_buf());
    }
    std::fs::write(path, bytes).map_err(RunError::Io)?;
    Ok(path.to_path_buf())
}

fn split_non_event_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| !line.starts_with("[JEST-BRIDGE-EVENT] "))
        .map(str::to_string)
        .collect()
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JestBridgeEventMeta {
    #[serde(rename = "testPath")]
    test_path: Option<String>,
}

fn collect_bridge_entries_from_bridge_events(
    stdout_bytes: &[u8],
    stderr_bytes: &[u8],
) -> BTreeMap<String, Vec<BridgeConsoleEntry>> {
    let parse_lines = |bytes: &[u8]| -> Vec<(String, String)> {
        String::from_utf8_lossy(bytes)
            .lines()
            .filter_map(|line| {
                let idx = line.find("[JEST-BRIDGE-EVENT]")?;
                let payload = line[idx + "[JEST-BRIDGE-EVENT]".len()..].trim();
                let meta = serde_json::from_str::<JestBridgeEventMeta>(payload).ok()?;
                let test_path = meta.test_path.as_deref().unwrap_or("").replace('\\', "/");
                (!test_path.trim().is_empty()).then_some((test_path, payload.to_string()))
            })
            .collect()
    };

    let mut by_test_path: BTreeMap<String, Vec<BridgeConsoleEntry>> = BTreeMap::new();
    for (test_path, payload) in parse_lines(stdout_bytes)
        .into_iter()
        .chain(parse_lines(stderr_bytes).into_iter())
    {
        by_test_path
            .entry(test_path)
            .or_default()
            .push(BridgeConsoleEntry {
                message: Some(serde_json::Value::String(format!(
                    "[JEST-BRIDGE-EVENT] {payload}"
                ))),
                type_name: None,
                origin: None,
            });
    }
    by_test_path
}

fn merge_console_entries_into_bridge_json(
    bridge: &mut BridgeJson,
    extra_console_by_test_path: &BTreeMap<String, Vec<BridgeConsoleEntry>>,
) {
    bridge.test_results.iter_mut().for_each(|file| {
        let key = file.test_file_path.replace('\\', "/");
        let Some(extra) = extra_console_by_test_path.get(&key) else {
            return;
        };
        if extra.is_empty() {
            return;
        }
        match file.console.as_mut() {
            Some(existing) => existing.extend(extra.iter().cloned()),
            None => file.console = Some(extra.clone()),
        }
    });
}
