use std::path::{Path, PathBuf};

use headlamp::test_model::{TestCaseResult, TestRunAggregated, TestRunModel, TestSuiteResult};

use super::{
    ParityRunGroup, ParityRunSpec, assert_parity_with_diagnostics, parity_meta,
    run_headlamp_with_args_tty, write_file,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerId {
    Jest,
    CargoTest,
    CargoNextest,
    Pytest,
}

impl RunnerId {
    pub fn as_runner_flag_value(self) -> &'static str {
        match self {
            RunnerId::Jest => "jest",
            RunnerId::CargoTest => "cargo-test",
            RunnerId::CargoNextest => "cargo-nextest",
            RunnerId::Pytest => "pytest",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunnerParityScenario {
    pub test_file_stem: String,
    pub passing_test_name: String,
    pub failing_test_name: String,
    pub should_fail: bool,
}

impl RunnerParityScenario {
    pub fn basic_pass() -> Self {
        Self {
            test_file_stem: "sum_test".to_string(),
            passing_test_name: "sum_passes".to_string(),
            failing_test_name: "sum_fails".to_string(),
            should_fail: false,
        }
    }

    pub fn basic_fail() -> Self {
        Self {
            should_fail: true,
            ..Self::basic_pass()
        }
    }
}

pub fn write_stubbed_runner_repo(repo: &Path, scenario: &RunnerParityScenario) {
    write_minimal_repo_files(repo, scenario);
    write_stubbed_jest(repo, scenario);
    write_stubbed_cargo(repo, scenario);
    write_stubbed_pytest(repo, scenario);
}

pub fn assert_runner_parity_tty_all_four(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
) {
    let columns = 120;
    let mut run_specs: Vec<ParityRunSpec> = vec![];
    let mut sides: Vec<parity_meta::ParityCompareSideInput> = vec![];
    runners.iter().for_each(|(runner, args)| {
        let (spec, exit, raw) = run_headlamp_with_args_tty(
            repo,
            headlamp_bin,
            columns,
            runner.as_runner_flag_value(),
            args,
        );
        let raw_bytes = raw.len();
        let raw_lines = raw.lines().count();
        let (normalized, normalization_meta) =
            super::normalize::normalize_tty_ui_runner_parity_with_meta(raw.clone(), repo);
        let normalized_bytes = normalized.len();
        let normalized_lines = normalized.lines().count();
        let side_label = spec.side_label.clone();
        run_specs.push(spec);
        sides.push(parity_meta::ParityCompareSideInput {
            label: side_label,
            exit,
            raw,
            normalized,
            meta: parity_meta::ParitySideMeta {
                raw_bytes,
                raw_lines,
                normalized_bytes,
                normalized_lines,
                normalization: normalization_meta,
            },
        });
    });

    let compare = parity_meta::ParityCompareInput { sides };
    let run_group = ParityRunGroup { sides: run_specs };
    assert_parity_with_diagnostics(repo, case, &compare, Some(&run_group));
}

pub fn runner_parity_headlamp_bin() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_headlamp")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(|| headlamp_parity_support::binaries::runner_parity_binaries().headlamp_bin)
}

fn write_minimal_repo_files(repo: &Path, scenario: &RunnerParityScenario) {
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    write_file(
        &repo.join(format!("tests/{}.js", scenario.test_file_stem)),
        "test('sum_passes', () => { expect(1).toBe(1); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*_test.js'] };\n",
    );

    write_file(
        &repo.join("Cargo.toml"),
        "\
[package]\n\
name = \"parity_stub\"\n\
version = \"0.1.0\"\n\
edition = \"2024\"\n\
\n\
[lib]\n\
path = \"src/lib.rs\"\n\
",
    );
    write_file(&repo.join("src/lib.rs"), "pub fn noop() {}\n");
    write_file(
        &repo.join(format!("tests/{}.rs", scenario.test_file_stem)),
        "\
#[test]\n\
fn sum_passes() {\n\
    assert_eq!(1, 1);\n\
}\n\
",
    );

    write_file(&repo.join("pyproject.toml"), "[tool.pytest.ini_options]\n");
    write_file(&repo.join("pkg/__init__.py"), "");
    write_file(
        &repo.join(format!("tests/{}.py", scenario.test_file_stem)),
        "def sum_passes() -> None:\n    assert 1 == 1\n",
    );
}

fn write_stubbed_jest(repo: &Path, scenario: &RunnerParityScenario) {
    let bin = repo.join("node_modules").join(".bin");
    let jest_path = bin.join(if cfg!(windows) { "jest.cmd" } else { "jest" });
    let scenario_path = repo.join(".headlamp-stub-scenario.json");
    let model = build_model(repo, scenario);
    let scenario_json = serde_json::json!({
        "model": model,
        "list_tests": [repo.join(format!("tests/{}.js", scenario.test_file_stem)).to_string_lossy().to_string()],
    });
    write_file(
        &scenario_path,
        &serde_json::to_string_pretty(&scenario_json).unwrap(),
    );

    let script = if cfg!(windows) {
        format!("@echo off\r\nnode \"%~dp0\\..\\..\\stub_jest.js\" %*\r\n")
    } else {
        "#!/usr/bin/env node\nrequire('../../stub_jest.js');\n".to_string()
    };
    write_file(&jest_path, &script);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&jest_path, std::fs::Permissions::from_mode(0o755));
    }

    let impl_path = repo.join("stub_jest.js");
    let impl_contents = r#"
const fs = require('node:fs');
const path = require('node:path');

function readScenario() {
  const p = path.join(process.cwd(), '.headlamp-stub-scenario.json');
  return JSON.parse(fs.readFileSync(p, 'utf8'));
}

function main() {
  const args = process.argv.slice(2);
  const scenario = readScenario();
  if (args.includes('--listTests')) {
    (scenario.list_tests || []).forEach((p) => process.stdout.write(`${p}\n`));
    process.exit(0);
  }
  const outPath = process.env.JEST_BRIDGE_OUT;
  if (outPath) {
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, JSON.stringify(scenario.model));
  }
  const shouldFail = Boolean(scenario.model && scenario.model.aggregated && !scenario.model.aggregated.success);
  process.exit(shouldFail ? 1 : 0);
}

main();
"#;
    write_file(&impl_path, impl_contents);
}

fn write_stubbed_cargo(repo: &Path, scenario: &RunnerParityScenario) {
    let bin = repo.join("bin");
    let cargo_path = bin.join(if cfg!(windows) { "cargo.cmd" } else { "cargo" });
    let scenario_path = repo.join(".headlamp-stub-scenario.json");
    if !scenario_path.exists() {
        let model = build_model(repo, scenario);
        let scenario_json = serde_json::json!({ "model": model, "list_tests": [] });
        write_file(
            &scenario_path,
            &serde_json::to_string_pretty(&scenario_json).unwrap(),
        );
    }

    let script = super::stub_scripts::cargo_stub_script();
    write_file(&cargo_path, &script);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&cargo_path, std::fs::Permissions::from_mode(0o755));
    }
}

fn write_stubbed_pytest(repo: &Path, scenario: &RunnerParityScenario) {
    let bin = repo.join("bin");
    let pytest_path = bin.join(if cfg!(windows) {
        "pytest.exe"
    } else {
        "pytest"
    });
    let scenario_path = repo.join(".headlamp-stub-scenario.json");
    if !scenario_path.exists() {
        let model = build_model(repo, scenario);
        let scenario_json = serde_json::json!({ "model": model, "list_tests": [] });
        write_file(
            &scenario_path,
            &serde_json::to_string_pretty(&scenario_json).unwrap(),
        );
    }

    let script = if cfg!(windows) {
        "pytest stub not supported on windows\n".to_string()
    } else {
        r#"#!/usr/bin/env python3
import json
import sys

def main() -> int:
    with open(".headlamp-stub-scenario.json", "r", encoding="utf8") as f:
        scenario = json.load(f)
    success = bool(scenario.get("model", {}).get("aggregated", {}).get("success", True))
    def emit(nodeid: str, outcome: str) -> None:
        payload = {
            "type": "case",
            "nodeid": nodeid,
            "outcome": outcome,
            "duration": 0.0,
            "stdout": "",
            "stderr": "",
            "longrepr": None,
        }
        sys.stdout.write("HEADLAMP_PYTEST_EVENT " + json.dumps(payload) + "\n")
    emit("tests/sum_test.py::sum_passes", "passed")
    if not success:
        emit("tests/sum_test.py::sum_fails", "failed")
        return 1
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
"#
        .to_string()
    };
    write_file(&pytest_path, &script);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&pytest_path, std::fs::Permissions::from_mode(0o755));
    }
}

fn build_model(repo: &Path, scenario: &RunnerParityScenario) -> TestRunModel {
    let test_file_path = scenario_test_file_path(repo, scenario);
    let pass_case = passing_case(&scenario.passing_test_name);
    let test_results = suite_test_results(pass_case, scenario);
    let suite = suite_result(test_file_path, test_results, scenario.should_fail);
    TestRunModel {
        start_time: 0,
        test_results: vec![suite],
        aggregated: aggregated_for_should_fail(scenario.should_fail),
    }
}

fn scenario_test_file_path(repo: &Path, scenario: &RunnerParityScenario) -> String {
    repo.join("tests")
        .join(format!("{}.js", scenario.test_file_stem))
        .to_string_lossy()
        .to_string()
}

fn passing_case(passing_test_name: &str) -> TestCaseResult {
    let full_name = passing_test_name.to_string();
    TestCaseResult {
        title: full_name.clone(),
        full_name,
        status: "passed".to_string(),
        timed_out: None,
        duration: 0,
        location: None,
        failure_messages: vec![],
        failure_details: None,
    }
}

fn failing_case(scenario: &RunnerParityScenario) -> TestCaseResult {
    let full_name = scenario.failing_test_name.to_string();
    let (status, failure_messages) = if scenario.should_fail {
        ("failed".to_string(), vec!["fail".to_string()])
    } else {
        ("passed".to_string(), vec![])
    };
    TestCaseResult {
        title: full_name.clone(),
        full_name,
        status,
        timed_out: None,
        duration: 0,
        location: None,
        failure_messages,
        failure_details: None,
    }
}

fn suite_test_results(pass_case: TestCaseResult, scenario: &RunnerParityScenario) -> Vec<TestCaseResult> {
    scenario
        .should_fail
        .then(|| vec![pass_case, failing_case(scenario)])
        .unwrap_or_else(|| vec![pass_case])
}

fn suite_result(
    test_file_path: String,
    test_results: Vec<TestCaseResult>,
    should_fail: bool,
) -> TestSuiteResult {
    TestSuiteResult {
        test_file_path,
        status: if should_fail {
            "failed".to_string()
        } else {
            "passed".to_string()
        },
        timed_out: None,
        failure_message: String::new(),
        failure_details: None,
        test_exec_error: None,
        console: None,
        test_results,
    }
}

fn aggregated_for_should_fail(should_fail: bool) -> TestRunAggregated {
    let total_suites = 1u64;
    let failed_suites = if should_fail { 1 } else { 0 };
    let passed_suites = total_suites.saturating_sub(failed_suites);
    let total_tests: u64 = if should_fail { 2 } else { 1 };
    let failed_tests: u64 = if should_fail { 1 } else { 0 };
    let passed_tests = total_tests.saturating_sub(failed_tests);
    TestRunAggregated {
        num_total_test_suites: total_suites,
        num_passed_test_suites: passed_suites,
        num_failed_test_suites: failed_suites,
        num_total_tests: total_tests,
        num_passed_tests: passed_tests,
        num_failed_tests: failed_tests,
        num_pending_tests: 0,
        num_todo_tests: 0,
        num_timed_out_tests: None,
        num_timed_out_test_suites: None,
        start_time: 0,
        success: !should_fail,
        run_time_ms: Some(0),
    }
}
