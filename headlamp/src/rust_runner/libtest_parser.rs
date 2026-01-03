use std::path::Path;

use crate::format::cargo_test::CargoTestStreamParser;
use crate::test_model::TestRunModel;

pub(crate) fn parse_libtest_output_for_suite(
    repo_root: &Path,
    suite_source_path: &str,
    combined_output: &str,
) -> Option<TestRunModel> {
    let mut parser = CargoTestStreamParser::new(repo_root);
    let suite_line = format!("Running {suite_source_path}");
    let _ = parser.push_line(&suite_line);
    combined_output.lines().for_each(|line| {
        let _ = parser.push_line(line);
    });
    parser.finalize()
}
