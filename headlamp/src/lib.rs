extern crate self as headlamp_core;

pub mod cargo;
pub mod cargo_select;
pub mod fast_related;
pub mod git;
pub mod jest;
pub mod jest_config;
#[cfg(test)]
mod jest_coverage_test;
pub mod jest_discovery;
pub mod jest_ownership;
#[cfg(test)]
mod jest_threshold_test;
pub mod live_progress;
#[cfg(test)]
mod live_progress_test;
pub mod parallel_stride;
pub mod process;
pub mod pytest;
pub mod pytest_select;
pub(crate) mod pythonpath;
pub mod run;
mod seed_match;
pub mod session;
pub mod streaming;
pub mod watch;

pub mod args;
pub mod config;
mod config_ts;
pub mod coverage;
pub mod diagnostics_trace;
pub mod error;
pub mod format;
pub mod help;
pub(crate) mod profile;
pub mod project;
pub(crate) mod rust_parse;
pub mod selection;
pub mod test_model;

#[cfg(test)]
mod args_test;
#[cfg(test)]
mod cargo_empty_model_test;
#[cfg(test)]
mod cargo_select_test;
#[cfg(test)]
mod git_test;
#[cfg(test)]
mod pytest_artifacts_test;
#[cfg(test)]
mod pytest_coverage_test;
#[cfg(test)]
mod pytest_location_test;
#[cfg(test)]
mod pytest_timing_test;
#[cfg(test)]
mod pythonpath_test;

pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
