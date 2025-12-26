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

#[cfg(test)]
mod istanbul_text_test;
