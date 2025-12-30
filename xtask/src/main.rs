use std::process::{Command, Stdio};

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use regex::Regex;
use semver::Version;
use std::path::{Path, PathBuf};

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
}

#[derive(Parser, Debug)]
struct RevertReleaseArgs {
    /// Remote name to delete the tag from.
    #[arg(long, default_value = "origin")]
    remote: String,

    /// Override the tag to delete (e.g. v0.1.36).
    #[arg(long)]
    tag: Option<String>,

    /// Do not delete the remote tag.
    #[arg(long)]
    no_remote: bool,

    /// Print the git commands that would run, but do not execute them.
    #[arg(long, default_value_t = true)]
    dry_run: bool,

    /// Actually perform the deletion.
    ///
    /// This sets dry_run=false.
    #[arg(long)]
    yes: bool,
}

#[derive(Parser, Debug)]
struct ReleaseArgs {
    /// Release version (e.g. v0.1.37 or 0.1.37)
    version: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::RevertRelease(args) => revert_release(args),
        Commands::Release(args) => release(args),
    }
}

fn revert_release(mut args: RevertReleaseArgs) -> anyhow::Result<()> {
    if args.yes {
        args.dry_run = false;
    }

    let tag = match args.tag.clone() {
        Some(t) => t,
        None => resolve_latest_stable_reachable_tag()?
            .with_context(|| "no stable semver tag reachable from HEAD")?,
    };

    let local_cmd = format!("git tag -d {tag}");
    let remote_cmd = format!("git push --delete {} {tag}", args.remote);

    if args.dry_run {
        println!("{local_cmd}");
        if !args.no_remote {
            println!("{remote_cmd}");
        }
        return Ok(());
    }

    // Safety: require --yes for non-dry-run (even if someone passes --dry-run=false explicitly).
    if !args.yes {
        bail!(
            "refusing to run destructive action without --yes. Re-run with --yes or use --dry-run."
        );
    }

    run_git(["tag", "-d", &tag])?;
    if !args.no_remote {
        run_git(["push", "--delete", &args.remote, &tag])?;
    }

    Ok(())
}

fn release(args: ReleaseArgs) -> anyhow::Result<()> {
    let repo_root = repo_root()?;
    let new_version = parse_version(&args.version)?;

    let headlamp_cargo_toml = repo_root.join("headlamp").join("Cargo.toml");
    let old_version = read_cargo_package_version(&headlamp_cargo_toml).with_context(|| {
        format!(
            "failed to read current headlamp version from {}",
            headlamp_cargo_toml.display()
        )
    })?;

    // 1) Rust crate version
    write_cargo_package_version(&headlamp_cargo_toml, &new_version)?;

    // 2) npm wrapper version
    let npm_package_json = repo_root.join("npm").join("headlamp").join("package.json");
    write_npm_package_version(&npm_package_json, &old_version, &new_version)?;

    // 3) PyPI wrapper version
    let pypi_pyproject = repo_root
        .join("python")
        .join("headlamp_pypi")
        .join("pyproject.toml");
    write_pyproject_version(&pypi_pyproject, &old_version, &new_version)?;

    // 4) Copy top-level README.md into all wrapper package READMEs.
    let top_readme = repo_root.join("README.md");
    let readme_contents = std::fs::read_to_string(&top_readme)
        .with_context(|| format!("failed to read {}", top_readme.display()))?;
    overwrite_file(
        &repo_root.join("npm").join("headlamp").join("README.md"),
        &readme_contents,
    )?;
    overwrite_file(
        &repo_root
            .join("python")
            .join("headlamp_pypi")
            .join("README.md"),
        &readme_contents,
    )?;

    Ok(())
}

fn resolve_latest_stable_reachable_tag() -> anyhow::Result<Option<String>> {
    // `--merged HEAD` filters to reachable tags.
    // `--sort=-v:refname` gives version-sort order (best-effort).
    let out = run_git_capture(["tag", "--merged", "HEAD", "--sort=-v:refname"])?;
    let tags = out
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    // Common GitHub-ish tag styles:
    // - v0.1.36
    // - 0.1.36
    // - headlamp-v0.1.36 / release-v0.1.36 / refs/tags/v0.1.36
    // We parse the first semver-like substring and then ignore prerelease tags.
    let version_re =
        Regex::new(r"(?P<prefix>^|[^0-9A-Za-z])v?(?P<ver>\d+\.\d+\.\d+(?:[-+][0-9A-Za-z\.\-]+)?)")
            .expect("regex");

    let mut best: Option<(Version, String)> = None;
    for tag in tags {
        let Some(cap) = version_re.captures(&tag) else {
            continue;
        };
        let ver_text = cap.name("ver").unwrap().as_str();
        let Ok(ver) = Version::parse(ver_text) else {
            continue;
        };
        if !ver.pre.is_empty() {
            // Ignore prerelease tags like v1.2.3-rc.1
            continue;
        }

        match &best {
            None => best = Some((ver, tag)),
            Some((best_ver, best_tag)) => {
                if ver > *best_ver {
                    best = Some((ver, tag));
                } else if ver == *best_ver {
                    // Prefer canonical tags if multiple represent the same version.
                    let canonical_v = format!("v{}", best_ver);
                    let is_tag_canonical = tag == canonical_v || tag == best_ver.to_string();
                    let is_best_canonical =
                        best_tag == &canonical_v || best_tag == &best_ver.to_string();
                    let should_replace = (is_tag_canonical && !is_best_canonical)
                        || (is_tag_canonical == is_best_canonical && tag.len() < best_tag.len());
                    if should_replace {
                        best = Some((ver, tag));
                    }
                }
            }
        }
    }

    Ok(best.map(|(_, tag)| tag))
}

fn run_git(args: impl IntoIterator<Item = impl AsRef<str>>) -> anyhow::Result<()> {
    let mut cmd = Command::new("git");
    for a in args {
        cmd.arg(a.as_ref());
    }
    let status = cmd.status().context("failed to spawn git")?;
    if !status.success() {
        bail!("git command failed (exit={:?})", status.code());
    }
    Ok(())
}

fn run_git_capture(args: impl IntoIterator<Item = impl AsRef<str>>) -> anyhow::Result<String> {
    let mut cmd = Command::new("git");
    for a in args {
        cmd.arg(a.as_ref());
    }
    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to spawn git")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git command failed (exit={:?}): {stderr}",
            output.status.code()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn repo_root() -> anyhow::Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .context("xtask: failed to locate repo root (no parent of CARGO_MANIFEST_DIR)")?;
    Ok(repo_root.to_path_buf())
}

fn parse_version(input: &str) -> anyhow::Result<Version> {
    let s = input.trim().trim_start_matches('v');
    let v = Version::parse(s).with_context(|| format!("invalid semver version: {input}"))?;
    Ok(v)
}

fn read_cargo_package_version(path: &Path) -> anyhow::Result<Version> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let re =
        Regex::new(r#"(?m)^\s*\[package\]\s*$[\s\S]*?^\s*version\s*=\s*"([^"]+)""#).expect("regex");
    let caps = re
        .captures(&text)
        .with_context(|| format!("no [package].version found in {}", path.display()))?;
    let ver_text = caps.get(1).unwrap().as_str();
    Version::parse(ver_text)
        .with_context(|| format!("failed to parse [package].version in {}", path.display()))
}

fn write_cargo_package_version(path: &Path, new_version: &Version) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let re = Regex::new(r#"(?m)^(\s*version\s*=\s*")([^"]+)(")\s*$"#).expect("regex");
    // Replace only within the first [package] stanza by slicing.
    let pkg_re = Regex::new(r#"(?m)^\s*\[package\]\s*$"#).expect("regex");
    let Some(pkg_start) = pkg_re.find(&text) else {
        bail!("no [package] section found in {}", path.display());
    };
    let (head, tail) = text.split_at(pkg_start.start());
    let replaced_tail = {
        // Find the end of the [package] section: next [section] header or EOF.
        let section_re = Regex::new(r#"(?m)^\s*\[[^\]]+\]\s*$"#).expect("regex");
        let mut it = section_re.find_iter(tail);
        let _first = it.next(); // this is [package]
        let pkg_end = it.next().map(|m| m.start()).unwrap_or(tail.len());
        let (pkg_block, rest) = tail.split_at(pkg_end);
        let replaced_pkg = re
            .replacen(pkg_block, 1, |caps: &regex::Captures<'_>| {
                format!("{}{}{}", &caps[1], new_version, &caps[3])
            })
            .to_string();
        format!("{replaced_pkg}{rest}")
    };
    let new_text = format!("{head}{replaced_tail}");
    std::fs::write(path, new_text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_npm_package_version(
    path: &Path,
    old_version: &Version,
    new_version: &Version,
) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let re = Regex::new(r#""version"\s*:\s*"([^"]+)""#).expect("regex");
    let Some(caps) = re.captures(&text) else {
        bail!("npm: no version field found in {}", path.display());
    };
    let current = caps.get(1).unwrap().as_str();
    if current != old_version.to_string() {
        bail!(
            "npm: version mismatch in {} (expected {}, found {})",
            path.display(),
            old_version,
            current
        );
    }
    let new_text = re
        .replacen(&text, 1, format!(r#""version": "{new_version}""#))
        .to_string();
    std::fs::write(path, new_text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_pyproject_version(
    path: &Path,
    old_version: &Version,
    new_version: &Version,
) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let re = Regex::new(r#"(?m)^\s*version\s*=\s*"([^"]+)""#).expect("regex");
    let Some(caps) = re.captures(&text) else {
        bail!("pypi: no version field found in {}", path.display());
    };
    let current = caps.get(1).unwrap().as_str();
    if current != old_version.to_string() {
        bail!(
            "pypi: version mismatch in {} (expected {}, found {})",
            path.display(),
            old_version,
            current
        );
    }
    let new_text = re
        .replacen(&text, 1, format!(r#"version = "{new_version}""#))
        .to_string();
    std::fs::write(path, new_text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn overwrite_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("mkdir -p {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
