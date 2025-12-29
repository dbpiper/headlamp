use std::path::Path;
use std::path::PathBuf;

use git2::{Repository, Signature};

fn mk_workspace_temp_dir(name: &str) -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let dir = workspace_root.join("target").join("test-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn commit_one_commit(repo_root: &Path) -> git2::Oid {
    let repo = Repository::init_bare(repo_root).unwrap();
    let mut index = repo.index().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = Signature::now("headlamp-test", "headlamp-test@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap()
}

#[test]
fn git_short_head_returns_8_char_prefix_of_head_commit() {
    let dir = mk_workspace_temp_dir("git-short-head");
    let commit_oid = commit_one_commit(&dir);

    let expected = commit_oid.to_string().chars().take(8).collect::<String>();
    let actual = headlamp::fast_related::git_short_head(&dir).unwrap();

    assert_eq!(actual, expected);
}
