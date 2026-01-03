use std::path::{Path, PathBuf};

pub fn ensure_repo_local_jest_bin(repo: &Path) {
    // Jest runner requires repo-local node_modules/.bin/jest, but we never check in node_modules.
    // Mirror CI: keep deps elsewhere, and drop a tiny repo-local shim that delegates to that jest.
    ensure_node_modules_dir(repo);
    let jest_name = if cfg!(windows) { "jest.cmd" } else { "jest" };
    let Some(jest_src) = find_shared_jest_bin(jest_name) else {
        return;
    };
    let jest_dst = repo.join("node_modules").join(".bin").join(jest_name);
    ensure_parent_dir(repo, "jest", &jest_dst);
    write_jest_shim(repo, &jest_src, &jest_dst);
}

fn ensure_node_modules_dir(repo: &Path) {
    let node_modules = repo.join("node_modules");
    if let Ok(metadata) = std::fs::symlink_metadata(&node_modules)
        && !metadata.is_dir()
    {
        let _ = std::fs::remove_file(&node_modules);
        let _ = std::fs::remove_dir_all(&node_modules);
    }
    let _ = std::fs::create_dir_all(&node_modules);
}

fn find_shared_jest_bin(jest_name: &str) -> Option<PathBuf> {
    // CI speed: allow using a preinstalled deps bundle inside the CI image.
    let js_deps_bin_prebuilt = PathBuf::from("/opt/headlamp/js_deps/node_modules/.bin");
    let js_deps_bin_repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("headlamp_tests")
        .join("tests")
        .join("js_deps")
        .join("node_modules")
        .join(".bin");
    [js_deps_bin_prebuilt, js_deps_bin_repo]
        .into_iter()
        .map(|dir| dir.join(jest_name))
        .find(|candidate| candidate.exists())
}

fn ensure_parent_dir(repo: &Path, runner: &str, dst: &Path) {
    let Some(dst_parent) = dst.parent() else {
        panic_jest_like_setup_failure(
            repo,
            runner,
            format!(
                "failed to compute {runner} bin parent for {}",
                dst.display()
            ),
        );
    };
    if let Err(error) = std::fs::create_dir_all(dst_parent) {
        panic_jest_like_setup_failure(
            repo,
            runner,
            format!("failed to create {} ({})", dst_parent.display(), error),
        );
    }
}

fn write_jest_shim(repo: &Path, jest_src: &Path, jest_dst: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // If the fixture repo was created in another environment (e.g. CI), it may contain a
        // repo-local `jest` shim that is a symlink to a now-nonexistent path. Remove it so we can
        // write a fresh shim pointing at this machine's shared jest.
        let _ = std::fs::remove_file(jest_dst);
        let script = format!(
            "#!/usr/bin/env bash\nexec \"{}\" \"$@\"\n",
            jest_src.to_string_lossy()
        );
        if let Err(error) = std::fs::write(jest_dst, script.as_bytes()) {
            panic_jest_like_setup_failure(
                repo,
                "jest",
                format!("failed to write {} ({})", jest_dst.display(), error),
            );
        }
        let _ = std::fs::set_permissions(jest_dst, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(windows)]
    {
        let _ = std::fs::copy(jest_src, jest_dst);
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
