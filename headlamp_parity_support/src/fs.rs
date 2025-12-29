use std::path::{Path, PathBuf};

pub fn mk_temp_dir(name: &str) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("parity-fixtures")
        .join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

pub fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let existing = std::fs::read_to_string(path).ok();
    let already_same = existing.as_deref() == Some(contents);
    if already_same {
        return;
    }
    std::fs::write(path, contents).unwrap();
}

pub fn write_jest_config(repo: &Path, test_match: &str) {
    write_file(
        &repo.join("jest.config.js"),
        &format!("module.exports = {{ testMatch: ['{test_match}'] }};\n"),
    );
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
