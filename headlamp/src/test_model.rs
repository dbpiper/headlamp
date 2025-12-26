use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRunModel {
    pub start_time: u64,
    pub test_results: Vec<TestSuiteResult>,
    pub aggregated: TestRunAggregated,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSuiteResult {
    pub test_file_path: String,
    pub status: String,
    pub timed_out: Option<bool>,
    pub failure_message: String,
    pub failure_details: Option<Vec<serde_json::Value>>,
    pub test_exec_error: Option<serde_json::Value>,
    pub console: Option<Vec<TestConsoleEntry>>,
    pub test_results: Vec<TestCaseResult>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestConsoleEntry {
    pub message: Option<serde_json::Value>,
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCaseResult {
    pub title: String,
    pub full_name: String,
    pub status: String,
    pub timed_out: Option<bool>,
    pub duration: u64,
    pub location: Option<TestLocation>,
    pub failure_messages: Vec<String>,
    pub failure_details: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestLocation {
    pub line: i64,
    pub column: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRunAggregated {
    pub num_total_test_suites: u64,
    pub num_passed_test_suites: u64,
    pub num_failed_test_suites: u64,
    pub num_total_tests: u64,
    pub num_passed_tests: u64,
    pub num_failed_tests: u64,
    pub num_pending_tests: u64,
    pub num_todo_tests: u64,
    pub num_timed_out_tests: Option<u64>,
    pub num_timed_out_test_suites: Option<u64>,
    pub start_time: u64,
    pub success: bool,
    pub run_time_ms: Option<u64>,
}
