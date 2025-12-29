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
        .unwrap_or_else(super::ensure_headlamp_bin_from_target_dir)
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

    let script = if cfg!(windows) {
        "@echo off\r\necho cargo stub not supported on windows\r\nexit /b 1\r\n".to_string()
    } else {
        r#"#!/usr/bin/env bash
set -euo pipefail

scenario_path=".headlamp-stub-scenario.json"
if [[ ! -f "$scenario_path" ]]; then
  echo "missing $scenario_path" >&2
  exit 2
fi

sub="${1:-}"
shift || true

if [[ "$sub" == "test" ]]; then
  # Minimal output that CargoTestStreamParser can parse.
  echo "Running tests/sum_test.rs (target/debug/deps/sum_test-0000000000000000)"
  echo "test sum_passes ... ok"
  if grep -q '"success":[[:space:]]*false' "$scenario_path"; then
    echo "test sum_fails ... FAILED"
    echo
    echo "failures:"
    echo
    echo "---- sum_fails stdout ----"
    echo "fail"
    echo
    echo "failures:"
    echo "    sum_fails"
    echo
    echo "test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out"
    exit 101
  else
    echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
    exit 0
  fi
fi

if [[ "$sub" == "nextest" ]]; then
  if [[ "${1:-}" == "--version" ]]; then
    echo "cargo-nextest 0.0.0"
    exit 0
  fi
  # Minimal libtest-json-plus stream.
  echo '{"type":"suite","event":"started","test_count":2,"nextest":{"crate":"parity_stub","test_binary":"sum_test","kind":"test"}}'
  echo '{"type":"test","event":"started","name":"parity_stub::sum_test$sum_passes"}'
  echo '{"type":"test","event":"ok","name":"parity_stub::sum_test$sum_passes","exec_time":0.0}'
  if grep -q '"success":[[:space:]]*false' "$scenario_path"; then
    echo '{"type":"test","event":"failed","name":"parity_stub::sum_test$sum_fails","exec_time":0.0,"stdout":"fail"}'
    echo '{"type":"suite","event":"failed","passed":1,"failed":1,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.0,"nextest":{"crate":"parity_stub","test_binary":"sum_test","kind":"test"}}'
    exit 101
  else
    echo '{"type":"suite","event":"ok","passed":1,"failed":0,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.0,"nextest":{"crate":"parity_stub","test_binary":"sum_test","kind":"test"}}'
    exit 0
  fi
fi

if [[ "$sub" == "llvm-cov" ]]; then
  if [[ "${1:-}" == "--version" ]]; then
    echo "cargo-llvm-cov 0.0.0"
    exit 0
  fi
  exit 0
fi

echo "unsupported cargo stub subcommand: $sub" >&2
exit 2
"#
        .to_string()
    };
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
    let test_file_path = repo
        .join("tests")
        .join(format!("{}.js", scenario.test_file_stem))
        .to_string_lossy()
        .to_string();
    let pass_case = TestCaseResult {
        title: scenario.passing_test_name.clone(),
        full_name: scenario.passing_test_name.clone(),
        status: "passed".to_string(),
        timed_out: None,
        duration: 0,
        location: None,
        failure_messages: vec![],
        failure_details: None,
    };
    let fail_case = TestCaseResult {
        title: scenario.failing_test_name.clone(),
        full_name: scenario.failing_test_name.clone(),
        status: if scenario.should_fail {
            "failed".to_string()
        } else {
            "passed".to_string()
        },
        timed_out: None,
        duration: 0,
        location: None,
        failure_messages: if scenario.should_fail {
            vec!["fail".to_string()]
        } else {
            vec![]
        },
        failure_details: None,
    };
    let test_results = if scenario.should_fail {
        vec![pass_case, fail_case]
    } else {
        vec![pass_case]
    };
    let suite = TestSuiteResult {
        test_file_path,
        status: if scenario.should_fail {
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
    };
    let total_suites = 1u64;
    let failed_suites = if scenario.should_fail { 1 } else { 0 };
    let passed_suites = total_suites.saturating_sub(failed_suites);
    let total_tests: u64 = if scenario.should_fail { 2 } else { 1 };
    let failed_tests: u64 = if scenario.should_fail { 1 } else { 0 };
    let passed_tests = total_tests.saturating_sub(failed_tests);
    TestRunModel {
        start_time: 0,
        test_results: vec![suite],
        aggregated: TestRunAggregated {
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
            success: !scenario.should_fail,
            run_time_ms: Some(0),
        },
    }
}
