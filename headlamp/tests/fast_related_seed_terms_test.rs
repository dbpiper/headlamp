use std::collections::BTreeSet;
use std::path::Path;

use headlamp::fast_related::build_seed_terms_ts_like;

fn as_set(items: Vec<String>) -> BTreeSet<String> {
    items.into_iter().collect::<BTreeSet<_>>()
}

#[test]
fn fast_related_seed_terms_strip_cjs() {
    let repo_root = Path::new("/repo");
    let seeds = vec!["/repo/src/foo.cjs".to_string()];
    let terms = as_set(build_seed_terms_ts_like(repo_root, &seeds));
    assert!(terms.contains("src/foo"));
    assert!(terms.contains("foo"));
}

#[test]
fn fast_related_seed_terms_strip_mjs_like_ts() {
    let repo_root = Path::new("/repo");
    let seeds = vec!["/repo/src/foo.mjs".to_string()];
    let terms = as_set(build_seed_terms_ts_like(repo_root, &seeds));
    assert!(terms.contains("src/foo"));
    assert!(terms.contains("foo"));
}
