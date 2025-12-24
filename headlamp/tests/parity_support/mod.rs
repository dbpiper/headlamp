#![allow(dead_code)]

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

pub mod cluster;
pub mod diagnostics;
pub mod diff_report;
pub mod normalize;
pub mod parity_meta;
pub mod token_ast;
pub use normalize::normalize;
pub use normalize::normalize_tty_ui;
pub use parity_meta::ParitySideLabel;

static CAPTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn next_capture_id() -> usize {
    CAPTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone)]
pub struct ParityRunSpec {
    pub cwd: PathBuf,
    pub program: PathBuf,
    pub side_label: ParitySideLabel,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub tty_columns: Option<usize>,
    pub stdout_piped: bool,
}

#[derive(Debug, Clone)]
pub struct ParityRunGroup {
    pub sides: Vec<ParityRunSpec>,
}

pub fn assert_parity_normalized_outputs(
    repo: &Path,
    case: &str,
    side_0_exit: i32,
    side_0_out: &str,
    side_1_exit: i32,
    side_1_out: &str,
) {
    let label_0 = ParitySideLabel {
        binary: "unknown".to_string(),
        runner_stack: "unknown".to_string(),
    };
    let label_1 = ParitySideLabel {
        binary: "unknown".to_string(),
        runner_stack: "unknown".to_string(),
    };
    let compare_input = parity_meta::ParityCompareInput {
        sides: vec![
            parity_meta::ParityCompareSideInput {
                label: label_0,
                exit: side_0_exit,
                raw: side_0_out.to_string(),
                normalized: side_0_out.to_string(),
                meta: parity_meta::ParitySideMeta {
                    raw_bytes: side_0_out.as_bytes().len(),
                    raw_lines: side_0_out.lines().count(),
                    normalized_bytes: side_0_out.as_bytes().len(),
                    normalized_lines: side_0_out.lines().count(),
                    normalization: parity_meta::NormalizationMeta {
                        normalizer: parity_meta::NormalizerKind::NonTty,
                        used_fallback: false,
                        last_failed_tests_line: None,
                        last_test_files_line: None,
                        last_box_table_top_line: None,
                        stages: vec![],
                    },
                },
            },
            parity_meta::ParityCompareSideInput {
                label: label_1,
                exit: side_1_exit,
                raw: side_1_out.to_string(),
                normalized: side_1_out.to_string(),
                meta: parity_meta::ParitySideMeta {
                    raw_bytes: side_1_out.as_bytes().len(),
                    raw_lines: side_1_out.lines().count(),
                    normalized_bytes: side_1_out.as_bytes().len(),
                    normalized_lines: side_1_out.lines().count(),
                    normalization: parity_meta::NormalizationMeta {
                        normalizer: parity_meta::NormalizerKind::NonTty,
                        used_fallback: false,
                        last_failed_tests_line: None,
                        last_test_files_line: None,
                        last_box_table_top_line: None,
                        stages: vec![],
                    },
                },
            },
        ],
    };

    assert_parity_with_diagnostics(repo, case, &compare_input, None);
}

pub fn assert_parity_non_tty_with_diagnostics(
    repo: &Path,
    case: &str,
    side_0_exit: i32,
    side_0_raw: String,
    side_1_exit: i32,
    side_1_raw: String,
    run_group: Option<&ParityRunGroup>,
) {
    let (side_0_normalized, side_0_meta) = normalize::normalize_with_meta(side_0_raw.clone(), repo);
    let (side_1_normalized, side_1_meta) = normalize::normalize_with_meta(side_1_raw.clone(), repo);

    let side_0_raw_bytes = side_0_raw.as_bytes().len();
    let side_0_raw_lines = side_0_raw.lines().count();
    let side_0_norm_bytes = side_0_normalized.as_bytes().len();
    let side_0_norm_lines = side_0_normalized.lines().count();

    let side_1_raw_bytes = side_1_raw.as_bytes().len();
    let side_1_raw_lines = side_1_raw.lines().count();
    let side_1_norm_bytes = side_1_normalized.as_bytes().len();
    let side_1_norm_lines = side_1_normalized.lines().count();

    let label_0 = run_group
        .and_then(|group| group.sides.first())
        .map(|spec| spec.side_label.clone())
        .unwrap_or(ParitySideLabel {
            binary: "unknown".to_string(),
            runner_stack: "unknown".to_string(),
        });
    let label_1 = run_group
        .and_then(|group| group.sides.get(1))
        .map(|spec| spec.side_label.clone())
        .unwrap_or(ParitySideLabel {
            binary: "unknown".to_string(),
            runner_stack: "unknown".to_string(),
        });
    let compare = parity_meta::ParityCompareInput {
        sides: vec![
            parity_meta::ParityCompareSideInput {
                label: label_0,
                exit: side_0_exit,
                raw: side_0_raw,
                normalized: side_0_normalized,
                meta: parity_meta::ParitySideMeta {
                    raw_bytes: side_0_raw_bytes,
                    raw_lines: side_0_raw_lines,
                    normalized_bytes: side_0_norm_bytes,
                    normalized_lines: side_0_norm_lines,
                    normalization: side_0_meta,
                },
            },
            parity_meta::ParityCompareSideInput {
                label: label_1,
                exit: side_1_exit,
                raw: side_1_raw,
                normalized: side_1_normalized,
                meta: parity_meta::ParitySideMeta {
                    raw_bytes: side_1_raw_bytes,
                    raw_lines: side_1_raw_lines,
                    normalized_bytes: side_1_norm_bytes,
                    normalized_lines: side_1_norm_lines,
                    normalization: side_1_meta,
                },
            },
        ],
    };

    assert_parity_with_diagnostics(repo, case, &compare, run_group);
}

pub fn assert_parity_tty_ui_with_diagnostics(
    repo: &Path,
    case: &str,
    side_0_exit: i32,
    side_0_raw: String,
    side_1_exit: i32,
    side_1_raw: String,
    run_group: Option<&ParityRunGroup>,
) {
    let (side_0_normalized, side_0_meta) =
        normalize::normalize_tty_ui_with_meta(side_0_raw.clone(), repo);
    let (side_1_normalized, side_1_meta) =
        normalize::normalize_tty_ui_with_meta(side_1_raw.clone(), repo);

    let side_0_raw_bytes = side_0_raw.as_bytes().len();
    let side_0_raw_lines = side_0_raw.lines().count();
    let side_0_norm_bytes = side_0_normalized.as_bytes().len();
    let side_0_norm_lines = side_0_normalized.lines().count();

    let side_1_raw_bytes = side_1_raw.as_bytes().len();
    let side_1_raw_lines = side_1_raw.lines().count();
    let side_1_norm_bytes = side_1_normalized.as_bytes().len();
    let side_1_norm_lines = side_1_normalized.lines().count();

    let label_0 = run_group
        .and_then(|group| group.sides.first())
        .map(|spec| spec.side_label.clone())
        .unwrap_or(ParitySideLabel {
            binary: "unknown".to_string(),
            runner_stack: "unknown".to_string(),
        });
    let label_1 = run_group
        .and_then(|group| group.sides.get(1))
        .map(|spec| spec.side_label.clone())
        .unwrap_or(ParitySideLabel {
            binary: "unknown".to_string(),
            runner_stack: "unknown".to_string(),
        });
    let compare = parity_meta::ParityCompareInput {
        sides: vec![
            parity_meta::ParityCompareSideInput {
                label: label_0,
                exit: side_0_exit,
                raw: side_0_raw,
                normalized: side_0_normalized,
                meta: parity_meta::ParitySideMeta {
                    raw_bytes: side_0_raw_bytes,
                    raw_lines: side_0_raw_lines,
                    normalized_bytes: side_0_norm_bytes,
                    normalized_lines: side_0_norm_lines,
                    normalization: side_0_meta,
                },
            },
            parity_meta::ParityCompareSideInput {
                label: label_1,
                exit: side_1_exit,
                raw: side_1_raw,
                normalized: side_1_normalized,
                meta: parity_meta::ParitySideMeta {
                    raw_bytes: side_1_raw_bytes,
                    raw_lines: side_1_raw_lines,
                    normalized_bytes: side_1_norm_bytes,
                    normalized_lines: side_1_norm_lines,
                    normalization: side_1_meta,
                },
            },
        ],
    };

    assert_parity_with_diagnostics(repo, case, &compare, run_group);
}

pub fn assert_parity_with_diagnostics(
    repo: &Path,
    case: &str,
    compare: &parity_meta::ParityCompareInput,
    run_group: Option<&ParityRunGroup>,
) {
    if compare.sides.len() < 2 {
        return;
    }

    let all_exits_equal = compare
        .sides
        .first()
        .map(|first| compare.sides.iter().all(|side| side.exit == first.exit))
        .unwrap_or(true);
    let all_normalized_equal = compare
        .sides
        .first()
        .map(|first| {
            compare
                .sides
                .iter()
                .all(|side| side.normalized == first.normalized)
        })
        .unwrap_or(true);
    if all_exits_equal && all_normalized_equal {
        return;
    }

    let safe = case
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();

    let repo_key = repo.file_name().unwrap_or_default();
    let dump_dir = std::env::temp_dir()
        .join("headlamp-parity-dumps")
        .join(repo_key);
    let _ = std::fs::create_dir_all(&dump_dir);

    let pivot_index = cluster::pick_pivot_index(compare);
    let clusters = cluster::cluster_indices_by_normalized(compare);

    let report_path = dump_dir.join(format!("{safe}--report.txt"));
    let meta_path = dump_dir.join(format!("{safe}--meta.json"));
    let analysis_path = dump_dir.join(format!("{safe}--analysis.json"));
    let reruns_dir = dump_dir.join(format!("{safe}--reruns"));

    let side_dump_paths = compare
        .sides
        .iter()
        .map(|side| {
            let side_key = side.label.file_safe_label();
            let normalized = dump_dir.join(format!("{safe}--{side_key}--normalized.txt"));
            let raw = dump_dir.join(format!("{safe}--{side_key}--raw.txt"));
            let tokens = dump_dir.join(format!("{safe}--{side_key}--tokens.json"));
            let ast = dump_dir.join(format!("{safe}--{side_key}--ast.json"));
            (normalized, raw, tokens, ast)
        })
        .collect::<Vec<_>>();

    compare.sides.iter().zip(side_dump_paths.iter()).for_each(
        |(side, (normalized_path, raw_path, _, _))| {
            let _ = std::fs::write(normalized_path, &side.normalized);
            let _ = std::fs::write(raw_path, &side.raw);
        },
    );

    let token_ast_summary = TokenAstSummary {
        sides: compare
            .sides
            .iter()
            .zip(side_dump_paths.iter())
            .map(|(side, (_, _, tokens_path, ast_path))| {
                let raw = token_ast::build_token_stream(&side.raw);
                let normalized = token_ast::build_token_stream(&side.normalized);
                let doc_ast = token_ast::build_document_ast(&side.normalized);
                let _ = std::fs::File::create(tokens_path)
                    .ok()
                    .and_then(|mut file| serde_json::to_writer_pretty(&mut file, &raw).ok());
                let _ = std::fs::File::create(ast_path)
                    .ok()
                    .and_then(|mut file| serde_json::to_writer_pretty(&mut file, &doc_ast).ok());
                TokenAstSideSummary {
                    label: side.label.clone(),
                    raw: raw.stats,
                    normalized: normalized.stats,
                    block_order: doc_ast.blocks.into_iter().map(|b| b.hash).collect(),
                }
            })
            .collect(),
    };
    let reruns = run_group
        .and_then(|group| run_diagnostic_reruns(&reruns_dir, group))
        .unwrap_or_default();
    let meta_file = ParityMetaFile {
        sides: compare
            .sides
            .iter()
            .map(|side| ParityMetaSide {
                label: &side.label,
                meta: &side.meta,
            })
            .collect(),
        token_ast: token_ast_summary,
        reruns,
    };
    if let Ok(mut file) = std::fs::File::create(&meta_path) {
        let _ = serde_json::to_writer_pretty(&mut file, &meta_file);
    }

    let diff_paths = clusters
        .iter()
        .filter(|cluster| !cluster.member_indices.contains(&pivot_index))
        .filter_map(|cluster| {
            cluster.member_indices.iter().copied().min_by(|&a, &b| {
                compare.sides[a]
                    .label
                    .display_label()
                    .cmp(&compare.sides[b].label.display_label())
            })
        })
        .map(|other_index| {
            let pivot_key = compare.sides[pivot_index].label.file_safe_label();
            let other_key = compare.sides[other_index].label.file_safe_label();
            let diff_path =
                dump_dir.join(format!("{safe}--diff--{pivot_key}--vs--{other_key}.txt"));
            let diff = similar_asserts::SimpleDiff::from_str(
                &compare.sides[pivot_index].normalized,
                &compare.sides[other_index].normalized,
                &compare.sides[pivot_index].label.display_label(),
                &compare.sides[other_index].label.display_label(),
            )
            .to_string();
            let _ = std::fs::write(&diff_path, &diff);
            diff_path.to_string_lossy().to_string()
        })
        .collect::<Vec<_>>();

    let artifacts = diagnostics::ArtifactPaths {
        sides: compare
            .sides
            .iter()
            .zip(side_dump_paths.iter())
            .map(
                |(side, (normalized, raw, tokens, ast))| diagnostics::SideArtifactPaths {
                    label: side.label.clone(),
                    normalized: normalized.to_string_lossy().to_string(),
                    raw: raw.to_string_lossy().to_string(),
                    tokens: tokens.to_string_lossy().to_string(),
                    ast: ast.to_string_lossy().to_string(),
                },
            )
            .collect(),
        diffs: diff_paths.clone(),
        report: report_path.to_string_lossy().to_string(),
        meta: meta_path.to_string_lossy().to_string(),
        analysis: analysis_path.to_string_lossy().to_string(),
        reruns_dir: reruns_dir.to_string_lossy().to_string(),
    };
    let bundle = diagnostics::build_bundle(repo, case, artifacts, compare, &meta_file.reruns);
    if let Ok(mut file) = std::fs::File::create(&analysis_path) {
        let _ = serde_json::to_writer_pretty(&mut file, &bundle);
    }

    let report = diff_report::build_parity_report_with_meta(compare);
    let _ = std::fs::write(&report_path, &report);
    let report_for_panic = truncate_report_for_panic(&report);

    let side_labels = compare
        .sides
        .iter()
        .enumerate()
        .map(|(index, side)| {
            format!(
                "  - side_{index}: exit={} label={}",
                side.exit,
                side.label.display_label()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let cluster_lines = clusters
        .iter()
        .enumerate()
        .map(|(cluster_index, cluster)| {
            let labels = cluster
                .member_indices
                .iter()
                .map(|&i| compare.sides[i].label.display_label())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "  - cluster_{cluster_index}: size={} [{}]",
                cluster.member_indices.len(),
                labels
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let diffs_for_panic = diff_paths
        .iter()
        .map(|p| format!("  - {p}"))
        .collect::<Vec<_>>()
        .join("\n");

    panic!(
        "parity mismatch ({case}) repo={}\n\nSIDES:\n{side_labels}\n\nCLUSTERS:\n{cluster_lines}\n\nREPORT:\n{}\n\nDIFF_FILES:\n{diffs_for_panic}\nREPORT_FILE: {}\nMETA: {}\nANALYSIS: {}\nRERUNS_DIR: {}",
        repo.display(),
        report_for_panic,
        report_path.display(),
        meta_path.display(),
        analysis_path.display(),
        reruns_dir.display(),
    );
}

#[derive(Debug, Serialize)]
struct ParityMetaFile<'a> {
    sides: Vec<ParityMetaSide<'a>>,
    token_ast: TokenAstSummary,
    reruns: Vec<RerunMeta>,
}

#[derive(Debug, Serialize)]
struct ParityMetaSide<'a> {
    label: &'a ParitySideLabel,
    meta: &'a parity_meta::ParitySideMeta,
}

#[derive(Debug, Serialize)]
struct TokenAstSummary {
    sides: Vec<TokenAstSideSummary>,
}

#[derive(Debug, Serialize)]
struct TokenAstSideSummary {
    label: ParitySideLabel,
    raw: token_ast::TokenStats,
    normalized: token_ast::TokenStats,
    block_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RerunMeta {
    variant: String,
    sides: Vec<RerunSideMeta>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RerunSideMeta {
    label: ParitySideLabel,
    code: i32,
    path: String,
    bytes: usize,
    tokens: token_ast::TokenStats,
    blocks: usize,
}

fn run_diagnostic_reruns(dir: &Path, run_group: &ParityRunGroup) -> Option<Vec<RerunMeta>> {
    let _ = std::fs::create_dir_all(dir);
    let variants = [
        RerunVariant::non_tty("no_color", &[("NO_COLOR", "1")]),
        RerunVariant::non_tty("term_dumb", &[("TERM", "dumb")]),
        RerunVariant::tty("tty_80", 80),
        RerunVariant::tty("tty_120", 120),
        RerunVariant::tty("tty_160", 160),
    ];
    Some(
        variants
            .into_iter()
            .filter_map(|variant| run_one_rerun(dir, run_group, &variant))
            .collect(),
    )
}

#[derive(Debug, Clone)]
struct RerunVariant {
    id: &'static str,
    mode: RerunMode,
}

#[derive(Debug, Clone)]
enum RerunMode {
    NonTty {
        extra_env: Vec<(&'static str, &'static str)>,
    },
    Tty {
        columns: usize,
    },
}

impl RerunVariant {
    fn non_tty(id: &'static str, extra_env: &[(&'static str, &'static str)]) -> Self {
        Self {
            id,
            mode: RerunMode::NonTty {
                extra_env: extra_env.to_vec(),
            },
        }
    }

    fn tty(id: &'static str, columns: usize) -> Self {
        Self {
            id,
            mode: RerunMode::Tty { columns },
        }
    }
}

fn run_one_rerun(
    dir: &Path,
    run_group: &ParityRunGroup,
    variant: &RerunVariant,
) -> Option<RerunMeta> {
    let sides = run_group
        .sides
        .iter()
        .map(|spec| {
            let side_key = spec.side_label.file_safe_label();
            let side_path = dir.join(format!("{side_key}--{}.txt", variant.id));

            let (side_code, side_out) = run_variant_side(spec, variant);
            let _ = std::fs::write(&side_path, &side_out);
            let side_tokens = token_ast::build_token_stream(&side_out).stats;
            let side_blocks = token_ast::build_document_ast(&side_out).blocks.len();
            RerunSideMeta {
                label: spec.side_label.clone(),
                code: side_code,
                path: side_path.to_string_lossy().to_string(),
                bytes: side_out.as_bytes().len(),
                tokens: side_tokens,
                blocks: side_blocks,
            }
        })
        .collect::<Vec<_>>();

    Some(RerunMeta {
        variant: variant.id.to_string(),
        sides,
    })
}

fn run_variant_side(spec: &ParityRunSpec, variant: &RerunVariant) -> (i32, String) {
    let mut cmd = Command::new(&spec.program);
    cmd.current_dir(&spec.cwd);
    spec.args.iter().for_each(|arg| {
        cmd.arg(arg);
    });
    spec.env.iter().for_each(|(k, v)| {
        cmd.env(k, v);
    });

    match &variant.mode {
        RerunMode::NonTty { extra_env } => {
            extra_env.iter().for_each(|(k, v)| {
                cmd.env(k, v);
            });
            run_cmd(cmd)
        }
        RerunMode::Tty { columns } => {
            if spec.stdout_piped {
                run_cmd_tty_stdout_piped(cmd, *columns)
            } else {
                run_cmd_tty(cmd, *columns)
            }
        }
    }
}

fn truncate_report_for_panic(report: &str) -> String {
    const MAX_CHARS: usize = 20_000;
    if report.chars().count() <= MAX_CHARS {
        return report.to_string();
    }
    let mut out = String::new();
    for ch in report.chars().take(MAX_CHARS) {
        out.push(ch);
    }
    out.push_str("\n\n… (report truncated in panic; see REPORT_FILE for full details)");
    out
}

pub struct ParityBinaries {
    pub ts_cli: PathBuf,
    pub rust_bin: PathBuf,
    pub node_modules: PathBuf,
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

pub fn parity_binaries() -> Option<ParityBinaries> {
    if std::env::var("HEADLAMP_RUN_PARITY").ok().as_deref() != Some("1") {
        return None;
    }

    let ts_cli = env_path("HEADLAMP_PARITY_TS_CLI")?;
    let rust_bin = env_path("HEADLAMP_PARITY_RS_BIN")?;
    let node_modules = env_path("HEADLAMP_PARITY_NODE_MODULES")?;

    if !(ts_cli.exists() && rust_bin.exists() && node_modules.exists()) {
        return None;
    }

    Some(ParityBinaries {
        ts_cli,
        rust_bin,
        node_modules,
    })
}

#[derive(Debug, Clone)]
pub struct RunnerParityBinaries {
    pub headlamp_bin: PathBuf,
    pub jest_node_modules: PathBuf,
}

fn npm_program() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

fn ensure_js_deps_node_modules(js_deps_dir: &Path) -> Result<PathBuf, String> {
    static INIT: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    INIT.get_or_init(|| {
        let node_modules = js_deps_dir.join("node_modules");
        let jest_bin =
            node_modules
                .join(".bin")
                .join(if cfg!(windows) { "jest.cmd" } else { "jest" });
        if jest_bin.exists() {
            return Ok(node_modules);
        }

        let status = Command::new(npm_program())
            .current_dir(js_deps_dir)
            .args(["ci", "--silent"])
            .env("npm_config_loglevel", "error")
            .status()
            .map_err(|e| format!("failed to run npm ci: {e}"))?;
        if !status.success() {
            return Err(format!(
                "npm ci failed in {} (status={:?})",
                js_deps_dir.display(),
                status.code()
            ));
        }
        if !jest_bin.exists() {
            return Err(format!(
                "jest not found at {} after npm ci",
                jest_bin.display()
            ));
        }
        Ok(node_modules)
    })
    .clone()
}

pub fn runner_parity_binaries() -> RunnerParityBinaries {
    let headlamp_bin = std::env::var("CARGO_BIN_EXE_headlamp")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(|| ensure_headlamp_bin_from_target_dir());

    let js_deps_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("js_deps");
    let jest_node_modules = ensure_js_deps_node_modules(&js_deps_dir)
        .unwrap_or_else(|e| panic!("failed to ensure Jest deps: {e}"));

    RunnerParityBinaries {
        headlamp_bin,
        jest_node_modules,
    }
}

fn ensure_headlamp_bin_from_target_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .unwrap_or_else(|| panic!("expected {} to have a parent dir", manifest_dir.display()))
        .to_path_buf();

    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));

    let exe_name = if cfg!(windows) {
        "headlamp.exe"
    } else {
        "headlamp"
    };
    let bin_path = target_dir.join("debug").join(exe_name);
    if bin_path.exists() {
        return bin_path;
    }

    let status = Command::new("cargo")
        .current_dir(&workspace_root)
        .args(["build", "-q", "-p", "headlamp"])
        .status()
        .unwrap_or_else(|e| panic!("failed to run cargo build: {e}"));
    if !status.success() {
        panic!(
            "failed to build headlamp binary (status={:?})",
            status.code()
        );
    }
    if !bin_path.exists() {
        panic!("headlamp binary missing at {}", bin_path.display());
    }
    bin_path
}

pub fn mk_temp_dir(name: &str) -> PathBuf {
    let base = std::env::temp_dir()
        .join("headlamp-parity-fixtures")
        .join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

pub fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

pub fn write_jest_config(repo: &Path, test_match: &str) {
    write_file(
        &repo.join("jest.config.js"),
        &format!("module.exports = {{ testMatch: ['{test_match}'] }};\n"),
    );
}

fn program_display_name(program: &Path) -> String {
    program
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn headlamp_runner_stack(runner: &str) -> String {
    match runner {
        "cargo-test" => "cargo-test->cargo".to_string(),
        "cargo-nextest" => "cargo-nextest->nextest".to_string(),
        other => format!("{other}->{other}"),
    }
}

fn build_env_map(repo: &Path, side_label: &ParitySideLabel) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    env.insert("CI".to_string(), "1".to_string());
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        let suffix = side_label.file_safe_label();
        env.insert(
            "HEADLAMP_CACHE_DIR".to_string(),
            repo.join(format!(".headlamp-cache-{suffix}"))
                .to_string_lossy()
                .to_string(),
        );
    }
    env
}

pub fn symlink_dir(src: &Path, dst: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = std::fs::remove_file(dst);
        let _ = std::fs::remove_dir_all(dst);
        symlink(src, dst).unwrap();
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_dir;
        let _ = std::fs::remove_dir_all(dst);
        symlink_dir(src, dst).unwrap();
    }
}

pub fn mk_repo(name: &str, node_modules: &Path) -> PathBuf {
    let repo = mk_temp_dir(name);
    symlink_dir(node_modules, &repo.join("node_modules"));
    repo
}

pub fn assert_parity(repo: &Path, binaries: &ParityBinaries) {
    assert_parity_with_args(repo, binaries, &[], &[]);
}

pub fn assert_parity_with_args(
    repo: &Path,
    binaries: &ParityBinaries,
    ts_args: &[&str],
    rs_args: &[&str],
) {
    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        ts_args,
        rs_args,
        "jest",
    );

    let n_ts = normalize(out_ts, repo);
    let n_rs = normalize(out_rs, repo);
    assert_parity_normalized_outputs(repo, "fixture", code_ts, &n_ts, code_rs, &n_rs);
}

pub fn assert_parity_tty_ui_with_args(
    repo: &Path,
    binaries: &ParityBinaries,
    case: &str,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
) {
    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        columns,
        ts_args,
        rs_args,
        "jest",
    );

    let n_ts = normalize_tty_ui(out_ts, repo);
    let n_rs = normalize_tty_ui(out_rs, repo);
    assert_parity_normalized_outputs(repo, case, code_ts, &n_ts, code_rs, &n_rs);
}

pub fn extract_coverage_ui_block(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .rposition(|ln| ln.trim_start().starts_with('┌') && ln.contains('┬'))
        .or_else(|| {
            lines
                .iter()
                .rposition(|ln| ln.contains('┌') && ln.contains('┬'))
        })
        .unwrap_or(0);

    let end = lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, ln)| {
            ln.starts_with(
                "================================================================================",
            )
            .then_some(index)
        })
        .unwrap_or(lines.len().saturating_sub(1));

    lines.get(start..=end).unwrap_or(&lines[..]).join("\n")
}

pub fn extract_istanbul_text_table_block(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let header_idx = lines
        .iter()
        .rposition(|line| {
            headlamp_core::format::stacks::strip_ansi_simple(line).contains("Uncovered Line #s")
        })
        .unwrap_or(0);

    let start = (0..=header_idx)
        .rev()
        .find(|&index| is_istanbul_dash_line(lines[index]))
        .unwrap_or(header_idx);

    let end = (header_idx..lines.len())
        .filter(|&index| is_istanbul_dash_line(lines[index]))
        .last()
        .unwrap_or(lines.len().saturating_sub(1));

    lines.get(start..=end).unwrap_or(&lines[..]).join("\n")
}

fn is_istanbul_dash_line(line: &str) -> bool {
    let stripped = headlamp_core::format::stacks::strip_ansi_simple(line);
    stripped.contains("|---------|") && stripped.chars().all(|c| c == '-' || c == '|')
}

fn run_cmd(mut cmd: Command) -> (i32, String) {
    let out = cmd.output().unwrap();
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}");
    let combined = combined.replace("\u{1b}[2K\rRUN ", "");
    let combined = combined.replace("\u{1b}[2K\r", "");
    (code, combined)
}

fn run_cmd_tty(mut cmd: Command, columns: usize) -> (i32, String) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("FORCE_COLOR", "1");
    cmd.env("CI", "1");
    cmd.env_remove("NO_COLOR");

    let mut script = Command::new("script");
    let capture_path = std::env::temp_dir().join(format!(
        "headlamp-tty-capture-{}-{}.txt",
        std::process::id(),
        next_capture_id()
    ));
    let _ = std::fs::remove_file(&capture_path);
    script.arg("-q").arg(&capture_path);
    script
        .arg("sh")
        .arg("-lc")
        .arg(build_tty_shell_command(&cmd, columns));
    script.current_dir(
        cmd.get_current_dir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    );
    cmd.get_envs().for_each(|(key, value)| match value {
        Some(v) => {
            script.env(key, v);
        }
        None => {
            script.env_remove(key);
        }
    });

    let out = script.output().unwrap();
    let code = out.status.code().unwrap_or(1);
    let bytes = std::fs::read(&capture_path).unwrap_or_default();
    let mut combined = String::from_utf8_lossy(&bytes).to_string();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    let combined = combined
        .replace('\u{0008}', "")
        .replace('\u{0004}', "")
        .replace("^D", "");
    let _ = std::fs::remove_file(&capture_path);
    (code, combined)
}

fn run_cmd_tty_stdout_piped(mut cmd: Command, columns: usize) -> (i32, String) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("CI", "1");
    cmd.env_remove("NO_COLOR");
    cmd.env_remove("FORCE_COLOR");

    let stdout_capture_path = std::env::temp_dir().join(format!(
        "headlamp-tty-stdout-capture-{}-{}.txt",
        std::process::id(),
        next_capture_id()
    ));
    let _ = std::fs::remove_file(&stdout_capture_path);

    let mut script = Command::new("script");
    let tty_capture_path = std::env::temp_dir().join(format!(
        "headlamp-tty-capture-{}-{}.txt",
        std::process::id(),
        next_capture_id()
    ));
    let _ = std::fs::remove_file(&tty_capture_path);
    script.arg("-q").arg(&tty_capture_path);
    script
        .arg("sh")
        .arg("-lc")
        .arg(build_tty_shell_command_stdout_redirect(
            &cmd,
            columns,
            &stdout_capture_path,
        ));
    script.current_dir(
        cmd.get_current_dir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    );
    cmd.get_envs().for_each(|(key, value)| match value {
        Some(v) => {
            script.env(key, v);
        }
        None => {
            script.env_remove(key);
        }
    });

    let out = script.output().unwrap();
    let code = out.status.code().unwrap_or(1);
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(
        &std::fs::read(&stdout_capture_path).unwrap_or_default(),
    ));
    combined.push_str(&String::from_utf8_lossy(
        &std::fs::read(&tty_capture_path).unwrap_or_default(),
    ));
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    let combined = combined
        .replace('\u{0008}', "")
        .replace('\u{0004}', "")
        .replace("^D", "");
    let _ = std::fs::remove_file(&stdout_capture_path);
    let _ = std::fs::remove_file(&tty_capture_path);
    (code, combined)
}

fn build_tty_shell_command(cmd: &Command, columns: usize) -> String {
    let exe = shell_escape(cmd.get_program().to_string_lossy().as_ref());
    let args = cmd
        .get_args()
        .map(|a| shell_escape(a.to_string_lossy().as_ref()))
        .collect::<Vec<_>>()
        .join(" ");
    format!("stty cols {columns} rows 40 2>/dev/null || true; exec {exe} {args}")
}

fn build_tty_shell_command_stdout_redirect(
    cmd: &Command,
    columns: usize,
    stdout_path: &Path,
) -> String {
    let exe = shell_escape(cmd.get_program().to_string_lossy().as_ref());
    let args = cmd
        .get_args()
        .map(|a| shell_escape(a.to_string_lossy().as_ref()))
        .collect::<Vec<_>>()
        .join(" ");
    let stdout_capture = shell_escape(stdout_path.to_string_lossy().as_ref());
    format!("stty cols {columns} rows 40 2>/dev/null || true; exec {exe} {args} > {stdout_capture}")
}

fn shell_escape(text: &str) -> String {
    let safe = text.replace('\'', r"'\''");
    format!("'{safe}'")
}

fn ensure_rust_bin(rust_bin: &Path) {
    if rust_bin.exists() {
        return;
    }
}

pub fn run_parity_fixture_with_args(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
) -> (ParityRunGroup, i32, String, i32, String) {
    let ts_side_label = ParitySideLabel {
        binary: "node".to_string(),
        runner_stack: format!("ts-cli->{runner}"),
    };
    let rs_side_label = ParitySideLabel {
        binary: program_display_name(rust_bin),
        runner_stack: headlamp_runner_stack(runner),
    };
    let ts_env = build_env_map(repo, &ts_side_label);
    let rs_env = build_env_map(repo, &rs_side_label);

    let mut cmd_ts = Command::new("node");
    cmd_ts
        .current_dir(repo)
        .arg(ts_cli)
        .arg("--sequential")
        .arg(format!("--runner={runner}"));
    ts_env.iter().for_each(|(k, v)| {
        cmd_ts.env(k, v);
    });
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let ts_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        side_label: ts_side_label,
        args: [
            ts_cli.to_string_lossy().to_string(),
            "--sequential".to_string(),
            format!("--runner={runner}"),
        ]
        .into_iter()
        .chain(ts_args.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>(),
        env: ts_env,
        tty_columns: None,
        stdout_piped: false,
    };
    let (code_ts, out_ts) = run_cmd(cmd_ts);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg(format!("--runner={runner}"))
        .arg("--sequential");
    rs_env.iter().for_each(|(k, v)| {
        cmd_rs.env(k, v);
    });
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let rs_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        side_label: rs_side_label,
        args: [format!("--runner={runner}"), "--sequential".to_string()]
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env: rs_env,
        tty_columns: None,
        stdout_piped: false,
    };
    let (code_rs, out_rs) = run_cmd(cmd_rs);
    (
        ParityRunGroup {
            sides: vec![ts_spec, rs_spec],
        },
        code_ts,
        out_ts,
        code_rs,
        out_rs,
    )
}

pub fn run_parity_fixture_with_args_tty(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
) -> (ParityRunGroup, i32, String, i32, String) {
    let ts_side_label = ParitySideLabel {
        binary: "node".to_string(),
        runner_stack: format!("ts-cli->{runner}"),
    };
    let rs_side_label = ParitySideLabel {
        binary: program_display_name(rust_bin),
        runner_stack: headlamp_runner_stack(runner),
    };
    let ts_env = build_env_map(repo, &ts_side_label);
    let rs_env = build_env_map(repo, &rs_side_label);

    let mut cmd_ts = Command::new("node");
    cmd_ts
        .current_dir(repo)
        .arg(ts_cli)
        .arg("--sequential")
        .arg(format!("--runner={runner}"));
    ts_env.iter().for_each(|(k, v)| {
        cmd_ts.env(k, v);
    });
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let ts_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        side_label: ts_side_label,
        args: [
            ts_cli.to_string_lossy().to_string(),
            "--sequential".to_string(),
            format!("--runner={runner}"),
        ]
        .into_iter()
        .chain(ts_args.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>(),
        env: ts_env,
        tty_columns: Some(columns),
        stdout_piped: false,
    };
    let (code_ts, out_ts) = run_cmd_tty(cmd_ts, columns);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg(format!("--runner={runner}"))
        .arg("--sequential");
    rs_env.iter().for_each(|(k, v)| {
        cmd_rs.env(k, v);
    });
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let rs_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        side_label: rs_side_label,
        args: [format!("--runner={runner}"), "--sequential".to_string()]
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env: rs_env,
        tty_columns: Some(columns),
        stdout_piped: false,
    };
    let (code_rs, out_rs) = run_cmd_tty(cmd_rs, columns);
    (
        ParityRunGroup {
            sides: vec![ts_spec, rs_spec],
        },
        code_ts,
        out_ts,
        code_rs,
        out_rs,
    )
}

pub fn run_parity_headlamp_vs_headlamp_with_args_tty(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    baseline_runner: &str,
    candidate_runner: &str,
    baseline_args: &[&str],
    candidate_args: &[&str],
) -> (ParityRunGroup, i32, String, i32, String) {
    let baseline_spec =
        mk_headlamp_tty_run_spec(repo, headlamp_bin, columns, baseline_runner, baseline_args);
    let candidate_spec = mk_headlamp_tty_run_spec(
        repo,
        headlamp_bin,
        columns,
        candidate_runner,
        candidate_args,
    );

    let (code_baseline, out_baseline) =
        run_cmd_tty(build_command_from_spec(&baseline_spec), columns);
    let (code_candidate, out_candidate) =
        run_cmd_tty(build_command_from_spec(&candidate_spec), columns);

    (
        ParityRunGroup {
            sides: vec![baseline_spec, candidate_spec],
        },
        code_baseline,
        out_baseline,
        code_candidate,
        out_candidate,
    )
}

pub fn run_headlamp_with_args_tty(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    runner: &str,
    args: &[&str],
) -> (ParityRunSpec, i32, String) {
    let spec = mk_headlamp_tty_run_spec(repo, headlamp_bin, columns, runner, args);
    let (code, out) = run_cmd_tty(build_command_from_spec(&spec), columns);
    (spec, code, out)
}

fn mk_headlamp_tty_run_spec(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    runner: &str,
    args: &[&str],
) -> ParityRunSpec {
    let base_args = [format!("--runner={runner}"), "--sequential".to_string()];
    let side_label = ParitySideLabel {
        binary: program_display_name(headlamp_bin),
        runner_stack: headlamp_runner_stack(runner),
    };
    ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: headlamp_bin.to_path_buf(),
        side_label: side_label.clone(),
        args: base_args
            .into_iter()
            .chain(args.iter().map(|s| s.to_string()))
            .collect(),
        env: build_env_map(repo, &side_label),
        tty_columns: Some(columns),
        stdout_piped: false,
    }
}

fn build_command_from_spec(spec: &ParityRunSpec) -> Command {
    let mut cmd = Command::new(&spec.program);
    cmd.current_dir(&spec.cwd);
    spec.args.iter().for_each(|arg| {
        cmd.arg(arg);
    });
    spec.env.iter().for_each(|(k, v)| {
        cmd.env(k, v);
    });
    cmd
}

pub fn run_parity_fixture_with_args_tty_stdout_piped(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
) -> (ParityRunGroup, i32, String, i32, String) {
    let ts_side_label = ParitySideLabel {
        binary: "node".to_string(),
        runner_stack: format!("ts-cli->{runner}"),
    };
    let rs_side_label = ParitySideLabel {
        binary: program_display_name(rust_bin),
        runner_stack: headlamp_runner_stack(runner),
    };
    let ts_env = build_env_map(repo, &ts_side_label);
    let rs_env = build_env_map(repo, &rs_side_label);

    let mut cmd_ts = Command::new("node");
    cmd_ts
        .current_dir(repo)
        .arg(ts_cli)
        .arg("--sequential")
        .arg(format!("--runner={runner}"));
    ts_env.iter().for_each(|(k, v)| {
        cmd_ts.env(k, v);
    });
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let ts_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        side_label: ts_side_label,
        args: [
            ts_cli.to_string_lossy().to_string(),
            "--sequential".to_string(),
            format!("--runner={runner}"),
        ]
        .into_iter()
        .chain(ts_args.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>(),
        env: ts_env,
        tty_columns: Some(columns),
        stdout_piped: true,
    };
    let (code_ts, out_ts) = run_cmd_tty_stdout_piped(cmd_ts, columns);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg(format!("--runner={runner}"))
        .arg("--sequential");
    rs_env.iter().for_each(|(k, v)| {
        cmd_rs.env(k, v);
    });
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let rs_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        side_label: rs_side_label,
        args: [format!("--runner={runner}"), "--sequential".to_string()]
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env: rs_env,
        tty_columns: Some(columns),
        stdout_piped: true,
    };
    let (code_rs, out_rs) = run_cmd_tty_stdout_piped(cmd_rs, columns);
    (
        ParityRunGroup {
            sides: vec![ts_spec, rs_spec],
        },
        code_ts,
        out_ts,
        code_rs,
        out_rs,
    )
}

pub fn run_rust_fixture_with_args_tty_stdout_piped(
    repo: &Path,
    rust_bin: &Path,
    columns: usize,
    rs_args: &[&str],
) -> (i32, String) {
    let side_label = ParitySideLabel {
        binary: program_display_name(rust_bin),
        runner_stack: headlamp_runner_stack("jest"),
    };
    let env = build_env_map(repo, &side_label);
    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    env.iter().for_each(|(k, v)| {
        cmd_rs.env(k, v);
    });
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    run_cmd_tty_stdout_piped(cmd_rs, columns)
}

pub fn git_init(repo: &Path) {
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["init", "-q"])
        .status();
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["config", "user.email", "parity@example.com"])
        .status();
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["config", "user.name", "Parity"])
        .status();
}

pub fn git_commit_all(repo: &Path, message: &str) {
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["add", "-A"])
        .status();
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["commit", "-q", "-m", message])
        .status();
}
