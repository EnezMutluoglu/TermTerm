use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{io::Write, path::PathBuf, sync::Arc, time::{Duration, Instant}};
use termterm_vpn_windows_broker::{transaction::{Journal, Object}, windows::{network::OwnedNetwork, orphan, store::{self, ProtectedRoot}, wintun::{self, Wintun}}};
use uuid::Uuid;
use windows_sys::Win32::{Devices::DeviceAndDriverInstallation::*, NetworkManagement::IpHelper::*, Networking::WinSock::AF_INET};

const GUID: &str = "d09975b4-5ab5-472c-9d7d-5440627c313e";
const CAP: Duration = Duration::from_secs(60);

fn emit(value: Value) -> Result<()> {
    let mut output = std::io::stdout().lock();
    writeln!(output, "{value}")?;
    output.flush()?;
    Ok(())
}

fn rows(guid: Uuid, level: MIB_IF_TABLE_LEVEL) -> Result<Vec<Value>> {
    let mut table = std::ptr::null_mut();
    let code = unsafe { GetIfTable2Ex(level, &mut table) };
    ensure!(code == 0, "GetIfTable2Ex: {code}");
    let result = unsafe { std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize) }.iter()
        .filter(|row| wintun::from_guid(row.InterfaceGuid) == guid)
        .map(|row| {
            let text = |v: &[u16]| String::from_utf16_lossy(&v[..v.iter().position(|c| *c == 0).unwrap_or(v.len())]);
            json!({"guid":guid,"luid":unsafe {row.InterfaceLuid.Value}.to_string(),"index":row.InterfaceIndex,
                "alias":text(&row.Alias),"description":text(&row.Description),"type":row.Type,
                "operStatus":row.OperStatus,"adminStatus":row.AdminStatus,"mediaConnectState":row.MediaConnectState,
                "statusFlags":row.InterfaceAndOperStatusFlags._bitfield})
        }).collect();
    unsafe { FreeMibTable(table.cast()); }
    Ok(result)
}

fn snapshot(guid: Uuid, luid: Option<u64>) -> Result<Value> {
    let instance = format!("SWD\\WINTUN\\{{{}}}", guid.to_string().to_uppercase()).encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut node = 0;
    let phantom = unsafe { CM_Locate_DevNodeW(&mut node, instance.as_ptr(), CM_LOCATE_DEVNODE_PHANTOM) };
    let mut flags = 0; let mut problem = 0;
    let status = if phantom == CR_SUCCESS { unsafe { CM_Get_DevNode_Status(&mut flags, &mut problem, node, 0) } } else { phantom };
    let mut normal_node = 0;
    let normal = unsafe { CM_Locate_DevNodeW(&mut normal_node, instance.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) };
    let mut ip = MIB_IPINTERFACE_ROW::default();
    unsafe { InitializeIpInterfaceEntry(&mut ip); }
    ip.Family = AF_INET;
    ip.InterfaceLuid.Value = luid.unwrap_or(0);
    let ip_code = luid.map(|_| unsafe { GetIpInterfaceEntry(&mut ip) });
    let normal_rows = rows(guid, MibIfTableNormal)?;
    let raw_rows = rows(guid, MibIfTableRaw)?;
    let selected_index = normal_rows.first().and_then(|row| row["index"].as_u64()).map(|v| v as u32);
    let mut stacks = std::ptr::null_mut();
    let stack_code = unsafe { GetIfStackTable(&mut stacks) };
    let related = if stack_code == 0 {
        let result = unsafe { std::slice::from_raw_parts((*stacks).Table.as_ptr(), (*stacks).NumEntries as usize) }.iter()
            .filter(|row| selected_index.is_some_and(|i| row.HigherLayerInterfaceIndex == i || row.LowerLayerInterfaceIndex == i))
            .map(|row| json!({"higherIndex":row.HigherLayerInterfaceIndex,"lowerIndex":row.LowerLayerInterfaceIndex})).collect::<Vec<_>>();
        unsafe { FreeMibTable(stacks.cast()); }
        result
    } else { vec![] };
    Ok(json!({"guid":guid,"expectedLuid":luid.map(|v|v.to_string()),"normal":normal_rows,"raw":raw_rows,
        "pnp":{"phantomLocateCode":phantom,"normalLocateCode":normal,"statusCode":status,"flags":flags,"problem":problem},
        "ipv4Code":ip_code,"stackCode":stack_code,"relatedStackRows":related}))
}

fn absent(snapshot: &Value) -> bool {
    snapshot["normal"].as_array().is_some_and(Vec::is_empty)
        && snapshot["raw"].as_array().is_some_and(Vec::is_empty)
        && snapshot["pnp"]["phantomLocateCode"].as_u64() == Some(u64::from(CR_NO_SUCH_DEVNODE))
        && snapshot["pnp"]["normalLocateCode"].as_u64() == Some(u64::from(CR_NO_SUCH_DEVNODE))
}

fn probe() -> Result<()> {
    let guid: Uuid = GUID.parse()?;
    ensure!(termterm_vpn_windows_broker::windows::identity::process_identity(unsafe {windows_sys::Win32::System::Threading::GetCurrentProcess()})?.elevated, "probe_requires_explicit_elevation");
    let _lock = store::machine_lock()?;
    let root = ProtectedRoot::open()?;
    ensure!(root.journal_count()? == 0, "probe_refuses_existing_journals");
    ensure!(wintun::termterm_interface_inventory()?.is_empty(), "probe_refuses_existing_termterm_interfaces");
    ensure!(orphan::devnode_absent(guid)? && wintun::lookup_guid(guid)?.is_none(), "probe_guid_already_present");
    let baseline = wintun::interface_inventory()?;
    let dll = PathBuf::from(std::env::current_exe()?.canonicalize()?.parent().context("probe_directory")?).join("wintun.dll");
    let api = Wintun::load(&dll)?;
    let mut network = OwnedNetwork::new(api);
    let mut journal = Journal::new(Uuid::new_v4(), 1)?;
    let mut disk = root.store(&journal);
    let started = Instant::now();
    emit(json!({"stage":"before","sessionId":journal.session_id,"snapshot":snapshot(guid,None)?,"mutations":["one_owned_adapter","one_packet_session"],"configuredAddressesRoutesDns":false,"watchdog":false}))?;
    journal.apply(Object::Adapter { guid, name:format!("TermTerm VPN {}",guid.simple()) }, &mut disk, &mut network)?;
    let adapter = network.adapters.get(&guid).context("probe_adapter_missing")?.clone();
    let luid = adapter.luid;
    emit(json!({"stage":"created","elapsedMs":started.elapsed().as_millis(),"snapshot":snapshot(guid,Some(luid))?}))?;
    let session = adapter.start(1400)?;
    std::thread::sleep(Duration::from_millis(250));
    drop(session); drop(adapter);
    ensure!(Arc::strong_count(network.adapters.get(&guid).context("probe_adapter_missing")?) == 1,"probe_session_still_referenced");
    emit(json!({"stage":"session_ended","elapsedMs":started.elapsed().as_millis(),"snapshot":snapshot(guid,Some(luid))?}))?;
    let close = journal.cleanup(&mut disk, &mut network);
    emit(json!({"stage":"close_returned","elapsedMs":started.elapsed().as_millis(),"error":close.as_ref().err().map(|e|format!("{e:#}")),"snapshot":snapshot(guid,Some(luid))?}))?;
    let observing = Instant::now();
    loop {
        let observed = snapshot(guid,Some(luid))?;
        let gone = absent(&observed);
        emit(json!({"stage":"read_only_observation","afterCloseMs":observing.elapsed().as_millis(),"snapshot":observed}))?;
        if gone {
            journal.cleanup(&mut disk, &mut network)?;
            disk.remove(&journal)?;
            let mut after = wintun::interface_inventory()?; let mut before = baseline; after.sort_unstable(); before.sort_unstable();
            ensure!(after == before, "other_registered_interface_inventory_changed");
            ensure!(root.journal_count()? == 0,"probe_journal_remains");
            return emit(json!({"stage":"finished","ok":true,"afterCloseMs":observing.elapsed().as_millis(),"remainingJournals":0,"sameOtherInterfaceInventory":true}));
        }
        if observing.elapsed() >= CAP {
            emit(json!({"stage":"pending_at_cap","ok":false,"seconds":CAP.as_secs(),"journalPreserved":true,"furtherRecoveryPerformed":false}))?;
            anyhow::bail!("probe_unregister_pending_at_60_seconds");
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [action] if action == "--describe" => emit(json!({"readOnly":true,"guid":GUID,"probeCommand":"--probe-single-owned-adapter","observationCapSeconds":60,"configuredAddressesRoutesDns":false,"safety":"existing production driver hash/sweep guards, exclusive broker lock, empty protected journal preflight; never recovers existing journals"})),
        [action] if action == "--probe-single-owned-adapter" => probe(),
        [action, guid] if action == "--observe" => emit(snapshot(guid.parse()?,None)?),
        _ => anyhow::bail!("expected --describe, --observe GUID, or --probe-single-owned-adapter"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absence_requires_both_tables_and_both_pnp_absence_codes() {
        let complete=json!({"normal":[],"raw":[],"pnp":{"phantomLocateCode":CR_NO_SUCH_DEVNODE,"normalLocateCode":CR_NO_SUCH_DEVNODE}});
        assert!(absent(&complete));
        for key in ["normal","raw"] {let mut changed=complete.clone();changed[key]=json!([{"guid":GUID}]);assert!(!absent(&changed));}
        for key in ["phantomLocateCode","normalLocateCode"] {let mut changed=complete.clone();changed["pnp"][key]=json!(CR_SUCCESS);assert!(!absent(&changed));}
        assert!(!absent(&json!({})));
    }
}
