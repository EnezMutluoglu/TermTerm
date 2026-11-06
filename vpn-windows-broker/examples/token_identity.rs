//! Read-only self/OS-linked token inspection. Never elevates or sends credentials.
#[cfg(windows)]
fn main()->anyhow::Result<()> {
    use termterm_vpn_windows_broker::windows::identity;
    use sha2::{Digest,Sha256};
    let current=unsafe {windows_sys::Win32::System::Threading::GetCurrentProcess()};
    let token=identity::process_token(current)?;let own=identity::from_token(token.raw())?;
    let linked=identity::linked_token(token.raw())?;let other=identity::from_token(linked.raw())?;
    let describe=|value:&identity::Identity|serde_json::json!({"userSidSha256":format!("{:x}",Sha256::digest(&value.user)),"logonSidSha256":format!("{:x}",Sha256::digest(&value.logon_sid)),"authenticationId":[value.authentication_id.0,value.authentication_id.1],"sessionId":value.session_id,"elevated":value.elevated});
    println!("{}",serde_json::json!({"readOnly":true,"currentPid":std::process::id(),"currentBirth":identity::birth_time(current)?,"currentType":identity::elevation_type(token.raw())?,"linkedType":identity::elevation_type(linked.raw())?,"current":describe(&own),"linked":describe(&other),"sameUser":own.user==other.user,"sameLogonSid":own.logon_sid==other.logon_sid,"sameAuthenticationId":own.authentication_id==other.authentication_id,"sameSession":own.session_id==other.session_id}));Ok(())
}
#[cfg(not(windows))]
fn main(){eprintln!("Token identity inspection requires Windows");}
