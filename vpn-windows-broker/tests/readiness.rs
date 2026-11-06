#![cfg(windows)]
use std::os::windows::process::CommandExt;

#[test]
fn normal_readiness_reports_the_validated_development_path_and_policy_limits() {
    let readiness=termterm_vpn_windows_broker::windows::readiness();assert!(readiness.available);assert!(readiness.reason.is_empty());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_termterm-vpn-broker")).arg("--capabilities").creation_flags(0x08000000).output().unwrap();
    assert!(output.status.success());let value:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(value["available"],true);
    assert_eq!(value["requiresPrivilegedValidation"],false);assert_eq!(value["dns"],true);assert_eq!(value["fullTunnel"],true);assert_eq!(value["maxConnections"],3);
    assert_eq!(value["version"],2);assert_eq!(value["networkLease"],true);assert_eq!(value["quiesce"],true);
}
#[tokio::test]
async fn ordinary_server_rejects_unelevated_execution_before_protected_state_or_dll_access() {
    let owner=termterm_vpn_windows_broker::windows::identity::Process::open(std::process::id()).unwrap();
    let error=termterm_vpn_windows_broker::windows::runtime::serve(uuid::Uuid::new_v4(),owner.pid,owner.birth,false).await.unwrap_err();
    assert!(error.to_string().contains("elevated_broker_required"),"{error:#}");
}
#[test]
fn watchdog_handle_whitelist_launches_but_unelevated_child_never_touches_network() {
    use termterm_vpn_windows_broker::windows::process;
    let lock=process::duplicate(unsafe {windows_sys::Win32::System::Threading::GetCurrentProcess()},false).unwrap();
    let error=process::watchdog(std::path::Path::new(env!("CARGO_BIN_EXE_termterm-vpn-broker")),&lock).unwrap_err().to_string();
    assert!(error.contains("watchdog_not_ready"),"{error}");
}
#[cfg(not(feature="privileged-validation"))]
#[test]
fn default_build_rejects_all_privileged_diagnostics_before_elevation_or_load() {
    for args in [vec!["--engine-launch-self-test"],vec!["--parent-launch-probe"],vec!["--privileged-self-test"],vec!["--recover-owned-journals","--expected-set-sha256","0000000000000000000000000000000000000000000000000000000000000000"],vec!["--validation-client"],vec!["--serve-validation","--pipe","1ab9d9b7-8660-454f-855b-0f3feb402e6d","--parent","1","--birth","1"]] {
        let output=std::process::Command::new(env!("CARGO_BIN_EXE_termterm-vpn-broker")).args(&args).creation_flags(0x08000000).output().unwrap();
        assert!(!output.status.success(),"accepted {args:?}");let error=String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("validation_build_required") || error.contains("privileged_windows_path_not_validated"),"{args:?}: {error}");
    }
}
#[cfg(feature="privileged-validation")]
#[test]
fn recovery_diagnostic_rejects_unelevated_before_journal_or_dll_access() {
    let output=std::process::Command::new(env!("CARGO_BIN_EXE_termterm-vpn-broker")).args(["--recover-owned-journals","--expected-set-sha256",&"0".repeat(64)]).creation_flags(0x08000000).output().unwrap();
    assert!(!output.status.success());assert!(String::from_utf8_lossy(&output.stderr).contains("elevated_recovery_required"));
}
