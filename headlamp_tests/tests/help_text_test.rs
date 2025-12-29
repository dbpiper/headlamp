#[test]
fn help_text_is_up_to_date() {
    let help = headlamp::help::help_text();
    assert!(help.contains("--runner=<jest|pytest|cargo-nextest|cargo-test>"), "{help}");
    assert!(!help.contains("vitest"), "{help}");
    assert!(help.contains("lastRelease"), "{help}");
    assert!(help.contains("--dependency-language"), "{help}");
}


