fn main() {
    #[cfg(target_os = "linux")]
    termterm_lib::native_webkit_response_fixture();
    #[cfg(not(target_os = "linux"))]
    eprintln!("VPN WebKit fixture requires Linux; skipped on this platform");
}
