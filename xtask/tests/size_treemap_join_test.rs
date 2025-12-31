use std::time::{Duration, Instant};

use xtask::size_treemap::generate_treemap_json;
use xtask::size_treemap::SizeTreemapInputs;

#[test]
fn treemap_generation_is_fast_for_empty_inputs() {
    let start = Instant::now();
    let result = generate_treemap_json(SizeTreemapInputs {
        map_path: std::path::PathBuf::from("/tmp/does_not_exist"),
        binary_path: std::path::PathBuf::from("/tmp/does_not_exist"),
        focus_headlamp: true,
    });
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_millis(100));
}
