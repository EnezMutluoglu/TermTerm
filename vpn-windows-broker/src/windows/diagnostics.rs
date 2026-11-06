//! Bounded admin-only evidence for explicit privileged-validation builds.
use super::{orphan,store::ProtectedRoot,wintun::lookup_guid};
use crate::transaction::{Journal,Object};
use anyhow::{ensure,Result};
use serde::Serialize;
use std::{fs::File,io::Write,sync::Mutex,time::Instant};
use uuid::Uuid;

const LIMIT:u64=65536;
const CODES:&[&str]=&["windows_interface_unregister_pending","wireguard_peer_unresponsive","packet_channel_closed","control_channel_closed","broker_control_or_watchdog_lost","adapter_still_in_use","adapter_still_referenced","owned_adapter_close","owned_adapter_still_present_cleanup_required","interface_identity_changed_after_owned_adapter_close","normal_close_owned_phantom_cleanup","normal_close_journal_retry","owned_phantom_cleanup","owned_guid_present_without_devnode","owned_device_is_not_phantom","unknown_phantom_status","owned_phantom_removal_not_confirmed","cleanup_not_complete","journal_acl_or_owner_not_admin_protected"];

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Event<'a> {
    stage:&'a str,elapsed_ms:u128,process_id:u32,session_id:Option<Uuid>,generation:Option<u64>,
    guid:Option<Uuid>,expected_luid:Option<u64>,observed_luid:Option<u64>,devnode_absent:Option<bool>,native_query_failed:bool,
    unfinished_entries:Option<usize>,error_codes:Vec<&'static str>,
}
fn codes(error:Option<&anyhow::Error>)->Vec<&'static str> {
    // Match fixed codes only; raw errors can contain imported text or private data.
    let messages=error.into_iter().flat_map(|error|error.chain()).map(ToString::to_string).collect::<Vec<_>>();
    CODES.iter().copied().filter(|code|messages.iter().any(|message|message.split(|c:char|!c.is_ascii_alphanumeric() && c!='_').any(|word|word==*code))).collect()
}
pub(super) struct Diagnostics {file:Mutex<File>,started:Instant,_root:Option<ProtectedRoot>}
impl Diagnostics {
    pub fn open()->Result<Self> {let root=ProtectedRoot::diagnostics()?;let file=root.diagnostic_file()?;Ok(Self {file:Mutex::new(file),started:Instant::now(),_root:Some(root)})}
    pub fn record(&self,stage:&'static str,journal:Option<&Journal>,guid:Option<Uuid>,luid:Option<u64>,error:Option<&anyhow::Error>) {
        let guid=guid.or_else(||journal.and_then(|j|j.entries.iter().find_map(|e|if let Object::Adapter {guid,..}=e.object {Some(guid)} else {None})));
        let luid=luid.or_else(||journal.and_then(|j|j.entries.iter().find_map(|e|match e.object {Object::Address {luid,..}|Object::Route {luid,..}=>Some(luid),_=>None})));
        let (observed,absent)=guid.map(|guid|(lookup_guid(guid),orphan::devnode_absent(guid))).unwrap_or((Ok(None),Ok(true)));
        let failed=observed.is_err() || absent.is_err();
        let event=Event {stage,elapsed_ms:self.started.elapsed().as_millis(),process_id:std::process::id(),session_id:journal.map(|j|j.session_id),generation:journal.map(|j|j.generation),guid,expected_luid:luid,observed_luid:observed.ok().flatten(),devnode_absent:if guid.is_some() {absent.ok()} else {None},native_query_failed:failed,unfinished_entries:journal.map(|j|j.entries.iter().filter(|e|!e.undone).count()),error_codes:codes(error)};
        // Diagnostic I/O must never interrupt or authorize network cleanup.
        let _=self.append(&event);
    }
    fn append(&self,event:&Event<'_>)->Result<()> {
        ensure!(matches!(event.stage,"stop_requested"|"quiesced"|"owner_exited"|"cleanup_begin"|"cleanup_failed"|"journal_removed"|"adapter_close_begin"|"adapter_handle_closed"|"adapter_close_finished"|"recovery_journal"|"watchdog_recovery_begin"|"watchdog_recovery_failed"|"watchdog_recovery_finished"),"invalid_validation_diagnostic_stage");
        let mut bytes=serde_json::to_vec(event)?;bytes.push(b'\n');ensure!(bytes.len()<=4096,"validation_diagnostic_frame_too_large");
        let mut file=self.file.lock().map_err(|_|anyhow::anyhow!("validation_diagnostic_lock_failed"))?;
        ensure!(file.metadata()?.len()+bytes.len() as u64<=LIMIT,"validation_diagnostic_limit_reached");file.write_all(&bytes)?;file.sync_all()?;Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_redact_untrusted_errors_and_bound_the_file() {
        let temporary=tempfile::tempfile().unwrap();let reader=temporary.try_clone().unwrap();
        let diagnostic=Diagnostics {file:Mutex::new(temporary),started:Instant::now(),_root:None};
        let error=anyhow::anyhow!("password=private-secret private_key=AAAA owned_adapter_still_present_cleanup_required");
        for _ in 0..500 {diagnostic.record("cleanup_failed",None,None,None,Some(&error));}
        assert!(reader.metadata().unwrap().len()<=LIMIT);
        use std::io::{Read,Seek};let mut reader=reader;reader.rewind().unwrap();let mut text=String::new();reader.read_to_string(&mut text).unwrap();
        assert!(text.contains("owned_adapter_still_present_cleanup_required"));assert!(!text.contains("private-secret") && !text.contains("AAAA") && !text.contains("private_key"));
        for line in text.lines() {let value:serde_json::Value=serde_json::from_str(line).unwrap();assert_eq!(value["stage"],"cleanup_failed");}
        let event=Event {stage:"private-secret",elapsed_ms:0,process_id:0,session_id:None,generation:None,guid:None,expected_luid:None,observed_luid:None,devnode_absent:None,native_query_failed:false,unfinished_entries:None,error_codes:vec![]};
        assert!(diagnostic.append(&event).is_err());
    }
    #[test]
    fn diagnostic_root_rejects_unelevated_creation() {
        if !super::super::identity::process_identity(unsafe {windows_sys::Win32::System::Threading::GetCurrentProcess()}).unwrap().elevated {assert!(Diagnostics::open().is_err());}
    }
}
