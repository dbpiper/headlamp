#![allow(dead_code)]

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

pub mod diagnostics;
pub mod diff_report;
pub mod normalize;
pub mod parity_meta;
pub mod token_ast;
pub use normalize::normalize;
pub use normalize::normalize_tty_ui;

static CAPTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn next_capture_id() -> usize {
    CAPTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone)]
pub struct ParityRunSpec {
    pub cwd: PathBuf,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub tty_columns: Option<usize>,
    pub stdout_piped: bool,
}

#[derive(Debug, Clone)]
pub struct ParityRunPair {
    pub ts: ParityRunSpec,
    pub rs: ParityRunSpec,
}

pub fn assert_parity_normalized_outputs(
    repo: &Path,
    case: &str,
    code_ts: i32,
    out_ts: &str,
    code_rs: i32,
    out_rs: &str,
) {
    let compare_input = parity_meta::ParityCompareInput {
        raw_ts: out_ts.to_string(),
        raw_rs: out_rs.to_string(),
        normalized_ts: out_ts.to_string(),
        normalized_rs: out_rs.to_string(),
        meta: parity_meta::ParityCompareMeta {
            ts: parity_meta::ParitySideMeta {
                raw_bytes: out_ts.as_bytes().len(),
                raw_lines: out_ts.lines().count(),
                normalized_bytes: out_ts.as_bytes().len(),
                normalized_lines: out_ts.lines().count(),
                normalization: parity_meta::NormalizationMeta {
                    normalizer: parity_meta::NormalizerKind::NonTty,
                    used_fallback: false,
                    last_failed_tests_line: None,
                    last_test_files_line: None,
                    last_box_table_top_line: None,
                    stages: vec![],
                },
            },
            rs: parity_meta::ParitySideMeta {
                raw_bytes: out_rs.as_bytes().len(),
                raw_lines: out_rs.lines().count(),
                normalized_bytes: out_rs.as_bytes().len(),
                normalized_lines: out_rs.lines().count(),
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
    };

    assert_parity_with_diagnostics(repo, case, code_ts, code_rs, &compare_input, None);
}

pub fn assert_parity_non_tty_with_diagnostics(
    repo: &Path,
    case: &str,
    code_ts: i32,
    raw_ts: String,
    code_rs: i32,
    raw_rs: String,
    run_pair: Option<&ParityRunPair>,
) {
    let (normalized_ts, meta_ts) = normalize::normalize_with_meta(raw_ts.clone(), repo);
    let (normalized_rs, meta_rs) = normalize::normalize_with_meta(raw_rs.clone(), repo);

    let ts_raw_bytes = raw_ts.as_bytes().len();
    let ts_raw_lines = raw_ts.lines().count();
    let ts_norm_bytes = normalized_ts.as_bytes().len();
    let ts_norm_lines = normalized_ts.lines().count();

    let rs_raw_bytes = raw_rs.as_bytes().len();
    let rs_raw_lines = raw_rs.lines().count();
    let rs_norm_bytes = normalized_rs.as_bytes().len();
    let rs_norm_lines = normalized_rs.lines().count();

    let compare = parity_meta::ParityCompareInput {
        raw_ts,
        raw_rs,
        normalized_ts,
        normalized_rs,
        meta: parity_meta::ParityCompareMeta {
            ts: parity_meta::ParitySideMeta {
                raw_bytes: ts_raw_bytes,
                raw_lines: ts_raw_lines,
                normalized_bytes: ts_norm_bytes,
                normalized_lines: ts_norm_lines,
                normalization: meta_ts,
            },
            rs: parity_meta::ParitySideMeta {
                raw_bytes: rs_raw_bytes,
                raw_lines: rs_raw_lines,
                normalized_bytes: rs_norm_bytes,
                normalized_lines: rs_norm_lines,
                normalization: meta_rs,
            },
        },
    };

    assert_parity_with_diagnostics(repo, case, code_ts, code_rs, &compare, run_pair);
}

pub fn assert_parity_tty_ui_with_diagnostics(
    repo: &Path,
    case: &str,
    code_ts: i32,
    raw_ts: String,
    code_rs: i32,
    raw_rs: String,
    run_pair: Option<&ParityRunPair>,
) {
    let (normalized_ts, meta_ts) = normalize::normalize_tty_ui_with_meta(raw_ts.clone(), repo);
    let (normalized_rs, meta_rs) = normalize::normalize_tty_ui_with_meta(raw_rs.clone(), repo);

    let ts_raw_bytes = raw_ts.as_bytes().len();
    let ts_raw_lines = raw_ts.lines().count();
    let ts_norm_bytes = normalized_ts.as_bytes().len();
    let ts_norm_lines = normalized_ts.lines().count();

    let rs_raw_bytes = raw_rs.as_bytes().len();
    let rs_raw_lines = raw_rs.lines().count();
    let rs_norm_bytes = normalized_rs.as_bytes().len();
    let rs_norm_lines = normalized_rs.lines().count();

    let compare = parity_meta::ParityCompareInput {
        raw_ts,
        raw_rs,
        normalized_ts,
        normalized_rs,
        meta: parity_meta::ParityCompareMeta {
            ts: parity_meta::ParitySideMeta {
                raw_bytes: ts_raw_bytes,
                raw_lines: ts_raw_lines,
                normalized_bytes: ts_norm_bytes,
                normalized_lines: ts_norm_lines,
                normalization: meta_ts,
            },
            rs: parity_meta::ParitySideMeta {
                raw_bytes: rs_raw_bytes,
                raw_lines: rs_raw_lines,
                normalized_bytes: rs_norm_bytes,
                normalized_lines: rs_norm_lines,
                normalization: meta_rs,
            },
        },
    };

    assert_parity_with_diagnostics(repo, case, code_ts, code_rs, &compare, run_pair);
}

pub fn assert_parity_with_diagnostics(
    repo: &Path,
    case: &str,
    code_ts: i32,
    code_rs: i32,
    compare: &parity_meta::ParityCompareInput,
    run_pair: Option<&ParityRunPair>,
) {
    if code_ts == code_rs && compare.normalized_ts == compare.normalized_rs {
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
    let ts_path = dump_dir.join(format!("{safe}-ts.txt"));
    let rs_path = dump_dir.join(format!("{safe}-rs.txt"));
    let raw_ts_path = dump_dir.join(format!("{safe}-raw-ts.txt"));
    let raw_rs_path = dump_dir.join(format!("{safe}-raw-rs.txt"));
    let diff_path = dump_dir.join(format!("{safe}-diff.txt"));
    let report_path = dump_dir.join(format!("{safe}-report.txt"));
    let meta_path = dump_dir.join(format!("{safe}-meta.json"));
    let analysis_path = dump_dir.join(format!("{safe}-analysis.json"));
    let tokens_ts_path = dump_dir.join(format!("{safe}-tokens-ts.json"));
    let tokens_rs_path = dump_dir.join(format!("{safe}-tokens-rs.json"));
    let ast_ts_path = dump_dir.join(format!("{safe}-ast-ts.json"));
    let ast_rs_path = dump_dir.join(format!("{safe}-ast-rs.json"));
    let reruns_dir = dump_dir.join(format!("{safe}-reruns"));

    let _ = std::fs::write(&ts_path, &compare.normalized_ts);
    let _ = std::fs::write(&rs_path, &compare.normalized_rs);
    let _ = std::fs::write(&raw_ts_path, &compare.raw_ts);
    let _ = std::fs::write(&raw_rs_path, &compare.raw_rs);
    let token_ast_summary = write_token_ast_files(
        &tokens_ts_path,
        &tokens_rs_path,
        &ast_ts_path,
        &ast_rs_path,
        compare,
    );
    let reruns = run_pair
        .and_then(|pair| run_diagnostic_reruns(&reruns_dir, pair))
        .unwrap_or_default();
    let meta_file = ParityMetaFile {
        compare: &compare.meta,
        token_ast: token_ast_summary,
        reruns,
    };
    if let Ok(mut file) = std::fs::File::create(&meta_path) {
        let _ = serde_json::to_writer_pretty(&mut file, &meta_file);
    }
    let artifacts = diagnostics::ArtifactPaths {
        normalized_ts: ts_path.to_string_lossy().to_string(),
        normalized_rs: rs_path.to_string_lossy().to_string(),
        raw_ts: raw_ts_path.to_string_lossy().to_string(),
        raw_rs: raw_rs_path.to_string_lossy().to_string(),
        diff: diff_path.to_string_lossy().to_string(),
        report: report_path.to_string_lossy().to_string(),
        meta: meta_path.to_string_lossy().to_string(),
        analysis: analysis_path.to_string_lossy().to_string(),
        tokens_ts: tokens_ts_path.to_string_lossy().to_string(),
        tokens_rs: tokens_rs_path.to_string_lossy().to_string(),
        ast_ts: ast_ts_path.to_string_lossy().to_string(),
        ast_rs: ast_rs_path.to_string_lossy().to_string(),
        reruns_dir: reruns_dir.to_string_lossy().to_string(),
    };
    let bundle = diagnostics::build_bundle(
        repo,
        case,
        code_ts,
        code_rs,
        artifacts,
        compare,
        &meta_file.reruns,
    );
    if let Ok(mut file) = std::fs::File::create(&analysis_path) {
        let _ = serde_json::to_writer_pretty(&mut file, &bundle);
    }

    let diff = similar_asserts::SimpleDiff::from_str(
        &compare.normalized_ts,
        &compare.normalized_rs,
        "ts",
        "rs",
    )
    .to_string();
    let _ = std::fs::write(&diff_path, &diff);

    let report = diff_report::build_parity_report_with_meta(compare);
    let _ = std::fs::write(&report_path, &report);
    let report_for_panic = truncate_report_for_panic(&report);

    panic!(
        "parity mismatch ({case}) repo={}: ts_exit={code_ts} rs_exit={code_rs}\n\nREPORT:\n{}\n\nDIFF: {}\nREPORT_FILE: {}\nMETA: {}\nANALYSIS: {}\nTS: {}\nRS: {}\nRAW_TS: {}\nRAW_RS: {}\nTOKENS_TS: {}\nTOKENS_RS: {}\nAST_TS: {}\nAST_RS: {}\nRERUNS_DIR: {}",
        repo.display(),
        report_for_panic,
        diff_path.display(),
        report_path.display(),
        meta_path.display(),
        analysis_path.display(),
        ts_path.display(),
        rs_path.display(),
        raw_ts_path.display(),
        raw_rs_path.display(),
        tokens_ts_path.display(),
        tokens_rs_path.display(),
        ast_ts_path.display(),
        ast_rs_path.display(),
        reruns_dir.display(),
    );
}

#[derive(Debug, Serialize)]
struct ParityMetaFile<'a> {
    compare: &'a parity_meta::ParityCompareMeta,
    token_ast: TokenAstSummary,
    reruns: Vec<RerunMeta>,
}

#[derive(Debug, Serialize)]
struct TokenAstSummary {
    ts_raw: token_ast::TokenStats,
    rs_raw: token_ast::TokenStats,
    ts_norm: token_ast::TokenStats,
    rs_norm: token_ast::TokenStats,
    ts_block_order: Vec<String>,
    rs_block_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RerunMeta {
    variant: String,
    ts_code: i32,
    rs_code: i32,
    ts_path: String,
    rs_path: String,
    ts_bytes: usize,
    rs_bytes: usize,
    ts_tokens: token_ast::TokenStats,
    rs_tokens: token_ast::TokenStats,
    ts_blocks: usize,
    rs_blocks: usize,
}

fn write_token_ast_files(
    tokens_ts_path: &Path,
    tokens_rs_path: &Path,
    ast_ts_path: &Path,
    ast_rs_path: &Path,
    compare: &parity_meta::ParityCompareInput,
) -> TokenAstSummary {
    let ts_raw = token_ast::build_token_stream(&compare.raw_ts);
    let rs_raw = token_ast::build_token_stream(&compare.raw_rs);
    let ts_norm = token_ast::build_token_stream(&compare.normalized_ts);
    let rs_norm = token_ast::build_token_stream(&compare.normalized_rs);
    let ast_ts = token_ast::build_document_ast(&compare.normalized_ts);
    let ast_rs = token_ast::build_document_ast(&compare.normalized_rs);

    let _ = std::fs::File::create(tokens_ts_path)
        .ok()
        .and_then(|mut f| serde_json::to_writer_pretty(&mut f, &ts_raw).ok());
    let _ = std::fs::File::create(tokens_rs_path)
        .ok()
        .and_then(|mut f| serde_json::to_writer_pretty(&mut f, &rs_raw).ok());
    let _ = std::fs::File::create(ast_ts_path)
        .ok()
        .and_then(|mut f| serde_json::to_writer_pretty(&mut f, &ast_ts).ok());
    let _ = std::fs::File::create(ast_rs_path)
        .ok()
        .and_then(|mut f| serde_json::to_writer_pretty(&mut f, &ast_rs).ok());

    TokenAstSummary {
        ts_raw: ts_raw.stats,
        rs_raw: rs_raw.stats,
        ts_norm: ts_norm.stats,
        rs_norm: rs_norm.stats,
        ts_block_order: ast_ts.blocks.into_iter().map(|b| b.hash).collect(),
        rs_block_order: ast_rs.blocks.into_iter().map(|b| b.hash).collect(),
    }
}

fn run_diagnostic_reruns(dir: &Path, run_pair: &ParityRunPair) -> Option<Vec<RerunMeta>> {
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
            .filter_map(|variant| run_one_rerun(dir, run_pair, &variant))
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
    run_pair: &ParityRunPair,
    variant: &RerunVariant,
) -> Option<RerunMeta> {
    let ts_path = dir.join(format!("ts-{}.txt", variant.id));
    let rs_path = dir.join(format!("rs-{}.txt", variant.id));

    let (ts_code, ts_out) = run_variant_side(&run_pair.ts, variant);
    let (rs_code, rs_out) = run_variant_side(&run_pair.rs, variant);
    let _ = std::fs::write(&ts_path, &ts_out);
    let _ = std::fs::write(&rs_path, &rs_out);

    let ts_tokens = token_ast::build_token_stream(&ts_out).stats;
    let rs_tokens = token_ast::build_token_stream(&rs_out).stats;
    let ts_blocks = token_ast::build_document_ast(&ts_out).blocks.len();
    let rs_blocks = token_ast::build_document_ast(&rs_out).blocks.len();

    Some(RerunMeta {
        variant: variant.id.to_string(),
        ts_code,
        rs_code,
        ts_path: ts_path.to_string_lossy().to_string(),
        rs_path: rs_path.to_string_lossy().to_string(),
        ts_bytes: ts_out.as_bytes().len(),
        rs_bytes: rs_out.as_bytes().len(),
        ts_tokens,
        rs_tokens,
        ts_blocks,
        rs_blocks,
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

fn env_path_or_default(var: &str, default: &str) -> PathBuf {
    std::env::var(var)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

pub fn parity_binaries() -> Option<ParityBinaries> {
    if std::env::var("HEADLAMP_RUN_PARITY").ok().as_deref() != Some("1") {
        return None;
    }

    let ts_cli = env_path_or_default(
        "HEADLAMP_PARITY_TS_CLI",
        "/Users/david/src/headlamp-original/dist/cli.cjs",
    );
    let rust_bin = env_path_or_default(
        "HEADLAMP_PARITY_RS_BIN",
        "/Users/david/src/headlamp/target/debug/headlamp",
    );
    let node_modules = env_path_or_default(
        "HEADLAMP_PARITY_NODE_MODULES",
        "/Users/david/src/headlamp-original/node_modules",
    );

    ensure_rust_bin(&rust_bin);
    if !(ts_cli.exists() && rust_bin.exists() && node_modules.exists()) {
        return None;
    }

    Some(ParityBinaries {
        ts_cli,
        rust_bin,
        node_modules,
    })
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

fn build_env_map(repo: &Path, is_ts: bool) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    env.insert("CI".to_string(), "1".to_string());
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        let suffix = if is_ts { "ts" } else { "rs" };
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
    let (_spec, code_ts, out_ts, code_rs, out_rs) =
        run_parity_fixture_with_args(repo, &binaries.ts_cli, &binaries.rust_bin, ts_args, rs_args);

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
    let _ = Command::new("cargo")
        .current_dir("/Users/david/src/headlamp")
        .args(["build", "-q", "-p", "headlamp"])
        .status()
        .unwrap();
}

pub fn run_parity_fixture_with_args(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    ts_args: &[&str],
    rs_args: &[&str],
) -> (ParityRunPair, i32, String, i32, String) {
    let mut cmd_ts = Command::new("node");
    cmd_ts.current_dir(repo).arg(ts_cli).arg("--sequential");
    cmd_ts.env("CI", "1");
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        cmd_ts.env(
            "HEADLAMP_CACHE_DIR",
            repo.join(".headlamp-cache-ts")
                .to_string_lossy()
                .to_string(),
        );
    }
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let ts_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        args: [
            ts_cli.to_string_lossy().to_string(),
            "--sequential".to_string(),
        ]
        .into_iter()
        .chain(ts_args.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>(),
        env: build_env_map(repo, true),
        tty_columns: None,
        stdout_piped: false,
    };
    let (code_ts, out_ts) = run_cmd(cmd_ts);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    cmd_rs.env("CI", "1");
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        cmd_rs.env(
            "HEADLAMP_CACHE_DIR",
            repo.join(".headlamp-cache-rs")
                .to_string_lossy()
                .to_string(),
        );
    }
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let rs_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        args: ["--runner=jest".to_string(), "--sequential".to_string()]
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env: build_env_map(repo, false),
        tty_columns: None,
        stdout_piped: false,
    };
    let (code_rs, out_rs) = run_cmd(cmd_rs);
    (
        ParityRunPair {
            ts: ts_spec,
            rs: rs_spec,
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
) -> (ParityRunPair, i32, String, i32, String) {
    let mut cmd_ts = Command::new("node");
    cmd_ts.current_dir(repo).arg(ts_cli).arg("--sequential");
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        cmd_ts.env(
            "HEADLAMP_CACHE_DIR",
            repo.join(".headlamp-cache-ts")
                .to_string_lossy()
                .to_string(),
        );
    }
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let ts_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        args: [
            ts_cli.to_string_lossy().to_string(),
            "--sequential".to_string(),
        ]
        .into_iter()
        .chain(ts_args.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>(),
        env: build_env_map(repo, true),
        tty_columns: Some(columns),
        stdout_piped: false,
    };
    let (code_ts, out_ts) = run_cmd_tty(cmd_ts, columns);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        cmd_rs.env(
            "HEADLAMP_CACHE_DIR",
            repo.join(".headlamp-cache-rs")
                .to_string_lossy()
                .to_string(),
        );
    }
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let rs_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        args: ["--runner=jest".to_string(), "--sequential".to_string()]
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env: build_env_map(repo, false),
        tty_columns: Some(columns),
        stdout_piped: false,
    };
    let (code_rs, out_rs) = run_cmd_tty(cmd_rs, columns);
    (
        ParityRunPair {
            ts: ts_spec,
            rs: rs_spec,
        },
        code_ts,
        out_ts,
        code_rs,
        out_rs,
    )
}

pub fn run_parity_fixture_with_args_tty_stdout_piped(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
) -> (ParityRunPair, i32, String, i32, String) {
    let mut cmd_ts = Command::new("node");
    cmd_ts.current_dir(repo).arg(ts_cli).arg("--sequential");
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        cmd_ts.env(
            "HEADLAMP_CACHE_DIR",
            repo.join(".headlamp-cache-ts")
                .to_string_lossy()
                .to_string(),
        );
    }
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let ts_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        args: [
            ts_cli.to_string_lossy().to_string(),
            "--sequential".to_string(),
        ]
        .into_iter()
        .chain(ts_args.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>(),
        env: build_env_map(repo, true),
        tty_columns: Some(columns),
        stdout_piped: true,
    };
    let (code_ts, out_ts) = run_cmd_tty_stdout_piped(cmd_ts, columns);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        cmd_rs.env(
            "HEADLAMP_CACHE_DIR",
            repo.join(".headlamp-cache-rs")
                .to_string_lossy()
                .to_string(),
        );
    }
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let rs_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        args: ["--runner=jest".to_string(), "--sequential".to_string()]
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env: build_env_map(repo, false),
        tty_columns: Some(columns),
        stdout_piped: true,
    };
    let (code_rs, out_rs) = run_cmd_tty_stdout_piped(cmd_rs, columns);
    (
        ParityRunPair {
            ts: ts_spec,
            rs: rs_spec,
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
    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        cmd_rs.env(
            "HEADLAMP_CACHE_DIR",
            repo.join(".headlamp-cache-rs")
                .to_string_lossy()
                .to_string(),
        );
    }
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
