mod analysis;
mod api;
mod bars;
mod column_widths;
mod istanbul_text;
mod merge;
mod model;
mod path_shorten;
mod per_file_table;
mod table;

pub use api::format_istanbul_pretty;
pub use api::format_istanbul_pretty_from_lcov_report;

#[cfg(test)]
mod istanbul_text_test;

#[cfg(test)]
mod api_lcov_test;
