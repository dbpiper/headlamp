use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use path_slash::PathExt;
use tempfile::NamedTempFile;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::istanbul::{merge_istanbul_reports, read_istanbul_coverage_tree};
use headlamp_core::coverage::istanbul_pretty::format_istanbul_pretty;
use headlamp_core::coverage::lcov::{merge_reports, read_lcov_file, resolve_lcov_paths_to_root};
use headlamp_core::coverage::model::{CoverageReport, apply_statement_totals_to_report};
use headlamp_core::coverage::print::{
    PrintOpts, filter_report, render_report_text, should_render_hotspots,
};
use headlamp_core::coverage::thresholds::compare_thresholds_and_print_if_needed;
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::vitest::render_vitest_from_test_model;
use headlamp_core::selection::dependency_language::DependencyLanguageId;
use headlamp_core::selection::related_tests::select_related_tests;
use headlamp_core::selection::relevance::augment_rank_with_priority_paths;
use headlamp_core::selection::route_index::{discover_tests_for_http_paths, get_route_index};
use headlamp_core::selection::transitive_seed_refine::{
    filter_tests_by_transitive_seed, max_depth_from_args,
};
use headlamp_core::test_model::TestConsoleEntry;
use headlamp_core::test_model::TestRunModel;
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
use crate::live_progress::{LiveProgress, live_progress_mode};
use crate::parallel_stride::run_parallel_stride;
use crate::run::{RunError, run_bootstrap};
use crate::streaming::{OutputStream, StreamAction, StreamAdapter, run_streaming_capture_tail};

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
    let project_configs = if discovered_project_configs.is_empty() {
        vec![repo_root.to_path_buf()]
    } else {
        discovered_project_configs
    };

    let selection_exclude_globs = exclude_globs_for_selection(&args.exclude_globs);

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

    let dependency_language = args
        .dependency_language
        .unwrap_or(DependencyLanguageId::TsJs);

    let related_selection = if selection_is_tests_only {
        headlamp_core::selection::related_tests::RelatedTestSelection {
            selected_test_paths_abs: selection_paths_abs.clone(),
            rank_by_abs_path: BTreeMap::new(),
        }
    } else {
        selection_key
            .as_ref()
            .map(|key| {
                cached_related(repo_root, key, args.no_cache, || {
                    find_related_tests_fast(
                        repo_root,
                        &production_seeds,
                        &DEFAULT_TEST_GLOBS,
                        &selection_exclude_globs,
                        FAST_RELATED_TIMEOUT,
                    )
                })
                .map(|fast_tests| {
                    if !fast_tests.is_empty() {
                        let augmented = augment_with_http_tests(
                            repo_root,
                            &production_seeds,
                            &selection_exclude_globs,
                            fast_tests,
                        );
                        if args.changed.is_some() || args.changed_depth.is_some() {
                            return refine_by_transitive_seed_scan(
                                RefineByTransitiveSeedScanArgs {
                                    repo_root,
                                    dependency_language,
                                    project_configs: &project_configs,
                                    jest_bin: &jest_bin,
                                    discovery_args: &discovery_args,
                                    production_seeds_abs: &production_seeds,
                                    candidate_tests_abs: augmented,
                                    max_depth: max_depth_from_args(args.changed_depth),
                                    no_cache: args.no_cache,
                                },
                            );
                        }
                        headlamp_core::selection::related_tests::RelatedTestSelection {
                            selected_test_paths_abs: augmented,
                            rank_by_abs_path: BTreeMap::new(),
                        }
                    } else {
                        if args.changed.is_some() || args.changed_depth.is_some() {
                            return refine_by_transitive_seed_scan(
                                RefineByTransitiveSeedScanArgs {
                                    repo_root,
                                    dependency_language,
                                    project_configs: &project_configs,
                                    jest_bin: &jest_bin,
                                    discovery_args: &discovery_args,
                                    production_seeds_abs: &production_seeds,
                                    candidate_tests_abs: vec![],
                                    max_depth: max_depth_from_args(args.changed_depth),
                                    no_cache: args.no_cache,
                                },
                            );
                        }
                        select_related_tests(
                            repo_root,
                            dependency_language,
                            &production_seeds,
                            &selection_exclude_globs,
                        )
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

    let directness_rank_base = compute_directness_rank_base(
        repo_root,
        &selection_paths_abs,
        &selection_exclude_globs,
        args.no_cache,
    )?;
    let directness_rank = augment_rank_with_priority_paths(
        &directness_rank_base,
        &related_selection.selected_test_paths_abs,
    );

    let tmp = std::env::temp_dir().join("headlamp").join("jest");
    let reporter_path = write_asset(&tmp.join("reporter.cjs"), JEST_REPORTER_BYTES)?;
    let setup_path = write_asset(&tmp.join("setup.cjs"), JEST_SETUP_BYTES)?;
    let out_json_base = tmp.join(format!("jest-bridge-{}", std::process::id()));

    let name_pattern_only_for_discovery =
        should_skip_run_tests_by_path_for_name_pattern_only(args, &selection_paths_abs);

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
    ];
    let base_cmd_args = if name_pattern_only_for_discovery {
        base_cmd_args
    } else {
        base_cmd_args
            .iter()
            .cloned()
            .chain(std::iter::once("--runTestsByPath".to_string()))
            .collect::<Vec<_>>()
    };

    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );

    #[derive(Debug)]
    struct ProjectRunOutput {
        exit_code: i32,
        bridge: Option<TestRunModel>,
        captured_stdout: Vec<String>,
        captured_stderr: Vec<String>,
        coverage_failure_lines: Vec<String>,
        raw_output: String,
    }

    #[derive(Debug)]
    struct JestStreamingAdapter {
        emit_raw_lines: bool,
        captured_stdout: Vec<String>,
        captured_stderr: Vec<String>,
        extra_bridge_entries_by_test_path: BTreeMap<String, Vec<TestConsoleEntry>>,
    }

    impl JestStreamingAdapter {
        fn new(emit_raw_lines: bool) -> Self {
            Self {
                emit_raw_lines,
                captured_stdout: vec![],
                captured_stderr: vec![],
                extra_bridge_entries_by_test_path: BTreeMap::new(),
            }
        }

        fn push_non_event_line(&mut self, stream: OutputStream, line: &str) {
            match stream {
                OutputStream::Stdout => self.captured_stdout.push(line.to_string()),
                OutputStream::Stderr => self.captured_stderr.push(line.to_string()),
            }
        }

        fn push_bridge_event_line(&mut self, line: &str) {
            let Some(payload) = line.strip_prefix("[JEST-BRIDGE-EVENT] ") else {
                return;
            };
            let meta = serde_json::from_str::<JestBridgeEventMeta>(payload).ok();
            let test_path = meta
                .as_ref()
                .and_then(|m| m.test_path.as_deref())
                .unwrap_or("")
                .replace('\\', "/");
            if test_path.trim().is_empty() {
                return;
            }
            self.extra_bridge_entries_by_test_path
                .entry(test_path)
                .or_default()
                .push(TestConsoleEntry {
                    message: Some(serde_json::Value::String(format!(
                        "[JEST-BRIDGE-EVENT] {payload}"
                    ))),
                    type_name: None,
                    origin: None,
                });
        }
    }

    impl StreamAdapter for JestStreamingAdapter {
        fn on_start(&mut self) -> Option<String> {
            Some("jest".to_string())
        }

        fn on_line(&mut self, stream: OutputStream, line: &str) -> Vec<StreamAction> {
            if line.starts_with("[JEST-BRIDGE-EVENT] ") {
                self.push_bridge_event_line(line);
                return vec![];
            }

            self.push_non_event_line(stream, line);

            if !self.emit_raw_lines {
                return vec![];
            }
            match stream {
                OutputStream::Stdout => vec![StreamAction::PrintStdout(line.to_string())],
                OutputStream::Stderr => vec![StreamAction::PrintStderr(line.to_string())],
            }
        }
    }

    let stride = if args.sequential { 1 } else { 3 };
    let live_progress = LiveProgress::start(project_configs.len(), mode);
    let per_project_results = run_parallel_stride(&project_configs, stride, |cfg_path, index| {
        let cfg_token = config_token(repo_root, cfg_path);
        live_progress.set_current_label(cfg_token.clone());

        let tests_for_project = if selection_paths_abs.is_empty() {
            if name_pattern_only_for_discovery {
                vec![]
            } else {
                let mut list_args = discovery_args.clone();
                list_args.extend(["--config".to_string(), cfg_token.clone()]);
                discover_jest_list_tests_cached_with_timeout(
                    cfg_path.parent().unwrap_or(repo_root),
                    &jest_bin,
                    &list_args,
                    args.no_cache,
                    JEST_LIST_TESTS_TIMEOUT,
                )?
            }
        } else {
            filter_candidates_for_project(
                repo_root,
                &jest_bin,
                &discovery_args,
                cfg_path,
                &related_selection.selected_test_paths_abs,
            )?
        };

        if selection_paths_abs.is_empty()
            && tests_for_project.is_empty()
            && !name_pattern_only_for_discovery
        {
            live_progress.increment_done(1);
            return Ok(ProjectRunOutput {
                exit_code: 0,
                bridge: None,
                captured_stdout: vec![],
                captured_stderr: vec![],
                coverage_failure_lines: vec![],
                raw_output: String::new(),
            });
        }

        if !selection_paths_abs.is_empty() && tests_for_project.is_empty() {
            live_progress.increment_done(1);
            return Ok(ProjectRunOutput {
                exit_code: 0,
                bridge: None,
                captured_stdout: vec![],
                captured_stderr: vec![],
                coverage_failure_lines: vec![],
                raw_output: String::new(),
            });
        }

        let out_json = out_json_base.with_extension(format!("{index}.json"));

        let mut cmd_args = base_cmd_args.clone();
        cmd_args.extend(["--config".to_string(), cfg_token.clone()]);
        cmd_args.extend(args.runner_args.iter().cloned());
        ensure_watchman_disabled_by_default(&mut cmd_args);
        if args.no_cache && !cmd_args.iter().any(|t| t == "--no-cache") {
            cmd_args.push("--no-cache".to_string());
        }
        if args.sequential {
            cmd_args.push("--runInBand".to_string());
        }
        if args.collect_coverage {
            if !cmd_args
                .iter()
                .any(|t| t == "--coverage" || t.starts_with("--coverage="))
            {
                cmd_args.extend(
                    [
                        "--coverage",
                        "--coverageProvider=babel",
                        "--coverageReporters=lcov",
                        "--coverageReporters=json",
                        "--coverageReporters=text-summary",
                    ]
                    .into_iter()
                    .map(String::from),
                );
            }
            cmd_args.push(format!(
                "--coverageDirectory={}",
                coverage_dir_for_config(cfg_path)
            ));
            cmd_args.extend(collect_coverage_from_args(
                repo_root,
                &selection_paths_abs,
                &args.selection_paths,
            ));
        }
        if args.show_logs {
            cmd_args.push("--no-silent".to_string());
        }
        if !tests_for_project.is_empty() {
            cmd_args.extend(tests_for_project);
        } else if !name_pattern_only_for_discovery {
            cmd_args.extend(args.selection_paths.iter().cloned());
        }

        let emit_raw_lines = args.ci;
        let mut command = std::process::Command::new(&jest_bin);
        command
            .args(cmd_args)
            .current_dir(repo_root)
            .env("NODE_ENV", "test")
            .env("FORCE_COLOR", "3")
            .env("JEST_BRIDGE_OUT", out_json.to_string_lossy().to_string());
        let mut adapter = JestStreamingAdapter::new(emit_raw_lines);
        let (exit_code, _tail) =
            run_streaming_capture_tail(command, &live_progress, &mut adapter, 1024 * 1024)?;

        let captured_stdout = adapter.captured_stdout;
        let captured_stderr = adapter.captured_stderr;
        let extra_bridge_entries_by_test_path = adapter.extra_bridge_entries_by_test_path;
        let raw_output = format!(
            "{}\n{}",
            captured_stdout.join("\n"),
            captured_stderr.join("\n")
        );
        let coverage_failure_lines = extract_coverage_failure_lines(raw_output.as_bytes(), b"");

        let bridge = std::fs::read_to_string(&out_json)
            .ok()
            .and_then(|raw| serde_json::from_str::<TestRunModel>(&raw).ok())
            .map(|mut bridge| {
                merge_console_entries_into_bridge_json(
                    &mut bridge,
                    &extra_bridge_entries_by_test_path,
                );
                if name_pattern_only_for_discovery {
                    bridge = filter_bridge_for_name_pattern_only(bridge);
                }
                bridge
            });

        Ok(ProjectRunOutput {
            exit_code,
            bridge,
            captured_stdout,
            captured_stderr,
            coverage_failure_lines,
            raw_output,
        })
    })?;
    live_progress.finish();

    let mut exit_codes: Vec<i32> = vec![];
    let mut bridges: Vec<TestRunModel> = vec![];
    let mut captured_stdout_all: Vec<String> = vec![];
    let mut captured_stderr_all: Vec<String> = vec![];
    let mut coverage_failure_lines: IndexSet<String> = IndexSet::new();
    let mut raw_output_all: Vec<String> = vec![];
    for result in per_project_results {
        exit_codes.push(result.exit_code);
        captured_stdout_all.extend(result.captured_stdout);
        captured_stderr_all.extend(result.captured_stderr);
        result.coverage_failure_lines.into_iter().for_each(|ln| {
            coverage_failure_lines.insert(ln);
        });
        raw_output_all.push(result.raw_output);
        if let Some(bridge) = result.bridge {
            bridges.push(bridge);
        }
    }

    let mut exit_code = exit_codes.into_iter().max().unwrap_or(1);

    if let Some(merged) = merge_bridge_json(&bridges, &directness_rank) {
        let ctx = make_ctx(
            repo_root,
            None,
            exit_code != 0,
            args.show_logs,
            args.editor_cmd.clone(),
        );
        let pretty = render_vitest_from_test_model(&merged, &ctx, args.only_failures);
        let combined = raw_output_all.join("\n");
        let maybe_merged = (!args.only_failures && looks_sparse(&pretty)).then(|| {
            let raw_also = headlamp_core::format::raw_jest::format_jest_output_vitest(
                &combined,
                &ctx,
                args.only_failures,
            );
            merge_sparse_bridge_and_raw(&pretty, &raw_also)
        });
        let final_text = maybe_merged.as_deref().unwrap_or(&pretty);
        if !final_text.trim().is_empty() {
            println!("{final_text}");
        }
    } else {
        let combined = raw_output_all.join("\n");
        let ctx = make_ctx(
            repo_root,
            None,
            combined.contains("FAIL"),
            args.show_logs,
            args.editor_cmd.clone(),
        );
        let formatted = headlamp_core::format::raw_jest::format_jest_output_vitest(
            &combined,
            &ctx,
            args.only_failures,
        );
        if !formatted.trim().is_empty() {
            println!("{formatted}");
        } else {
            for line in captured_stdout_all {
                println!("{line}");
            }
            for line in captured_stderr_all {
                eprintln!("{line}");
            }
        }
    }

    if args.collect_coverage {
        let jest_cov_dir = repo_root.join("coverage").join("jest");
        let json_tree = read_istanbul_coverage_tree(&jest_cov_dir);
        let json_reports = json_tree.into_iter().map(|(_, r)| r).collect::<Vec<_>>();
        let merged_json =
            (!json_reports.is_empty()).then(|| merge_istanbul_reports(&json_reports, repo_root));

        let mut lcov_candidates: Vec<PathBuf> = vec![repo_root.join("coverage").join("lcov.info")];
        if jest_cov_dir.exists() {
            WalkBuilder::new(&jest_cov_dir)
                .hidden(false)
                .git_ignore(false)
                .git_global(false)
                .git_exclude(false)
                .build()
                .map_while(Result::ok)
                .filter(|dent| dent.file_type().is_some_and(|t| t.is_file()))
                .filter(|dent| {
                    dent.path().file_name().and_then(|x| x.to_str()) == Some("lcov.info")
                })
                .for_each(|dent| lcov_candidates.push(dent.into_path()));
        }

        let reports = lcov_candidates
            .iter()
            .filter(|p| p.exists())
            .filter_map(|p| read_lcov_file(p).ok())
            .collect::<Vec<_>>();

        let resolved_lcov = (!reports.is_empty()).then(|| {
            let merged = merge_reports(&reports, repo_root);
            resolve_lcov_paths_to_root(merged, repo_root)
        });

        let threshold_report =
            build_jest_threshold_report(resolved_lcov.clone(), merged_json.clone());

        let resolved = merged_json.or_else(|| resolved_lcov.clone());

        let print_opts =
            PrintOpts::for_run(args, headlamp_core::format::terminal::is_output_terminal());

        if args.coverage_ui != headlamp_core::config::CoverageUi::Jest {
            if let Some(pretty) = format_istanbul_pretty(
                repo_root,
                &jest_cov_dir,
                &print_opts,
                &selection_paths_abs,
                &args.include_globs,
                &args.exclude_globs,
                args.coverage_detail,
            ) {
                println!("{pretty}");
            } else if let Some(resolved) = resolved {
                let filtered = filter_report(
                    resolved,
                    repo_root,
                    &args.include_globs,
                    &args.exclude_globs,
                );
                let include_hotspots = should_render_hotspots(args.coverage_detail);
                println!(
                    "{}",
                    render_report_text(&filtered, &print_opts, repo_root, include_hotspots)
                );
            }
        } else {
            // With --coverage-ui=jest, match TS behavior by not rendering Headlamp's coverage report.
        }
        let thresholds_failed = compare_thresholds_and_print_if_needed(
            args.coverage_thresholds.as_ref(),
            threshold_report.as_ref(),
        );
        if exit_code == 0 && thresholds_failed {
            exit_code = 1;
        } else if should_print_coverage_threshold_failure_summary(
            exit_code,
            &coverage_failure_lines,
        ) {
            print_coverage_threshold_failure_summary(&coverage_failure_lines);
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
    route_tests.into_iter().for_each(|t| {
        combined.insert(t);
    });
    related_tests_abs.into_iter().for_each(|t| {
        combined.insert(t);
    });
    combined.into_iter().collect::<Vec<_>>()
}

fn exclude_globs_for_selection(exclude_globs: &[String]) -> Vec<String> {
    exclude_globs
        .iter()
        .filter(|glob| glob.as_str() != "**/tests/**")
        .cloned()
        .collect::<Vec<_>>()
}

pub(crate) fn build_jest_threshold_report(
    resolved_lcov: Option<CoverageReport>,
    merged_json: Option<CoverageReport>,
) -> Option<CoverageReport> {
    let statement_totals_by_path = merged_json.as_ref().map(|report| {
        report
            .files
            .iter()
            .filter_map(|file| {
                Some((
                    file.path.clone(),
                    (file.statements_total?, file.statements_covered?),
                ))
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    });

    match (resolved_lcov, statement_totals_by_path, merged_json) {
        (Some(lcov), Some(statement_totals_by_path), _merged_json) => Some(
            apply_statement_totals_to_report(lcov, &statement_totals_by_path),
        ),
        (Some(lcov), None, _merged_json) => Some(lcov),
        (None, _maybe_map, merged_json) => merged_json,
    }
}

#[derive(Debug)]
struct RefineByTransitiveSeedScanArgs<'a> {
    repo_root: &'a Path,
    dependency_language: DependencyLanguageId,
    project_configs: &'a [PathBuf],
    jest_bin: &'a Path,
    discovery_args: &'a [String],
    production_seeds_abs: &'a [String],
    candidate_tests_abs: Vec<String>,
    max_depth: headlamp_core::selection::transitive_seed_refine::MaxDepth,
    no_cache: bool,
}

fn refine_by_transitive_seed_scan(
    args: RefineByTransitiveSeedScanArgs<'_>,
) -> headlamp_core::selection::related_tests::RelatedTestSelection {
    let RefineByTransitiveSeedScanArgs {
        repo_root,
        dependency_language,
        project_configs,
        jest_bin,
        discovery_args,
        production_seeds_abs,
        candidate_tests_abs,
        max_depth,
        no_cache,
    } = args;
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
                no_cache,
                JEST_LIST_TESTS_TIMEOUT,
            )
            .ok()
        })
        .flatten()
        .collect::<IndexSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut kept = filter_tests_by_transitive_seed(
        repo_root,
        dependency_language,
        &all_tests,
        production_seeds_abs,
        max_depth,
    );
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

fn compute_directness_rank_base(
    repo_root: &Path,
    selection_paths_abs: &[String],
    exclude_globs: &[String],
    no_cache: bool,
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

    let related = cached_related(repo_root, &selection_key, no_cache, || {
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
    let mut classifier = headlamp_core::project::classify::ProjectClassifier::for_path(
        DependencyLanguageId::TsJs,
        Path::new(candidate_path),
    );
    matches!(
        classifier.classify_abs_path(Path::new(candidate_path)),
        headlamp_core::project::classify::FileKind::Test
            | headlamp_core::project::classify::FileKind::Mixed
    )
}

fn filter_bridge_for_name_pattern_only(mut bridge: TestRunModel) -> TestRunModel {
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

fn should_skip_run_tests_by_path_for_name_pattern_only(
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

fn coverage_dir_for_config(cfg_path: &Path) -> String {
    let base = cfg_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("default");
    let safe = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("coverage/jest/{safe}")
}

fn collect_coverage_from_args(
    repo_root: &Path,
    selection_paths_abs: &[String],
    selection_paths_tokens: &[String],
) -> Vec<String> {
    let explicit_prod_abs = selection_paths_abs
        .iter()
        .filter(|abs| abs.contains('/') && !looks_like_test_path(abs))
        .filter_map(|abs| {
            Path::new(abs)
                .strip_prefix(repo_root)
                .ok()
                .and_then(|p| p.to_str())
                .map(|rel| rel.replace('\\', "/"))
        })
        .filter(|rel| !rel.is_empty() && !rel.starts_with("../") && !rel.starts_with("./../"))
        .map(|rel| {
            if rel.starts_with("./") {
                rel
            } else {
                format!("./{rel}")
            }
        })
        .collect::<Vec<_>>();

    let mut out: Vec<String> = vec![];
    for rel in explicit_prod_abs {
        out.push("--collectCoverageFrom".to_string());
        out.push(rel);
    }
    if out.is_empty() {
        // Keep behavior stable: no extra args when there are no explicit production selections.
        let _ = selection_paths_tokens;
    }
    out
}

fn ensure_watchman_disabled_by_default(jest_args: &mut Vec<String>) {
    let has_watchman_flag = jest_args
        .iter()
        .any(|tok| tok == "--no-watchman" || tok == "--watchman" || tok.starts_with("--watchman="));
    if !has_watchman_flag {
        jest_args.push("--no-watchman".to_string());
    }
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

fn extract_coverage_failure_lines(stdout_bytes: &[u8], stderr_bytes: &[u8]) -> Vec<String> {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout_bytes),
        String::from_utf8_lossy(stderr_bytes)
    );
    let mut out: IndexSet<String> = IndexSet::new();
    for line in text.lines() {
        let line_without_ansi = headlamp_core::format::stacks::strip_ansi_simple(line);
        let trimmed = line_without_ansi.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(formatted) = parse_global_coverage_threshold_failure_line(trimmed) {
            out.insert(formatted);
            continue;
        }
        if trimmed.to_ascii_lowercase().contains("does not meet")
            && trimmed.to_ascii_lowercase().contains("coverage for ")
        {
            out.insert(trimmed.to_string());
        }
    }
    out.into_iter().collect()
}

fn parse_global_coverage_threshold_failure_line(line: &str) -> Option<String> {
    let prefix = r#"Jest: "global" coverage threshold for "#;
    let rest = line.strip_prefix(prefix)?;
    let (metric_raw, rest) = rest.split_once(" (")?;
    let (expected_raw, rest) = rest.split_once("%)")?;
    let actual_raw = rest.split_once("not met:")?.1.trim();
    let actual_raw = actual_raw.strip_suffix('%')?.trim();
    let expected: f64 = expected_raw.trim().parse().ok()?;
    let actual: f64 = actual_raw.parse().ok()?;
    let short = (expected - actual).max(0.0);
    let metric = titlecase_first(metric_raw.trim());
    Some(format!(
        "{metric}: {actual:.2}% < {expected:.0}% (short {short:.2}%)"
    ))
}

fn titlecase_first(text: &str) -> String {
    let mut chars = text.chars();
    let first = chars.next().unwrap_or_default();
    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
}

fn print_coverage_threshold_failure_summary(lines: &IndexSet<String>) {
    println!();
    println!("Coverage thresholds not met");
    if lines.is_empty() {
        println!(" See tables above and jest coverageThreshold.");
        return;
    }
    lines.iter().for_each(|line| println!(" {line}"));
}

pub(crate) fn should_print_coverage_threshold_failure_summary(
    exit_code: i32,
    coverage_failure_lines: &IndexSet<String>,
) -> bool {
    exit_code != 0 && !coverage_failure_lines.is_empty()
}

fn looks_sparse(pretty: &str) -> bool {
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

fn merge_sparse_bridge_and_raw(bridge_pretty: &str, raw_pretty: &str) -> String {
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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JestBridgeEventMeta {
    #[serde(rename = "testPath")]
    test_path: Option<String>,
}

fn merge_console_entries_into_bridge_json(
    bridge: &mut TestRunModel,
    extra_console_by_test_path: &BTreeMap<String, Vec<TestConsoleEntry>>,
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
