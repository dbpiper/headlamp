use headlamp::format::ctx::make_ctx;
use headlamp::format::infra_failure::build_infra_failure_test_run_model;
use headlamp::format::vitest::render_vitest_from_test_model;

#[test]
fn render_infra_failure_snapshot() {
    let repo = std::path::PathBuf::from("/repo");
    let ctx = make_ctx(&repo, Some(80), true, false, Some("vscode".to_string()));
    let model = build_infra_failure_test_run_model(
        "/repo/headlamp/infra",
        "Test suite failed to run",
        "missing runner: jest (expected /repo/node_modules/.bin/jest)\n    at run_jest (/repo/src/jest.rs:10:3)\n    at main (/repo/src/main.rs:1:1)\n",
    );
    let out = render_vitest_from_test_model(&model, &ctx, false);
    insta::assert_snapshot!("render_infra_failure_snapshot", out);
}
