use std::path::{Path, PathBuf};

pub fn ensure_repo_local_jest_bin(repo: &Path) {
    // Jest runner requires repo-local node_modules/.bin/jest.
    let js_deps_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("headlamp_tests")
        .join("tests")
        .join("js_deps")
        .join("node_modules")
        .join(".bin");
    let jest_src = js_deps_bin.join(if cfg!(windows) { "jest.cmd" } else { "jest" });
    let jest_dst = repo
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "jest.cmd" } else { "jest" });
    if !jest_src.exists() {
        return;
    }
    let Some(jest_dst_parent) = jest_dst.parent() else {
        panic_jest_like_setup_failure(
            repo,
            "jest",
            format!(
                "failed to compute jest bin parent for {}",
                jest_dst.display()
            ),
        );
    };
    if let Err(error) = std::fs::create_dir_all(jest_dst_parent) {
        panic_jest_like_setup_failure(
            repo,
            "jest",
            format!("failed to create {} ({})", jest_dst_parent.display(), error),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let already_correct = std::fs::read_link(&jest_dst)
            .ok()
            .is_some_and(|target| target == jest_src);
        if !already_correct {
            let _ = std::fs::remove_file(&jest_dst);
            let _ = std::fs::remove_dir_all(&jest_dst);
            if let Err(error) = symlink(&jest_src, &jest_dst)
                && error.kind() != std::io::ErrorKind::AlreadyExists
            {
                panic_jest_like_setup_failure(
                    repo,
                    "jest",
                    format!(
                        "failed to symlink {} -> {} ({})",
                        jest_dst.display(),
                        jest_src.display(),
                        error
                    ),
                );
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = std::fs::copy(&jest_src, &jest_dst);
    }
}

fn panic_jest_like_setup_failure(repo: &Path, runner: &str, message: String) -> ! {
    let ctx = headlamp::format::ctx::make_ctx(repo, Some(120), true, true, None);
    let suite_path = format!("headlamp_parity_support/setup/{runner}");
    let model = headlamp::format::infra_failure::build_infra_failure_test_run_model(
        suite_path.as_str(),
        "Test suite failed to run",
        &message,
    );
    let rendered = headlamp::format::vitest::render_vitest_from_test_model(&model, &ctx, true);
    panic!("{rendered}");
}
