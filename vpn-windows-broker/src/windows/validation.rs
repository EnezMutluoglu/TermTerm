//! Explicit diagnostics. The privileged self-test is never called by build/unit tests.
use super::{identity::{self, Process}, network::OwnedNetwork, process, runtime, store::ProtectedRoot, wintun::Wintun, win, Handle};
use crate::{policy::{Assignment, Cidr, Plan, Registry}, transaction::{Journal, Network, Object}};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::{BTreeSet,HashMap}, mem::zeroed, net::{IpAddr, Ipv4Addr}, ptr};
use uuid::Uuid;
use windows_sys::Win32::{Foundation::*, NetworkManagement::IpHelper::*, Networking::WinSock::*, Storage::FileSystem::*, System::{IO::*, Pipes::*, Threading::*}};

fn take(args: &mut HashMap<String,String>, key: &str) -> Result<String> { args.remove(key).ok_or_else(||anyhow::anyhow!("missing_argument: {key}")) }
fn options(args: Vec<String>) -> Result<HashMap<String,String>> {
    ensure!(args.len() % 2 == 0 && args.len() <= 16,"invalid_arguments");
    let mut result = HashMap::new();
    for pair in args.chunks_exact(2) {ensure!(pair[0].starts_with("--") && result.insert(pair[0].clone(),pair[1].clone()).is_none(),"invalid_or_duplicate_argument");} Ok(result)
}
pub async fn entry(mut args: Vec<String>) -> Result<()> {
    ensure!(!args.is_empty(),"broker_action_required"); let action = args.remove(0);
    if action == "--capabilities" { ensure!(args.is_empty(),"invalid_arguments"); println!("{}",json!({"version":crate::PROTOCOL_VERSION,"available":crate::PRIVILEGED_PATH_VALIDATED,"maxConnections":crate::MAX_CONNECTIONS,"dns":true,"fullTunnel":true,"networkLease":true,"quiesce":true,"requiresPrivilegedValidation":!crate::PRIVILEGED_PATH_VALIDATED})); return Ok(()); }
    if action == "--wintun-sweep-preflight" {
        ensure!(args.is_empty(),"invalid_arguments");let entries=super::orphan::dll_sweep_inventory()?;
        let interfaces=super::wintun::termterm_interface_inventory()?.into_iter().map(|(guid,luid,name)|json!({"guid":guid,"luid":luid.to_string(),"name":name})).collect::<Vec<_>>();
        println!("{}",json!({"readOnly":true,"safe":entries.iter().all(|v|v.safe),"devices":entries,"registeredTermTermInterfaces":interfaces}));
        ensure!(entries.iter().all(|v|v.safe),"wintun_global_sweep_would_touch_orphan_or_legacy_device");return Ok(());
    }
    if action == "--wintun-driver-preflight" {
        ensure!(args.is_empty(),"invalid_arguments");let packages=super::driver::inventory()?;
        super::driver::validate(&packages)?;
        println!("{}",json!({"readOnly":true,"safe":true,"packages":packages}));return Ok(());
    }
    let mut args = options(args)?;
    match action.as_str() {
        "--serve" | "--serve-validation" => {
            let pipe = take(&mut args,"--pipe")?.parse()?; let parent = take(&mut args,"--parent")?.parse()?; let birth = take(&mut args,"--birth")?.parse()?;
            ensure!(args.is_empty(),"unknown_argument"); runtime::serve(pipe,parent,birth,action == "--serve-validation").await
        }
        "--watchdog" => {
            ensure!(crate::PRIVILEGED_PATH_VALIDATED || cfg!(feature="privileged-validation"),"privileged_windows_path_not_validated");
            let broker = Handle::new(take(&mut args,"--broker-handle")?.parse::<usize>()? as HANDLE)?;
            let lock = Handle::new(take(&mut args,"--lock-handle")?.parse::<usize>()? as HANDLE)?;
            let ready = Handle::new(take(&mut args,"--ready-handle")?.parse::<usize>()? as HANDLE)?;
            ensure!(args.is_empty(),"unknown_argument"); runtime::watchdog(broker,lock,ready)
        }
        "--packet-probe" => {
            let packet = Handle::new(take(&mut args,"--handle")?.parse::<usize>()? as HANDLE)?;
            let parent = take(&mut args,"--parent")?.parse()?; let birth = take(&mut args,"--birth")?.parse()?; let scenario = take(&mut args,"--scenario")?;
            ensure!(args.is_empty(),"unknown_argument"); packet_probe(packet,parent,birth,&scenario)
        }
        "--parent-launch-probe" => {
            ensure!(cfg!(feature="privileged-validation"),"validation_build_required");
            let packet=Handle::new(take(&mut args,"--handle")?.parse::<usize>()? as HANDLE)?;
            let parent=take(&mut args,"--parent")?.parse()?;let birth=take(&mut args,"--birth")?.parse::<u64>()?;
            let excluded=take(&mut args,"--excluded")?.parse::<usize>()? as HANDLE;let scenario=take(&mut args,"--scenario")?;
            ensure!(args.is_empty(),"unknown_argument");parent_launch_child(packet,parent,birth,excluded,&scenario)
        }
        "--engine-launch-self-test" => {
            ensure!(cfg!(feature="privileged-validation"),"validation_build_required");
            let parent=take(&mut args,"--parent")?.parse()?;let birth=take(&mut args,"--birth")?.parse::<u64>()?;
            ensure!(args.is_empty(),"unknown_argument");
            let owner=Process::open(parent)?;ensure!(owner.birth==birth,"launch_test_parent_pid_reused");
            identity::verify_elevated_peer(&owner.identity,unsafe {GetCurrentProcess()})?;
            println!("{}",engine_launch_self_test(&owner,&std::env::current_exe()?.canonicalize()?).await?);Ok(())
        }
        "--privileged-self-test" => {
            ensure!(cfg!(feature="privileged-validation"),"validation_build_required");
            let fault = args.remove("--fault");let coexistence=args.remove("--coexistence");let routes=args.remove("--routes");ensure!(args.is_empty(),"unknown_argument");
            ensure!(fault.as_deref().is_none_or(|v|matches!(v,"broker-death"|"adapter-created")),"invalid_fault");
            ensure!(coexistence.as_deref().is_none_or(|v|v=="retained") && (coexistence.is_none() || fault.as_deref().is_none_or(|v|v=="broker-death")),"invalid_coexistence_mode");
            ensure!(routes.as_deref().is_none_or(|v|v=="nested" && fault.is_none() && coexistence.is_none()),"invalid_routes_mode");
            println!("{}",privileged_self_test(fault.as_deref(),coexistence.is_some(),routes.is_some()).await?); Ok(())
        }
        "--recover-owned-journals" => {
            ensure!(cfg!(feature="privileged-validation"),"validation_build_required");
            let expected=take(&mut args,"--expected-set-sha256")?;ensure!(args.is_empty(),"unknown_argument");
            println!("{}",recover_owned_journals(&expected)?);Ok(())
        }
        "--validation-client" => {
            ensure!(args.is_empty() && cfg!(feature="privileged-validation"),"validation_build_required");
            validation_client().await
        }
        _ => anyhow::bail!("unknown_broker_action"),
    }
}
fn recover_owned_journals(expected: &str) -> Result<Value> {
    ensure!(expected.len() == 64 && expected.bytes().all(|v|v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),"invalid_expected_journal_set_digest");
    ensure!(identity::process_identity(unsafe {GetCurrentProcess()})?.elevated,"elevated_recovery_required");
    let _lock=super::store::machine_lock().context("recovery_machine_lock")?;
    let root=ProtectedRoot::open().context("recovery_protected_root")?;
    ensure!(root.recovery_set_digest()? == expected,"recovery_journal_set_changed");
    let journals=root.journals()?;
    let before=journals.len();
    let snapshots=journals.iter().map(|(journal,_)|journal.clone()).collect::<Vec<_>>();
    let candidates=super::orphan::preflight(&snapshots).context("recovery_owned_phantom_preflight")?;
    let phantom_count=candidates.len();drop(candidates);
    let mut network=OwnedNetwork::recovering();
    runtime::recover(&root,&mut network).context("owned_journal_recovery")?;
    ensure!(root.journal_count()? == 0,"recovery_journals_remain");
    Ok(json!({"ok":true,"recoveredJournals":before,"remainingJournals":0,"expectedSetSha256":expected,"requiredOwnedInterfacesAbsent":true,"removedOwnedPhantomAdapters":phantom_count,"createdAdapters":0}))
}
fn raw_read(pipe: HANDLE, buffer: &mut [u8]) -> Result<usize> {
    let event = Handle::new(unsafe {CreateEventW(ptr::null(),1,0,ptr::null())})?;
    let mut overlap: OVERLAPPED = unsafe {zeroed()}; overlap.hEvent = event.raw(); let mut size = 0;
    let read = unsafe {ReadFile(pipe,buffer.as_mut_ptr(),buffer.len() as u32,&mut size,&mut overlap)};
    if read == 0 {
        let code = unsafe {GetLastError()}; ensure!(code == ERROR_IO_PENDING,"packet_read: {}",std::io::Error::from_raw_os_error(code as i32));
        if unsafe {WaitForSingleObject(event.raw(),5000)} != WAIT_OBJECT_0 {
            unsafe {CancelIoEx(pipe,&overlap); GetOverlappedResult(pipe,&overlap,&mut size,1);}
            anyhow::bail!("packet_read_timeout");
        }
        unsafe {win(GetOverlappedResult(pipe,&overlap,&mut size,0))?;}
    }
    Ok(size as usize)
}
fn raw_write(pipe: HANDLE, packet: &[u8]) -> Result<()> {
    let event = Handle::new(unsafe {CreateEventW(ptr::null(),1,0,ptr::null())})?;
    let mut overlap: OVERLAPPED = unsafe {zeroed()}; overlap.hEvent = event.raw(); let mut size = 0;
    if unsafe {WriteFile(pipe,packet.as_ptr(),packet.len() as u32,&mut size,&mut overlap)} == 0 {
        ensure!(unsafe {GetLastError()} == ERROR_IO_PENDING,"packet_write_failed");
        if unsafe {WaitForSingleObject(event.raw(),5000)} != WAIT_OBJECT_0 {unsafe {CancelIoEx(pipe,&overlap);GetOverlappedResult(pipe,&overlap,&mut size,1);} anyhow::bail!("packet_write_timeout");}
        unsafe {win(GetOverlappedResult(pipe,&overlap,&mut size,0))?;}
    }
    ensure!(size as usize == packet.len(),"partial_packet_write"); Ok(())
}
fn parent_launch_child(packet:Handle,parent:u32,birth:u64,excluded:HANDLE,scenario:&str)->Result<()> {
    use windows_sys::Win32::System::Console::*;
    let owner=Process::open(parent)?;ensure!(owner.birth==birth,"launch_probe_parent_pid_reused");
    let current=identity::process_identity(unsafe {GetCurrentProcess()})?;identity::same_logon(&owner.identity,&current)?;ensure!(!current.elevated,"launch_probe_must_be_unelevated");
    let mut in_job=0;unsafe {win(windows_sys::Win32::System::JobObjects::IsProcessInJob(GetCurrentProcess(),ptr::null_mut(),&mut in_job))?;}ensure!(in_job!=0,"launch_probe_job_required");
    // The numeric slot can be reused during startup. Signal it if it is an event;
    // the parent checks its retained original event is still unsignaled.
    unsafe {SetEvent(excluded);}
    let input=unsafe {GetStdHandle(STD_INPUT_HANDLE)};let output=unsafe {GetStdHandle(STD_OUTPUT_HANDLE)};let error=unsafe {GetStdHandle(STD_ERROR_HANDLE)};
    ensure!(unsafe {GetFileType(input)}==FILE_TYPE_PIPE && unsafe {GetFileType(output)}==FILE_TYPE_PIPE && unsafe {GetFileType(error)}==FILE_TYPE_CHAR,"launch_probe_stdio_not_whitelisted");
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);let mut available=0;
    loop {unsafe {win(PeekNamedPipe(input,ptr::null_mut(),0,ptr::null_mut(),&mut available,ptr::null_mut()))?;}if available>=4 {break;}ensure!(std::time::Instant::now()<deadline,"launch_probe_stdin_timeout");std::thread::sleep(std::time::Duration::from_millis(10));}
    let mut command=[0;4];let mut size=0;unsafe {win(ReadFile(input,command.as_mut_ptr(),4,&mut size,ptr::null_mut()))?;}ensure!(size==4 && &command==b"PING","launch_probe_control_mismatch");
    unsafe {win(WriteFile(output,b"READY\n".as_ptr(),6,&mut size,ptr::null_mut()))?;}ensure!(size==6,"launch_probe_stdout_partial");
    match scenario {
        "echo"=>{let mut data=vec![0;crate::packet::MAX_PACKET];let size=raw_read(packet.raw(),&mut data)?;crate::packet::validate(&data[..size],1500)?;raw_write(packet.raw(),&data[..size])?;}
        "job-close"=>loop {std::thread::sleep(std::time::Duration::from_millis(100));},
        _=>anyhow::bail!("invalid_parent_launch_probe_scenario"),
    } Ok(())
}
/// Explicit diagnostic uses the exact production launcher; no Wintun/IP Helper calls.
pub async fn engine_launch_self_test(owner:&Process,helper:&std::path::Path)->Result<Value> {
    ensure!(cfg!(feature="privileged-validation"),"validation_build_required");
    use std::{io::Write,os::windows::io::AsRawHandle,time::{Duration,Instant}};
    let mut outcomes=vec![];
    for scenario in ["echo","job-close"] {
        let name=format!(r"\\.\pipe\TermTerm.VpnDev.packet.{}",Uuid::new_v4());
        let server=super::pipe::server(&name,&owner.identity,true)?;let endpoint=super::pipe::packet_endpoint(&name)?;server.connect().await?;
        let mut child=process::parent_launch_probe(owner,helper,endpoint.raw(),scenario).context("parent_launch_probe_spawn")?;drop(endpoint);
        child.input.write_all(b"PING")?;child.input.flush()?;
        let deadline=Instant::now()+Duration::from_secs(5);let mut available=0;
        loop {unsafe {win(PeekNamedPipe(child.output.as_raw_handle(),ptr::null_mut(),0,ptr::null_mut(),&mut available,ptr::null_mut()))?;}if available>=6 {break;}ensure!(Instant::now()<deadline,"parent_launch_probe_ready_timeout");tokio::time::sleep(Duration::from_millis(10)).await;}
        let mut ready=[0;6];use std::io::Read;child.output.read_exact(&mut ready)?;ensure!(&ready==b"READY\n","parent_launch_probe_ready_invalid");child.verify_probe_exclusion()?;
        if scenario=="echo" {
            let mut packet=vec![0;20];packet[0]=0x45;packet[3]=20;
            tokio::time::timeout(Duration::from_secs(5),super::pipe::write_packet(&server,&packet,1500)).await??;
            let echo=tokio::time::timeout(Duration::from_secs(5),super::pipe::read_packet(&server,1500)).await??;ensure!(echo==packet,"parent_launch_packet_mismatch");
            let handle=child.process.raw() as usize;
            ensure!(tokio::task::spawn_blocking(move||unsafe {WaitForSingleObject(handle as HANDLE,5000)}).await?==WAIT_OBJECT_0,"parent_launch_child_exit_timeout");
            let mut code=0;unsafe {win(GetExitCodeProcess(child.process.raw(),&mut code))?;}ensure!(code==0,"parent_launch_child_failed: {code}");
        } else {
            let process=process::duplicate(child.process.raw(),false)?;drop(child);
            let handle=process.raw() as usize;ensure!(tokio::task::spawn_blocking(move||unsafe {WaitForSingleObject(handle as HANDLE,5000)}).await?==WAIT_OBJECT_0,"parent_launch_job_close_did_not_terminate");
        }
        outcomes.push(json!({"scenario":scenario,"ok":true}));
    }
    Ok(json!({"ok":true,"readOnlyHostNetwork":true,"createdAdapters":0,"unelevatedOwnerIdentityVerified":true,"explicitHandleListVerified":true,"jobBeforeResumeVerified":true,"probes":outcomes}))
}
fn packet_probe(packet: Handle, parent: u32, birth: u64, scenario: &str) -> Result<()> {
    let owner = Process::open(parent)?; let current = identity::process_identity(unsafe {GetCurrentProcess()})?;
    ensure!(!current.elevated && owner.birth == birth,"unelevated_probe_owner_required"); identity::same_logon(&owner.identity,&current)?;
    let mut server = 0; unsafe {win(GetNamedPipeServerProcessId(packet.raw(),&mut server))?;} owner.verify(server)?;
    let mut buffer = vec![0;crate::packet::MAX_PACKET];
    match scenario {
        "echo-cancel" => {
            let size = raw_read(packet.raw(),&mut buffer)?; crate::packet::validate(&buffer[..size],1500)?; raw_write(packet.raw(),&buffer[..size])?;
            let event = Handle::new(unsafe {CreateEventW(ptr::null(),1,0,ptr::null())})?;
            let mut overlap: OVERLAPPED = unsafe {zeroed()}; overlap.hEvent = event.raw(); let mut size = 0;
            ensure!(unsafe {ReadFile(packet.raw(),buffer.as_mut_ptr(),buffer.len() as u32,&mut size,&mut overlap)} == 0 && unsafe {GetLastError()} == ERROR_IO_PENDING,"pending_read_required");
            unsafe {win(CancelIoEx(packet.raw(),&overlap))?;}
            ensure!(unsafe {GetOverlappedResult(packet.raw(),&overlap,&mut size,1)} == 0 && unsafe {GetLastError()} == ERROR_OPERATION_ABORTED,"packet_cancel_not_observed");
        }
        "oversize" => { ensure!(raw_read(packet.raw(),&mut buffer).is_err(),"oversized_frame_accepted"); }
        "eof" => { ensure!(raw_read(packet.raw(),&mut buffer).is_err(),"closed_packet_pipe_accepted"); }
        _ => anyhow::bail!("unknown_probe_scenario"),
    }
    Ok(())
}

unsafe fn ip(socket: SOCKADDR_INET) -> Result<IpAddr> {
    match socket.si_family { AF_INET => Ok(Ipv4Addr::from(socket.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes()).into()), AF_INET6 => Ok(std::net::Ipv6Addr::from(socket.Ipv6.sin6_addr.u.Byte).into()), _ => anyhow::bail!("unknown_address_family") }
}
fn checked_os_prefix(address: IpAddr, prefix: u8) -> Result<Cidr> {
    ensure!(prefix <= if address.is_ipv4() {32} else {128},"invalid_os_network_prefix");
    Ok(Cidr {address,prefix})
}
#[derive(PartialEq,Eq)]
struct Snapshot { rows: BTreeSet<String>, specific_routes: Vec<Cidr>, addresses: Vec<IpAddr> }
fn virtual_default_unsupported(oper_up:bool,flags:u8)->bool {oper_up && flags & 1 == 0}
pub fn check_host_plan(plan: &Plan, owned_luids: &[u64]) -> Result<()> {
    unsafe {
        let mut addresses=ptr::null_mut();super::status(GetUnicastIpAddressTable(AF_UNSPEC,&mut addresses))?;
        let connected=(||->Result<Vec<_>> {
            let mut connected=vec![];
            for row in std::slice::from_raw_parts((*addresses).Table.as_ptr(),(*addresses).NumEntries as usize) {
                if owned_luids.contains(&row.InterfaceLuid.Value) {continue;}
                let existing=checked_os_prefix(ip(row.Address)?,row.OnLinkPrefixLength)?.network();
                ensure!(!plan.host_route_conflicts(&existing,true),"address_conflicts_with_existing_host_network");
                connected.push((row.InterfaceLuid.Value,existing));
            }
            Ok(connected)
        })();FreeMibTable(addresses.cast());let connected=connected?;
        let mut table = ptr::null_mut();super::status(GetIpForwardTable2(AF_UNSPEC,&mut table))?;
        let result = (|| -> Result<()> {
            for row in std::slice::from_raw_parts((*table).Table.as_ptr(),(*table).NumEntries as usize) {
                if owned_luids.contains(&row.InterfaceLuid.Value) {continue;}
                if row.DestinationPrefix.PrefixLength == 0 {
                    if plan.full_tunnel && ip(row.DestinationPrefix.Prefix)?.is_ipv4() {
                        let mut interface=MIB_IF_ROW2 {InterfaceLuid:row.InterfaceLuid,..Default::default()};
                        super::status(GetIfEntry2(&mut interface))?;
                        // A virtual default can be another VPN; do not hijack it or guess by its name.
                        ensure!(!virtual_default_unsupported(interface.OperStatus==1,interface.InterfaceAndOperStatusFlags._bitfield),"virtual_default_route_unsupported");
                    }
                    continue;
                }
                let existing = checked_os_prefix(ip(row.DestinationPrefix.Prefix)?,row.DestinationPrefix.PrefixLength)?;
                let on_link=ip(row.NextHop)?.is_unspecified() && connected.iter().any(|(luid,prefix)|*luid==row.InterfaceLuid.Value && existing.prefix>=prefix.prefix && existing.overlaps(prefix));
                ensure!(!plan.host_route_conflicts(&existing,on_link),"route_conflicts_with_existing_host_network");
            } Ok(())
        })();FreeMibTable(table.cast());result
    }
}
impl Snapshot {
    fn read() -> Result<Self> {
        let mut snapshot = Self {rows:BTreeSet::new(),specific_routes:vec![],addresses:vec![]};
        unsafe {
            let mut table = ptr::null_mut(); super::status(GetIpForwardTable2(AF_UNSPEC,&mut table))?;
            for row in std::slice::from_raw_parts((*table).Table.as_ptr(),(*table).NumEntries as usize) {
                let address = ip(row.DestinationPrefix.Prefix)?; let prefix = row.DestinationPrefix.PrefixLength;
                snapshot.rows.insert(format!("route:{}:{address}/{prefix}:{}:{}:{}",row.InterfaceLuid.Value,ip(row.NextHop)?,row.Metric,row.Protocol));
                if prefix > 0 {snapshot.specific_routes.push(checked_os_prefix(address,prefix)?);}
            }
            FreeMibTable(table.cast());
            let mut table = ptr::null_mut(); super::status(GetUnicastIpAddressTable(AF_UNSPEC,&mut table))?;
            for row in std::slice::from_raw_parts((*table).Table.as_ptr(),(*table).NumEntries as usize) {
                let address = ip(row.Address)?; snapshot.addresses.push(address);
                snapshot.rows.insert(format!("address:{}:{address}/{}:{}:{}",row.InterfaceLuid.Value,row.OnLinkPrefixLength,row.PrefixOrigin,row.SuffixOrigin));
            }
            FreeMibTable(table.cast());
        }
        Ok(snapshot)
    }
    fn unused(&self, address: IpAddr) -> Result<()> {
        ensure!(!self.addresses.contains(&address),"self_test_address_in_use");
        let host=Cidr {address,prefix:if address.is_ipv4() {32} else {128}};
        ensure!(!self.specific_routes.iter().any(|route|host.overlaps(route)),"self_test_range_already_routed"); Ok(())
    }
}
fn self_test_routes(n:u8,nested:bool)->Result<Vec<Cidr>> {
    if nested { ["198.51.100.0/24","198.51.100.0/27","198.51.100.0/28","198.51.100.0/29","198.51.100.0/30","198.51.100.0/31","198.51.100.0/32","198.51.100.1/32"].into_iter().map(Cidr::parse).collect() }
    else {Ok(vec![Cidr::parse(&format!("198.51.100.{n}/32"))?])}
}
fn trace_owned_routes(luid:u64,stage:&str)->Result<()> {
    unsafe {
        let mut table=ptr::null_mut();super::status(GetIpForwardTable2(AF_INET,&mut table))?;
        let rows=std::slice::from_raw_parts((*table).Table.as_ptr(),(*table).NumEntries as usize).iter().filter(|row|row.InterfaceLuid.Value==luid).map(super::network::route_row_text).collect::<Vec<_>>();
        FreeMibTable(table.cast());ensure!(rows.len()<=128,"diagnostic_owned_route_table_too_large");
        println!("{}",json!({"routeTrace":true,"luid":luid.to_string(),"stage":stage,"rows":rows}));Ok(())
    }
}
struct RouteUndoTrace<'a> {network:&'a mut OwnedNetwork,failed:bool,verified:BTreeSet<Cidr>}
impl Network for RouteUndoTrace<'_> {
    fn prepare(&self,object:&Object)->Result<()> {self.network.prepare(object)}
    fn apply(&mut self,object:&Object)->Result<()> {self.network.apply(object)}
    fn undo(&mut self,object:&Object)->Result<()> {
        if let Object::Route {guid,luid,destination,metric}=object {
            let proof=self.network.route_undo_readback(*guid,*luid,destination,*metric).and_then(|present|trace_owned_routes(*luid,&format!("before-owned-or-absent-undo:{}",destination.text())).map(|_|present));
            if proof.is_err() {self.failed=true;}let present=proof?;
            self.verified.insert(destination.clone());
            println!("{}",json!({"routeUndoProof":true,"destination":destination.text(),"state":if present {"owned-netmgmt"} else {"absent"}}));
        }
        self.network.undo(object)?;
        if let Object::Route {luid,destination,..}=object {
            let proof=trace_owned_routes(*luid,&format!("after-undo:{}",destination.text()));if proof.is_err() {self.failed=true;}proof?;
        }
        Ok(())
    }
}
fn route_trace_result(journal:&Journal,expected:&BTreeSet<Cidr>,verified:&BTreeSet<Cidr>,failed:bool,outcome:Result<()>)->Result<()> {
    let only_adapter_pending=journal.entries.iter().all(|entry|entry.undone || matches!(entry.object,Object::Adapter {..}));
    if failed || expected!=verified || !only_adapter_pending {
        return outcome.and_then(|_|anyhow::bail!("diagnostic_route_undo_trace_incomplete"));
    }
    // Existing normal cleanup handles a closed adapter's verified phantom after every route undo.
    Ok(())
}
async fn privileged_self_test(fault: Option<&str>,coexistence:bool,nested:bool) -> Result<Value> {
    ensure!(identity::process_identity(unsafe {GetCurrentProcess()})?.elevated,"privileged_self_test_requires_explicit_elevation");
    let lock = super::store::machine_lock().context("self_test_machine_lock")?; let root = ProtectedRoot::open().context("self_test_protected_root")?;
    ensure!(root.journals()?.is_empty(),"recover_stale_journals_before_self_test");
    let baseline = Snapshot::read().context("self_test_snapshot_before")?;
    let last=if nested {241} else {243};
    for n in 241..=last {
        baseline.unused(Ipv4Addr::new(192,0,2,n).into())?;baseline.unused(Ipv4Addr::new(198,51,100,n).into())?;
        let plan = Plan {addresses:vec![Assignment {cidr:Cidr::parse(&format!("192.0.2.{n}/32"))?,gateway:None}],routes:self_test_routes(n,nested)?,mtu:1400,dns_servers:vec![],search_domain:String::new(),excluded_routes:vec![],remote:None,full_tunnel:false};
        check_host_plan(&plan,&[]).context("self_test_host_network_preflight")?;
    }
    let helper = std::env::current_exe()?.canonicalize()?; let watchdog = process::watchdog(&helper,&lock).context("self_test_watchdog_startup")?;
    let api = Wintun::load(&helper.parent().unwrap().join("wintun.dll")).context("self_test_wintun_load")?; let mut network = OwnedNetwork::new(api); runtime::recover(&root,&mut network).context("self_test_journal_recovery")?;
    let mut registry = Registry::default(); let mut sessions = vec![];
    let mut retained=if coexistence {
        let journal=Journal::new(Uuid::new_v4(),1)?;let store=root.store(&journal);
        Some((journal,store,None,Uuid::new_v4(),0_u64))
    } else {None};
    let result = (|| -> Result<()> {
        if let Some((journal,store,tun,guid,luid))=&mut retained {
            journal.apply(Object::Adapter{guid:*guid,name:format!("TermTerm VPN {}",guid.simple())},store,&mut network)?;
            let adapter=network.adapters.get(guid).unwrap();*luid=adapter.luid;*tun=Some(adapter.start(1400)?);
        }
        for n in 241..=last {
            let id = Uuid::new_v4(); let guid = Uuid::new_v4(); let address = Cidr::parse(&format!("192.0.2.{n}/32"))?;
            let plan = Plan {addresses:vec![Assignment {cidr:address.clone(),gateway:None}],routes:self_test_routes(n,nested)?,mtu:1400,dns_servers:vec![],search_domain:String::new(),excluded_routes:vec![],remote:None,full_tunnel:false}; registry.reserve(id,1,&plan)?;
            let journal = Journal::new(id,1)?; let store = root.store(&journal); sessions.push((journal,store)); let (journal,store) = sessions.last_mut().unwrap();
            journal.apply(Object::Adapter {guid,name:format!("TermTerm VPN {}",guid.simple())},store,&mut network).context("self_test_create_adapter")?;
            if fault==Some("adapter-created") {
                println!("{}",json!({"armed":true,"fault":"adapter-created","watchdogPid":unsafe {GetProcessId(watchdog.raw())},"sessions":[journal.session_id],"adapterGuid":guid,"adapterOnlyIntent":journal.entries.len()==1}));
                std::process::exit(92);
            }
            let adapter = network.adapters.get(&guid).unwrap().clone(); let luid = adapter.luid;
            journal.apply(Object::Address {guid,luid,cidr:address},store,&mut network).context("self_test_assign_address")?;
            let before = OwnedNetwork::mtu(guid,luid,false).context("self_test_read_mtu")?; journal.apply(Object::Mtu {guid,luid,ipv6:false,before,after:1400},store,&mut network).context("self_test_set_mtu")?;
            if nested {trace_owned_routes(luid,"before-routes")?;}
            for destination in super::network::route_apply_order(&plan.routes) {
                let stage=format!("after-attempt:{}",destination.text());
                let applied=journal.apply(Object::Route {guid,luid,destination:destination.clone(),metric:5},store,&mut network);
                if nested {trace_owned_routes(luid,&stage)?;}
                applied.context("self_test_add_route")?;
                if nested {network.verify_route(guid,luid,destination,5,None).context("self_test_verify_new_route_netmgmt")?;}
            }
            if nested {
                for destination in &plan.routes {network.verify_route(guid,luid,destination,5,Some(journal)).with_context(||format!("self_test_verify_route_after_all_adds: {}",destination.text()))?;}
                trace_owned_routes(luid,"all-owned-routes-verified")?;
            }
            let tun = adapter.start(1400).context("self_test_start_packet_session")?;
            // Opening and exchanging packets requires the separate kernel dataplane test.
            drop(tun); drop(adapter);
            if let Some((_,_,_,guid,luid))=&retained {
                super::wintun::verify_recovery_interface(*guid,*luid,&format!("TermTerm VPN {}",guid.simple()))?;
                let instance=format!("SWD\\WINTUN\\{{{guid}}}");
                ensure!(super::orphan::dll_sweep_inventory()?.iter().any(|v|v.instance.eq_ignore_ascii_case(&instance) && v.safe),"retained_healthy_adapter_changed");
            }
        }
        Ok(())
    })();
    if fault==Some("broker-death") && result.is_ok() {println!("{}",json!({"armed":true,"fault":"broker-death","watchdogPid":unsafe {GetProcessId(watchdog.raw())},"sessions":sessions.iter().map(|(j,_)|j.session_id).collect::<Vec<_>>(),"retainedHealthyAdapterTested":coexistence,"ownedJournalCount":root.journal_count()? })); std::process::exit(92);}
    let mut cleanup_error = None;
    for (journal,store) in sessions.iter_mut().rev() {
        if nested && result.is_ok() {
            let expected=journal.entries.iter().filter_map(|entry|match &entry.object {Object::Route {destination,..} if !entry.undone=>Some(destination.clone()),_=>None}).collect();
            let mut traced=RouteUndoTrace {network:&mut network,failed:false,verified:BTreeSet::new()};let outcome=journal.cleanup(store,&mut traced);
            if let Err(error)=route_trace_result(journal,&expected,&traced.verified,traced.failed,outcome) {cleanup_error=Some(error);}
        }
        if let Err(e) = runtime::cleanup_session_journal(journal,store,&mut network).and_then(|_|store.remove(journal)).and_then(|_|registry.release_after_cleanup(journal.session_id,journal.generation)) {cleanup_error = Some(e);}
    }
    if let Some((journal,store,tun,guid,luid))=&mut retained {
        if result.is_ok() {if let Err(error)=super::wintun::verify_recovery_interface(*guid,*luid,&format!("TermTerm VPN {}",guid.simple())) {cleanup_error=Some(error);}}
        drop(tun.take());
        if let Err(error)=runtime::cleanup_session_journal(journal,store,&mut network).and_then(|_|store.remove(journal)) {cleanup_error=Some(error);}
    }
    if let Err(error) = result {if let Some(cleanup) = cleanup_error {return Err(error.context(format!("self_test_cleanup_also_failed: {cleanup:#}")));}return Err(error);}
    if let Some(error) = cleanup_error {return Err(error.context("self_test_cleanup"));}
    ensure!(Snapshot::read()? == baseline,"host_network_baseline_changed");
    ensure!(root.journals()?.is_empty(),"journal_cleanup_incomplete");
    ensure!(unsafe {WaitForSingleObject(watchdog.raw(),0)} == WAIT_TIMEOUT,"watchdog_exited_early");
    Ok(json!({"ok":true,"connections":if nested {1} else {3},"nestedRoutes":nested,"allOwnedRoutesVerified":nested,"ownedOrAbsentRouteUndoProofs":nested,"exactRoutes":true,"addressRouteBaselineRestored":true,"watchdogPid":unsafe {GetProcessId(watchdog.raw())},"wintunDriverMayRemainInstalled":true,"retainedHealthyAdapterTested":coexistence,"independentThirdPartyTrafficTested":false,"kernelPacketExchangeTested":false,"authenticationTested":false}))
}
async fn validation_client() -> Result<()> {
    use tokio::io::{AsyncReadExt,AsyncWriteExt};
    use std::io::Write;
    let helper = std::env::current_exe()?.canonicalize()?;
    let client = super::connect_inner(&helper,true).await?;
    let (mut reader,mut writer) = tokio::io::split(client);
    let (input,mut frames) = tokio::sync::mpsc::channel::<Result<zeroize::Zeroizing<Vec<u8>>>>(16);
    std::thread::spawn(move || {
        let stdin = std::io::stdin(); let mut stdin = stdin.lock();
        loop {
            let result = crate::protocol::read_sync(&mut stdin).and_then(|value| {
                let mut bytes = zeroize::Zeroizing::new(serde_json::to_vec(&value)?);
                ensure!(bytes.len() < crate::CONTROL_LIMIT,"control_message_too_large");bytes.push(b'\n');Ok(bytes)
            });
            let closed = result.is_err();if input.blocking_send(result).is_err() || closed {break;}
        }
    });
    let mut output = vec![0;65536];
    loop {tokio::select! {
        frame = frames.recv() => {let Some(Ok(frame)) = frame else {writer.shutdown().await?;return Ok(());};writer.write_all(&frame).await?;}
        result = reader.read(&mut output) => {let size = result?;if size == 0 {return Ok(());}let mut stdout = std::io::stdout().lock();stdout.write_all(&output[..size])?;stdout.flush()?;}
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn route_undo_trace_cannot_claim_success_after_untraced_undo_or_save_fallback() {
        use crate::transaction::Store;
        struct Disk {fail:bool}impl Store for Disk {fn save(&mut self,_:&Journal)->Result<()> {ensure!(!self.fail,"injected_save_error");Ok(())}}
        struct Fake {fail:bool}impl Network for Fake {
            fn prepare(&self,_:&Object)->Result<()> {Ok(())}fn apply(&mut self,_:&Object)->Result<()> {Ok(())}
            fn undo(&mut self,_:&Object)->Result<()> {ensure!(!self.fail,"injected_undo_error");Ok(())}
        }
        let destination=Cidr::parse("198.51.100.1/32").unwrap();let expected=[destination.clone()].into_iter().collect();
        for save_failure in [false,true] {
            let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();let mut store=Disk {fail:false};let mut network=Fake {fail:false};
            journal.apply(Object::Route {guid:Uuid::nil(),luid:17,destination:destination.clone(),metric:5},&mut store,&mut network).unwrap();
            store.fail=save_failure;network.fail=!save_failure;
            let traced=journal.cleanup(&mut store,&mut network);let evidence=route_trace_result(&journal,&expected,&expected,false,traced);
            assert!(evidence.is_err());store.fail=false;network.fail=false;journal.cleanup(&mut store,&mut network).unwrap();assert!(journal.complete());assert!(evidence.is_err(),"successful fallback must not rewrite failed trace evidence");
            assert!(route_trace_result(&journal,&expected,&BTreeSet::new(),false,Ok(())).is_err());
        }
    }
    #[test]
    fn nested_route_diagnostic_keeps_the_failure_order_and_documentation_ranges() {
        let routes=self_test_routes(241,true).unwrap();
        assert_eq!(routes.iter().map(Cidr::text).collect::<Vec<_>>(),["198.51.100.0/24","198.51.100.0/27","198.51.100.0/28","198.51.100.0/29","198.51.100.0/30","198.51.100.0/31","198.51.100.0/32","198.51.100.1/32"]);
        for route in &routes {assert_eq!(route,&route.network());assert!(Cidr::parse("198.51.100.0/24").unwrap().overlaps(route));}
        assert_eq!(self_test_routes(241,false).unwrap(),vec![Cidr::parse("198.51.100.241/32").unwrap()]);
    }
    #[test]
    fn active_virtual_default_routes_fail_closed_without_guessing_adapter_names() {
        assert!(virtual_default_unsupported(true,0));assert!(virtual_default_unsupported(true,0xfe));
        assert!(!virtual_default_unsupported(true,1));assert!(!virtual_default_unsupported(true,0xff));
        assert!(!virtual_default_unsupported(false,0));
    }
    #[test]
    fn actual_host_loopback_overlap_rejects_without_loading_wintun() {
        let before=Snapshot::read().unwrap();
        let plan=Plan {addresses:vec![Assignment {cidr:Cidr::parse("192.0.2.241/32").unwrap(),gateway:None}],routes:vec![Cidr::parse("127.0.0.1/32").unwrap()],mtu:1400,dns_servers:vec![],search_domain:String::new(),excluded_routes:vec![],remote:None,full_tunnel:false};
        assert!(check_host_plan(&plan,&[]).is_err());assert!(Snapshot::read().unwrap() == before);
    }
    #[test]
    fn ipv6_host_routes_do_not_make_ipv4_self_test_preflight_panic() {
        for route in ["fe80::/64","fe80::1/128"] {
            let snapshot=Snapshot {rows:BTreeSet::new(),specific_routes:vec![Cidr::parse(route).unwrap()],addresses:vec![]};
            snapshot.unused("192.0.2.241".parse().unwrap()).unwrap();
        }
        let snapshot=Snapshot {rows:BTreeSet::new(),specific_routes:vec![Cidr::parse("192.0.2.0/24").unwrap()],addresses:vec![]};
        snapshot.unused("fd12::1".parse().unwrap()).unwrap();
        assert!(checked_os_prefix("192.0.2.1".parse().unwrap(),128).is_err());
        assert!(checked_os_prefix("fd12::1".parse().unwrap(),129).is_err());
    }
}
