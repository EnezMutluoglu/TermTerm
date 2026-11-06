#[cfg(windows)]
fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    if let Err(error) = runtime.block_on(termterm_vpn_windows_broker::windows::validation::entry(std::env::args().skip(1).collect())) {
        // Errors carry fixed broker descriptions, never imported config or credentials.
        eprintln!("{error:#}"); std::process::exit(1);
    }
}
#[cfg(not(windows))]
fn main() { eprintln!("Windows VPN broker requires Windows"); std::process::exit(1); }
