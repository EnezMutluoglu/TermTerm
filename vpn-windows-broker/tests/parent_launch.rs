#![cfg(all(windows,feature="privileged-validation"))]
use termterm_vpn_windows_broker::windows::{identity,validation};

#[tokio::test]
async fn parent_token_explicit_handles_raw_packets_and_job_close_without_host_network() {
    let owner=identity::Process::open(std::process::id()).unwrap();assert!(!owner.identity.elevated);
    let value=validation::engine_launch_self_test(&owner,std::path::Path::new(env!("CARGO_BIN_EXE_termterm-vpn-broker"))).await.unwrap();
    assert_eq!(value["ok"],true);assert_eq!(value["createdAdapters"],0);assert_eq!(value["probes"].as_array().unwrap().len(),2);
}
