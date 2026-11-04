use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{io::Write, path::PathBuf, sync::Arc, time::{Duration, Instant}};
use termterm_vpn_windows_broker::{policy::{Assignment, Cidr, Plan}, transaction::{DnsSettings, Journal, Metric, Network, Object}, windows::{network::{self, OwnedNetwork}, orphan, store::{self, ProtectedRoot}, validation, wintun::{self, Wintun}}};
use uuid::Uuid;
use windows_sys::Win32::{Devices::DeviceAndDriverInstallation::*, NetworkManagement::IpHelper::*, Networking::WinSock::AF_INET};

const GUID: &str = "f4ce9462-cc63-4f3f-9d59-916ed5d8541a";
const CAP: Duration = Duration::from_secs(60);
const HOLD: Duration = Duration::from_secs(150);

fn plan() -> Result<Plan> {
    Ok(Plan {addresses:vec![Assignment {cidr:Cidr::parse("10.97.0.2/32")?,gateway:None}],
        routes:vec![Cidr::parse("10.97.0.1/32")?,Cidr::parse("10.88.0.53/32")?],mtu:1420,
        dns_servers:vec!["10.88.0.53".parse()?],search_domain:"d3366f27b0f1e5a7.termterm.test".into(),
        excluded_routes:vec![],remote:None,full_tunnel:false})
}

fn kind(object: &Object) -> &'static str {
    match object {Object::Adapter {..}=>"adapter",Object::Address {..}=>"address",Object::Mtu {..}=>"mtu",Object::Route {..}=>"route",Object::Metric {..}=>"metric",Object::Dns {..}=>"dns"}
}
struct CleanupTrace<'a,N> {network:&'a mut N,started:Instant}
impl<N:Network> Network for CleanupTrace<'_,N> {
    fn prepare(&self,object:&Object)->Result<()> {self.network.prepare(object)}
    fn apply(&mut self,object:&Object)->Result<()> {self.network.apply(object)}
    fn undo(&mut self,object:&Object)->Result<()> {
        let started=Instant::now();
        let _=emit(json!({"stage":"undo_begin","kind":kind(object),"elapsedMs":self.started.elapsed().as_millis()}));
        let result=self.network.undo(object);
        let _=emit(json!({"stage":"undo_returned","kind":kind(object),"elapsedMs":self.started.elapsed().as_millis(),"operationMs":started.elapsed().as_millis(),"error":result.as_ref().err().map(|e|format!("{e:#}"))}));
        result
    }
}

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
    let plan=plan()?;
    ensure!(termterm_vpn_windows_broker::windows::identity::process_identity(unsafe {windows_sys::Win32::System::Threading::GetCurrentProcess()})?.elevated, "probe_requires_explicit_elevation");
    let _lock = store::machine_lock()?;
    let root = ProtectedRoot::open()?;
    ensure!(root.journal_count()? == 0, "probe_refuses_existing_journals");
    ensure!(wintun::termterm_interface_inventory()?.is_empty(), "probe_refuses_existing_termterm_interfaces");
    ensure!(orphan::devnode_absent(guid)? && wintun::lookup_guid(guid)?.is_none(), "probe_guid_already_present");
    validation::check_host_plan(&plan,&[]).context("configured_probe_unowned_network_overlap_preflight")?;
    let baseline = wintun::interface_inventory()?;
    let dll = PathBuf::from(std::env::current_exe()?.canonicalize()?.parent().context("probe_directory")?).join("wintun.dll");
    let api = Wintun::load(&dll)?;
    let mut network = OwnedNetwork::new(api);
    let mut journal = Journal::new(Uuid::new_v4(), 1)?;
    let mut disk = root.store(&journal);
    let started = Instant::now();
    emit(json!({"stage":"before","sessionId":journal.session_id,"snapshot":snapshot(guid,None)?,"plan":plan,"preDnsHoldSeconds":HOLD.as_secs(),"noEngineOrApplicationTraffic":true,"watchdog":false}))?;
    journal.apply(Object::Adapter { guid, name:format!("TermTerm VPN {}",guid.simple()) }, &mut disk, &mut network)?;
    let adapter = network.adapters.get(&guid).context("probe_adapter_missing")?.clone();
    let luid = adapter.luid;
    emit(json!({"stage":"created","elapsedMs":started.elapsed().as_millis(),"snapshot":snapshot(guid,Some(luid))?}))?;
    let mut session=None;
    let setup=(||->Result<()> {
        journal.apply(Object::Address {guid,luid,cidr:plan.addresses[0].cidr.clone()},&mut disk,&mut network)?;
        let before=OwnedNetwork::mtu(guid,luid,false)?;
        journal.apply(Object::Mtu {guid,luid,ipv6:false,before,after:plan.mtu},&mut disk,&mut network)?;
        for destination in network::route_apply_order(&plan.routes) {
            journal.apply(Object::Route {guid,luid,destination:destination.clone(),metric:5},&mut disk,&mut network)?;
        }
        session=Some(adapter.start(plan.mtu as usize)?);
        let dad=Instant::now();
        while !network.address_ready(guid,luid,&plan.addresses[0].cidr)? {
            ensure!(dad.elapsed()<Duration::from_secs(10),"configured_probe_dad_timeout");
            std::thread::sleep(Duration::from_millis(200));
        }
        emit(json!({"stage":"pre_dns_hold_begin","elapsedMs":started.elapsed().as_millis(),"holdSeconds":HOLD.as_secs(),"configuredDns":false,"snapshot":snapshot(guid,Some(luid))?}))?;
        std::thread::sleep(HOLD);
        emit(json!({"stage":"pre_dns_hold_finished","elapsedMs":started.elapsed().as_millis(),"configuredDns":false,"snapshot":snapshot(guid,Some(luid))?}))?;
        network::verify_dns_routes(&plan.dns_servers,luid)?;
        let before=OwnedNetwork::metric(guid,luid)?;
        journal.apply(Object::Metric {guid,luid,before,after:Metric {automatic:false,value:1}},&mut disk,&mut network)?;
        let before=OwnedNetwork::dns(guid,luid)?;
        ensure!(before==DnsSettings::default(),"new_adapter_dns_already_configured");
        let after=DnsSettings {servers:plan.dns_servers.clone(),search_domain:plan.search_domain.clone(),search_list:vec![plan.search_domain.clone()]};
        journal.apply(Object::Dns {guid,luid,before,after},&mut disk,&mut network)?;
        emit(json!({"stage":"dns_applied_immediate_cleanup_next","elapsedMs":started.elapsed().as_millis(),"unfinishedEntries":journal.entries.iter().filter(|e|!e.undone).count()}))?;
        Ok(())
    })();
    drop(session); drop(adapter);
    ensure!(Arc::strong_count(network.adapters.get(&guid).context("probe_adapter_missing")?) == 1,"probe_session_still_referenced");
    let _=emit(json!({"stage":"session_ended","elapsedMs":started.elapsed().as_millis(),"setupError":setup.as_ref().err().map(|e|format!("{e:#}")),"snapshot":snapshot(guid,Some(luid)).ok()}));
    let close = journal.cleanup(&mut disk, &mut CleanupTrace {network:&mut network,started});
    emit(json!({"stage":"close_returned","elapsedMs":started.elapsed().as_millis(),"error":close.as_ref().err().map(|e|format!("{e:#}")),"snapshot":snapshot(guid,Some(luid))?}))?;
    let observing = Instant::now();
    loop {
        let observed = snapshot(guid,Some(luid))?;
        let gone = absent(&observed);
        emit(json!({"stage":"read_only_observation","afterCloseMs":observing.elapsed().as_millis(),"snapshot":observed}))?;
        if gone {
            journal.cleanup(&mut disk, &mut CleanupTrace {network:&mut network,started})?;
            disk.remove(&journal)?;
            let mut after = wintun::interface_inventory()?; let mut before = baseline; after.sort_unstable(); before.sort_unstable();
            ensure!(after == before, "other_registered_interface_inventory_changed");
            ensure!(root.journal_count()? == 0,"probe_journal_remains");
            emit(json!({"stage":"finished","ok":setup.is_ok(),"afterCloseMs":observing.elapsed().as_millis(),"remainingJournals":0,"sameOtherInterfaceInventory":true}))?;
            return setup;
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
        [action] if action == "--describe" => emit(json!({"readOnly":true,"guid":GUID,"probeCommand":"--probe-single-owned-adapter","observationCapSeconds":60,"preDnsHoldSeconds":150,"configuredAddressesRoutesDns":true,"plan":plan()?,"dnsDwell":"apply then immediate reverse cleanup; no deliberate resolver queries","safety":"existing production driver hash/sweep/host-route overlap guards, exclusive broker lock, empty protected journal preflight; never recovers existing journals"})),
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
    #[test]
    fn exact_v5_plan_is_specific_and_rejects_unowned_address_or_route_overlap() {
        let plan=plan().unwrap();
        assert_eq!(plan.addresses[0].cidr,Cidr::parse("10.97.0.2/32").unwrap());
        assert_eq!(plan.mtu,1420);assert!(!plan.full_tunnel);assert!(plan.excluded_routes.is_empty());
        assert_eq!(plan.routes.iter().map(Cidr::text).collect::<Vec<_>>(),["10.97.0.1/32","10.88.0.53/32"]);
        assert_eq!(plan.dns_servers,["10.88.0.53".parse::<std::net::IpAddr>().unwrap()]);
        assert_eq!(plan.search_domain,"d3366f27b0f1e5a7.termterm.test");
        for existing in ["10.97.0.2/32","10.97.0.1/32","10.88.0.53/32","10.0.0.0/8"] {
            for connected in [false,true] {assert!(plan.host_route_conflicts(&Cidr::parse(existing).unwrap(),connected));}
        }
        assert!(!plan.host_route_conflicts(&Cidr::parse("192.0.2.0/24").unwrap(),false));
    }
    #[test]
    fn configured_undo_failure_retains_intent_and_retry_restores_dns_first_adapter_last() {
        use termterm_vpn_windows_broker::transaction::{Entry, Store};
        #[derive(Default)] struct Disk;
        impl Store for Disk {fn save(&mut self,_:&Journal)->Result<()> {Ok(())}}
        struct Fake {fail:bool,undone:Vec<String>}
        impl Network for Fake {
            fn prepare(&self,_:&Object)->Result<()> {panic!("cleanup must not prepare new mutations")}
            fn apply(&mut self,_:&Object)->Result<()> {panic!("cleanup must not apply new mutations")}
            fn undo(&mut self,object:&Object)->Result<()> {
                self.undone.push(kind(object).into());ensure!(!self.fail,"inert_dns_restore_failure");Ok(())
            }
        }
        let guid=GUID.parse().unwrap();let luid=17;let plan=plan().unwrap();
        let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();
        let objects=vec![Object::Adapter {guid,name:format!("TermTerm VPN {}",guid.simple())},
            Object::Address {guid,luid,cidr:plan.addresses[0].cidr.clone()},Object::Mtu {guid,luid,ipv6:false,before:1500,after:1420},
            Object::Route {guid,luid,destination:plan.routes[0].clone(),metric:5},Object::Route {guid,luid,destination:plan.routes[1].clone(),metric:5},
            Object::Metric {guid,luid,before:Metric {automatic:true,value:25},after:Metric {automatic:false,value:1}},
            Object::Dns {guid,luid,before:DnsSettings::default(),after:DnsSettings {servers:plan.dns_servers,search_domain:plan.search_domain.clone(),search_list:vec![plan.search_domain]}}];
        journal.entries=objects.into_iter().map(|object|Entry {object,undone:false}).collect();
        let mut network=Fake {fail:true,undone:vec![]};let mut disk=Disk;
        assert!(journal.cleanup(&mut disk,&mut CleanupTrace {network:&mut network,started:Instant::now()}).is_err());
        assert_eq!(network.undone,["dns"]);assert!(journal.entries.iter().all(|e|!e.undone));
        network.fail=false;network.undone.clear();
        journal.cleanup(&mut disk,&mut CleanupTrace {network:&mut network,started:Instant::now()}).unwrap();
        assert!(journal.complete());assert_eq!(network.undone,["dns","metric","route","route","mtu","address","adapter"]);
    }
}
