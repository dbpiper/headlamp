extern crate self as headlamp_core;

pub mod cargo;
pub mod cargo_select;
pub mod fast_related;
pub mod git;
pub mod jest;
pub mod jest_config;
pub mod jest_discovery;
pub mod jest_ownership;
pub mod live_progress;
pub mod parallel_stride;
pub mod process;
pub mod pytest;
pub mod pytest_select;
pub mod run;
mod seed_match;
pub mod streaming;
pub mod watch;

pub mod args;
pub mod config;
mod config_ts;
pub mod coverage;
pub mod error;
pub mod format;
pub mod project;
pub mod selection;
pub mod test_model;

pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
