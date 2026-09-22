use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default()
}
pub fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".into()
    } else {
        std::env::var("SHELL")
            .ok()
            .filter(|s| Path::new(s).is_absolute() && Path::new(s).is_file())
            .or_else(login_shell)
            .unwrap_or_else(|| "/bin/sh".into())
    }
}
#[cfg(unix)]
fn login_shell() -> Option<String> {
    let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; 16384];
    // getpwuid_r writes into caller-owned storage; copy pw_shell before dropping that storage.
    unsafe {
        if libc::getpwuid_r(
            libc::getuid(),
            entry.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        ) != 0
            || result.is_null()
        {
            return None;
        }
        let shell = (*result).pw_shell;
        if shell.is_null() {
            return None;
        }
        let shell = std::ffi::CStr::from_ptr(shell).to_str().ok()?.to_string();
        (Path::new(&shell).is_absolute() && Path::new(&shell).is_file()).then_some(shell)
    }
}
#[cfg(not(unix))]
fn login_shell() -> Option<String> {
    None
}
pub fn validate_shell(shell: &str) -> Result<()> {
    if cfg!(windows) {
        ensure!(
            ["powershell.exe", "pwsh.exe", "cmd.exe", "wsl.exe"].contains(&shell),
            "Choose a supported local shell"
        );
    } else {
        ensure!(
            Path::new(shell).is_absolute() && Path::new(shell).is_file(),
            "Choose an installed shell by absolute path"
        );
    }
    Ok(())
}
pub fn info() -> serde_json::Value {
    serde_json::json!({"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"defaultShell":default_shell(),"agentKinds":if cfg!(windows){vec!["openssh","pageant"]}else{vec!["openssh"]},"rememberStore":if cfg!(windows){"Windows DPAPI"}else if cfg!(target_os="macos"){"macOS Keychain"}else{"Secret Service"}})
}
pub fn edit(path: &Path, editor: Option<&str>) -> Result<()> {
    let mut cmd = if let Some(editor) = editor.filter(|s| !s.trim().is_empty()) {
        ensure!(
            Path::new(editor).is_absolute() && Path::new(editor).is_file(),
            "Select an editor executable by absolute path"
        );
        let mut c = std::process::Command::new(editor);
        c.arg(path);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("notepad.exe");
        c.arg(path);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.args(["-t", "--"]).arg(path);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    cmd.spawn().context("Could not open the external editor")?;
    Ok(())
}
pub fn mosh_binary(root: &Path) -> PathBuf {
    root.join("bin").join(if cfg!(windows) {
        "mosh-client.exe"
    } else {
        "mosh-client"
    })
}
