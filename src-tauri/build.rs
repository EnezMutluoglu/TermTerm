fn main() {
    println!("cargo:rerun-if-env-changed=TERMTERM_RELEASE_CHANNEL");
    tauri_build::build();
    // Rust lib-test executables also link the native dialog code. Without a
    // Common Controls v6 activation manifest Windows loads v5 and cannot find
    // TaskDialogIndirect, so the test process fails before running any tests.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
        // Tauri already supplies resource #1 to the application executable.
        // Keep the generated activation manifest only for lib/test targets.
        println!("cargo:rustc-link-arg-bin=termterm=/MANIFEST:NO");
    }
}
