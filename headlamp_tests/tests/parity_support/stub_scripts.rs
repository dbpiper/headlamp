pub const CARGO_STUB_WINDOWS: &str =
    "@echo off\r\necho cargo stub not supported on windows\r\nexit /b 1\r\n";

pub const CARGO_STUB_BASH: &str = r#"#!/usr/bin/env bash
set -euo pipefail

scenario_path=".headlamp-stub-scenario.json"
if [[ ! -f "$scenario_path" ]]; then
  echo "missing $scenario_path" >&2
  exit 2
fi

sub="${1:-}"
shift || true

if [[ "$sub" == "test" ]]; then
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
"#;

pub fn cargo_stub_script() -> String {
    if cfg!(windows) {
        CARGO_STUB_WINDOWS.to_string()
    } else {
        CARGO_STUB_BASH.to_string()
    }
}


