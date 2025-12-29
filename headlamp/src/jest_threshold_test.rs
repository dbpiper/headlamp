use indexmap::IndexSet;

use crate::jest::should_print_coverage_threshold_failure_summary;

#[test]
fn should_print_coverage_threshold_failure_summary_only_when_threshold_lines_exist() {
    let empty: IndexSet<String> = IndexSet::new();
    assert!(!should_print_coverage_threshold_failure_summary(1, &empty));
    assert!(!should_print_coverage_threshold_failure_summary(0, &empty));

    let mut non_empty: IndexSet<String> = IndexSet::new();
    non_empty.insert("Lines: 99.00% < 100% (short 1.00%)".to_string());
    assert!(should_print_coverage_threshold_failure_summary(
        1, &non_empty
    ));
    assert!(!should_print_coverage_threshold_failure_summary(
        0, &non_empty
    ));
}
