#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if std::env::args().any(|arg| arg == "--termterm-keepass-worker") {
        termterm_lib::keepass_import::worker_main();
        return;
    }
    termterm_lib::run();
}
