pub mod args;
pub mod config;
pub mod coverage;
pub mod error;
pub mod format;
pub mod project;
pub mod selection;
pub mod test_model;

pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
