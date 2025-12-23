use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use headlamp::fast_related::{cached_related, default_cache_root};

fn set_env_var(key: &str, value: String) {
    unsafe {
        std::env::set_var(key, value);
    }
}

#[test]
fn cached_related_uses_env_cache_root_and_hits_cache() {
    let cache_root = std::env::temp_dir().join("headlamp-tests-cache-root");
    let _ = std::fs::remove_dir_all(&cache_root);
    std::fs::create_dir_all(&cache_root).unwrap();
    set_env_var(
        "HEADLAMP_CACHE_DIR",
        cache_root.to_string_lossy().to_string(),
    );

    let repo_root = std::env::temp_dir().join("headlamp-tests-repo");
    let _ = std::fs::remove_dir_all(&repo_root);
    std::fs::create_dir_all(&repo_root).unwrap();

    let existing_test = repo_root.join("tests").join("a.test.js");
    std::fs::create_dir_all(existing_test.parent().unwrap()).unwrap();
    std::fs::write(&existing_test, "test('a', () => { expect(1).toBe(1); });\n").unwrap();

    let call_count = Arc::new(AtomicUsize::new(0));
    let selection_key = "src/a.js";
    let first = cached_related(&repo_root, selection_key, {
        let call_count = Arc::clone(&call_count);
        let path_text = existing_test.to_string_lossy().to_string();
        move || {
            call_count.fetch_add(1, Ordering::SeqCst);
            Ok(vec![path_text.clone()])
        }
    })
    .unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
    assert_eq!(first.len(), 1);

    let second = cached_related(&repo_root, selection_key, {
        let call_count = Arc::clone(&call_count);
        move || {
            call_count.fetch_add(1, Ordering::SeqCst);
            Ok(vec![])
        }
    })
    .unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
    assert_eq!(second, first);

    let effective_root = default_cache_root();
    assert!(effective_root.exists());
}

#[test]
fn cached_related_recomputes_when_cached_paths_are_missing() {
    let cache_root = std::env::temp_dir().join("headlamp-tests-cache-missing");
    let _ = std::fs::remove_dir_all(&cache_root);
    std::fs::create_dir_all(&cache_root).unwrap();
    set_env_var(
        "HEADLAMP_CACHE_DIR",
        cache_root.to_string_lossy().to_string(),
    );

    let repo_root = std::env::temp_dir().join("headlamp-tests-repo-missing");
    let _ = std::fs::remove_dir_all(&repo_root);
    std::fs::create_dir_all(&repo_root).unwrap();

    let missing_test = repo_root.join("tests").join("missing.test.js");
    let missing_path_text = missing_test.to_string_lossy().to_string();

    let call_count = Arc::new(AtomicUsize::new(0));
    let selection_key = "src/missing.js";

    let _ = cached_related(&repo_root, selection_key, {
        let call_count = Arc::clone(&call_count);
        let missing_path_text = missing_path_text.clone();
        move || {
            call_count.fetch_add(1, Ordering::SeqCst);
            Ok(vec![missing_path_text.clone()])
        }
    })
    .unwrap();

    // Remove the cached path to trigger a recompute.
    let _ = std::fs::remove_file(&missing_test);

    let _ = cached_related(&repo_root, selection_key, {
        let call_count = Arc::clone(&call_count);
        move || {
            call_count.fetch_add(1, Ordering::SeqCst);
            Ok(vec![])
        }
    })
    .unwrap();

    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}
