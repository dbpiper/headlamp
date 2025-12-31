use std::process::Command;

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use xtask::size_report;
use xtask::size_treemap::{generate_treemap_json, write_treemap_json, SizeTreemapInputs};

mod release_cmd;
use release_cmd::{ReleaseArgs, RevertReleaseArgs};

#[derive(Parser, Debug)]
#[command(name = "xtask")]
#[command(about = "Repo maintenance tasks", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Delete the latest reachable stable release tag locally and on the remote.
    ///
    /// By default this is a dry-run; pass --yes to actually execute.
    RevertRelease(RevertReleaseArgs),

    /// Bump the release version across all packages and sync wrapper READMEs.
    ///
    /// This command ONLY edits files in the working tree. It does NOT create git commits or tags.
    ///
    /// Example:
    ///   cargo run -p xtask -- release v0.1.37
    Release(ReleaseArgs),

    /// Parse an Apple `ld` `-map` file and print the biggest code-size contributors.
    ///
    /// Example:
    ///   cargo run -p xtask -- size-report --map target/size/headlamp.map
    SizeReport(SizeReportArgs),

    /// Generate a treemap JSON for the `headlamp` binary (file/function level via DWARF).
    ///
    /// Example:
    ///   cargo run -p xtask -- size-treemap --map target/size/headlamp.map --dwarf target/size/headlamp.dSYM/Contents/Resources/DWARF/headlamp
    SizeTreemap(SizeTreemapArgs),
}

#[derive(Parser, Debug)]
struct SizeReportArgs {
    /// Path to an Apple `ld` `-map` file (generated via `-Wl,-map,<path>`).
    #[arg(long, default_value = "target/size/headlamp.map")]
    map: PathBuf,

    /// Number of rows to print.
    #[arg(long, default_value_t = 30)]
    top: usize,
}

#[derive(Parser, Debug)]
struct SizeTreemapArgs {
    /// Path to an Apple `ld` `-map` file.
    #[arg(long, default_value = "target/size/headlamp.map")]
    map: PathBuf,

    /// Path to the built headlamp release binary to analyze.
    #[arg(long, default_value = "target/release/headlamp")]
    binary: PathBuf,

    /// Where to write the dSYM output directory.
    #[arg(long, default_value = "target/size/headlamp.dSYM")]
    dsym: PathBuf,

    /// Where to write the treemap JSON.
    #[arg(long, default_value = "target/size/headlamp.treemap.json")]
    out: PathBuf,

    /// If set, include all crates (not just headlamp).
    #[arg(long, default_value_t = false)]
    include_deps: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::RevertRelease(args) => release_cmd::revert_release(args),
        Commands::Release(args) => release_cmd::release(args),
        Commands::SizeReport(args) => size_report_cmd(args),
        Commands::SizeTreemap(args) => size_treemap_cmd(args),
    }
}

fn size_report_cmd(args: SizeReportArgs) -> anyhow::Result<()> {
    let report = size_report::parse_map_file(&args.map)?;
    println!("map: {}", args.map.display());
    println!("top crates by summed symbol sizes:");

    for row in report.crate_sizes.iter().take(args.top) {
        println!("{:>10}  {}", format_bytes(row.bytes), row.crate_name);
    }

    Ok(())
}

fn size_treemap_cmd(args: SizeTreemapArgs) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let repo_root = release_cmd::repo_root()?;
    let map_path = repo_root.join(args.map);
    let binary_path = repo_root.join(args.binary);
    let out_path = repo_root.join(args.out);

    ensure_parent_dir_exists(&map_path)?;
    ensure_parent_dir_exists(&out_path)?;

    eprintln!("[size-treemap] build (map + debuginfo)…");
    build_headlamp_release_with_map(&map_path, &binary_path)?;
    eprintln!("[size-treemap] build done in {:?}", start.elapsed());

    eprintln!("[size-treemap] dsym…");
    ensure_dsym_for_binary(&binary_path)?;
    eprintln!("[size-treemap] dsym done in {:?}", start.elapsed());

    eprintln!("[size-treemap] parse/addr2line/aggregate…");
    let treemap = generate_treemap_json(SizeTreemapInputs {
        map_path,
        binary_path,
        focus_headlamp: !args.include_deps,
    })?;
    eprintln!("[size-treemap] analysis done in {:?}", start.elapsed());

    write_treemap_json(&out_path, &treemap)?;
    println!("wrote {}", out_path.display());
    eprintln!("[size-treemap] total {:?}", start.elapsed());
    Ok(())
}

fn ensure_parent_dir_exists(path: &Path) -> anyhow::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create dir {}", parent.display()))?;
    Ok(())
}

fn build_headlamp_release_with_map(map_path: &Path, binary_path: &Path) -> anyhow::Result<()> {
    if binary_path.exists() {
        std::fs::remove_file(binary_path)
            .with_context(|| format!("failed to remove {}", binary_path.display()))?;
    }

    let status = Command::new("cargo")
        .arg("rustc")
        .arg("-p")
        .arg("headlamp")
        .arg("--release")
        .arg("--bin")
        .arg("headlamp")
        .arg("--config")
        .arg("profile.release.debug=2")
        .arg("--")
        .arg("-C")
        .arg(format!("link-arg=-Wl,-map,{}", map_path.display()))
        .status()
        .context("failed to spawn cargo rustc")?;
    if !status.success() {
        bail!("cargo rustc failed (exit={:?})", status.code());
    }
    if !map_path.exists() {
        bail!(
            "expected linker map at {} but it was not created; build may have skipped linking",
            map_path.display()
        );
    }
    Ok(())
}

fn ensure_dsym_for_binary(binary_path: &Path) -> anyhow::Result<()> {
    let produced_dsym = binary_path.with_extension("dSYM");
    if produced_dsym.exists() {
        return Ok(());
    }

    let status = Command::new("xcrun")
        .arg("dsymutil")
        .arg("-o")
        .arg(&produced_dsym)
        .arg(binary_path)
        .status()
        .context("failed to spawn dsymutil")?;
    if !status.success() {
        bail!("dsymutil failed (exit={:?})", status.code());
    }
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;

    let bytes_f64 = bytes as f64;
    if bytes_f64 >= MIB {
        return format!("{:.2} MiB", bytes_f64 / MIB);
    }
    if bytes_f64 >= KIB {
        return format!("{:.2} KiB", bytes_f64 / KIB);
    }
    format!("{bytes} B")
}
