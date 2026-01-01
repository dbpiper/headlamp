#[cfg(feature = "desktop")]
mod app;
#[cfg(feature = "desktop")]
mod ui;
#[cfg(feature = "desktop")]
mod util;
#[cfg(feature = "desktop")]
mod view;

#[cfg(feature = "desktop")]
fn main() {
    dioxus::launch(app::App);
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!(
        "treemap_viewer desktop UI is disabled. Rebuild with `-p treemap_viewer --features desktop`."
    );
    std::process::exit(1);
}
