use headlamp::args::CoverageDetail;
use headlamp::args::derive_args;
use headlamp::coverage::print::{PrintOpts, should_render_hotspots};

#[test]
fn print_opts_for_run_matches_parsed_args() {
    let argv = vec![
        "--coverage.maxFiles=7".to_string(),
        "--coverage.maxHotspots=9".to_string(),
        "--coverage.pageFit=false".to_string(),
        "--coverage.editor=my-editor --file {file} --line {line}".to_string(),
    ];
    let parsed = derive_args(&[], &argv, true);

    let opts = PrintOpts::for_run(&parsed, false);
    assert_eq!(opts.max_files, Some(7));
    assert_eq!(opts.max_hotspots, Some(9));
    assert!(!opts.page_fit);
    assert!(!opts.tty);
    assert_eq!(
        opts.editor_cmd.as_deref(),
        Some("my-editor --file {file} --line {line}")
    );
}

#[test]
fn hotspot_predicate_is_consistent() {
    assert!(!should_render_hotspots(None));
    assert!(!should_render_hotspots(Some(CoverageDetail::Auto)));
    assert!(should_render_hotspots(Some(CoverageDetail::All)));
    assert!(should_render_hotspots(Some(CoverageDetail::Lines(3))));
}
