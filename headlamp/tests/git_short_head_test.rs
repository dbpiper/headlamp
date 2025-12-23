use std::path::Path;

use git2::{IndexAddOption, Repository, Signature};

fn commit_one_file(repo_root: &Path) -> git2::Oid {
    let repo = Repository::init(repo_root).unwrap();
    std::fs::write(repo_root.join("a.txt"), "hello").unwrap();

    let mut index = repo.index().unwrap();
    index
        .add_all(["a.txt"], IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();

    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = Signature::now("headlamp-test", "headlamp-test@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap()
}

#[test]
fn git_short_head_returns_8_char_prefix_of_head_commit() {
    let dir = tempfile::tempdir().unwrap();
    let commit_oid = commit_one_file(dir.path());

    let expected = commit_oid.to_string().chars().take(8).collect::<String>();
    let actual = headlamp::fast_related::git_short_head(dir.path()).unwrap();

    assert_eq!(actual, expected);
}
