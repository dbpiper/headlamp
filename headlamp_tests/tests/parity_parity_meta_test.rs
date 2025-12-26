mod parity_support;

use std::path::Path;

use parity_support::normalize::normalize_tty_ui_with_meta;
use parity_support::{assert_parity_tty_ui_with_diagnostics, mk_repo, parity_binaries};

#[test]
fn parity_meta_flags_normalized_empty_but_raw_nonempty() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("parity-meta-empty-normalized", &binaries.node_modules);
    let raw = "Starting Jest\nDiscovering → x\n".to_string();
    let (normalized, meta) = normalize_tty_ui_with_meta(raw.clone(), Path::new(&repo));
    assert!(!raw.is_empty());
    assert!(!normalized.is_empty());
    assert!(meta.used_fallback);
    assert!(normalized.contains("Starting Jest"));
}

#[test]
fn parity_meta_artifacts_written_on_mismatch() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("parity-meta-artifacts", &binaries.node_modules);

    let raw_ts = "PASS  tests/x.test.js\n".to_string();
    let raw_rs = "PASS  tests/y.test.js\n".to_string();

    let result = std::panic::catch_unwind(|| {
        assert_parity_tty_ui_with_diagnostics(&repo, "meta_artifacts", 0, raw_ts, 0, raw_rs, None);
    });
    assert!(result.is_err());

    let dump_dir = std::env::temp_dir()
        .join("headlamp-parity-dumps")
        .join(repo.file_name().unwrap_or_default());

    let safe = "meta_artifacts";
    assert!(dump_dir.join(format!("{safe}-raw-ts.txt")).exists());
    assert!(dump_dir.join(format!("{safe}-raw-rs.txt")).exists());
    assert!(dump_dir.join(format!("{safe}-meta.json")).exists());
    assert!(dump_dir.join(format!("{safe}-report.txt")).exists());
}
