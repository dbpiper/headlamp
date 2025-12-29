use std::process::{Command, Stdio};

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use regex::Regex;
use semver::Version;

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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::RevertRelease(args) => revert_release(args),
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
                    if is_tag_canonical && !is_best_canonical {
                        best = Some((ver, tag));
                    } else if is_tag_canonical == is_best_canonical && tag.len() < best_tag.len() {
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
