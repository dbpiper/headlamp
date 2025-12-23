use crate::format::ansi;

pub fn success(text: &str) -> String {
    ansi::green(text)
}

pub fn failure(text: &str) -> String {
    ansi::red(text)
}

pub fn run(text: &str) -> String {
    ansi::cyan(text)
}

pub fn skip(text: &str) -> String {
    ansi::yellow(text)
}

pub fn todo(text: &str) -> String {
    ansi::magenta(text)
}

pub fn bg_success(text: &str) -> String {
    ansi::bg_green(text)
}

pub fn bg_failure(text: &str) -> String {
    ansi::bg_red(text)
}

pub fn bg_run(text: &str) -> String {
    ansi::bg_gray(text)
}
