pub fn bold(text: &str) -> String {
    format!("\u{1b}[1m{text}\u{1b}[22m")
}

pub fn dim(text: &str) -> String {
    format!("\u{1b}[2m{text}\u{1b}[22m")
}

pub fn black(text: &str) -> String {
    format!("\u{1b}[30m{text}\u{1b}[39m")
}

pub fn red(text: &str) -> String {
    format!("\u{1b}[31m{text}\u{1b}[39m")
}

pub fn yellow(text: &str) -> String {
    format!("\u{1b}[33m{text}\u{1b}[39m")
}

pub fn green(text: &str) -> String {
    format!("\u{1b}[32m{text}\u{1b}[39m")
}

pub fn magenta(text: &str) -> String {
    format!("\u{1b}[35m{text}\u{1b}[39m")
}

pub fn gray(text: &str) -> String {
    format!("\u{1b}[90m{text}\u{1b}[39m")
}

pub fn cyan(text: &str) -> String {
    format!("\u{1b}[36m{text}\u{1b}[39m")
}

pub fn white(text: &str) -> String {
    format!("\u{1b}[97m{text}\u{1b}[39m")
}

pub fn bg_red(text: &str) -> String {
    format!("\u{1b}[41m{text}\u{1b}[49m")
}

pub fn bg_green(text: &str) -> String {
    format!("\u{1b}[42m{text}\u{1b}[49m")
}

pub fn bg_magenta(text: &str) -> String {
    format!("\u{1b}[45m{text}\u{1b}[49m")
}

pub fn bg_cyan(text: &str) -> String {
    format!("\u{1b}[46m{text}\u{1b}[49m")
}

pub fn bg_gray(text: &str) -> String {
    format!("\u{1b}[100m{text}\u{1b}[49m")
}

pub fn osc8(text: &str, url: &str) -> String {
    format!("\u{1b}]8;;{url}\u{7}{text}\u{1b}]8;;\u{7}")
}
