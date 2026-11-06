use super::{status, wintun::{lookup_guid, to_guid, Adapter, Wintun}};
use crate::{policy::Cidr, transaction::{DnsSettings, Metric, Network, Object, restore_needed}};
use anyhow::{ensure, Context, Result};
use std::{collections::HashMap, net::{IpAddr, Ipv4Addr, Ipv6Addr}, sync::Arc};
use uuid::Uuid;
use windows_sys::Win32::{Foundation::*, NetworkManagement::{IpHelper::*, Ndis::NET_LUID_LH}, Networking::WinSock::*};

pub struct OwnedNetwork { api: Option<Arc<Wintun>>, pub adapters: HashMap<Uuid, Arc<Adapter>>, closed_owned:HashMap<Uuid,u64>, recovery_identities:HashMap<Uuid,super::orphan::RecoveryIdentity>, #[cfg(feature="privileged-validation")] pub(super) diagnostics:Option<Arc<super::diagnostics::Diagnostics>> }
#[derive(Debug)]
pub(super) struct AdapterUnregisterPending;
impl std::fmt::Display for AdapterUnregisterPending {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("windows_interface_unregister_pending")}
}
impl std::error::Error for AdapterUnregisterPending {}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct PeerPath {pub luid:u64,prefix:Cidr,next_hop:IpAddr,metric:u32,source:IpAddr}
pub fn peer_path(peer:IpAddr)->Result<PeerPath> {
    ensure!(peer.is_ipv4(),"ipv4_peer_required");
    let mut row=MIB_IPFORWARD_ROW2::default();let mut source=SOCKADDR_INET::default();
    status(unsafe {GetBestRoute2(std::ptr::null(),0,std::ptr::null(),&sockaddr(peer),0,&mut row,&mut source)}).context("GetBestRoute2_peer")?;
    let ip=|value:SOCKADDR_INET|->Result<IpAddr> {ensure!(unsafe {value.si_family}==AF_INET,"ipv4_peer_route_required");Ok(Ipv4Addr::from(unsafe {value.Ipv4.sin_addr.S_un.S_addr}.to_ne_bytes()).into())};
    Ok(PeerPath {luid:unsafe {row.InterfaceLuid.Value},prefix:Cidr {address:ip(row.DestinationPrefix.Prefix)?,prefix:row.DestinationPrefix.PrefixLength},next_hop:ip(row.NextHop)?,metric:row.Metric,source:ip(source)?})
}
pub fn verify_peer_path(peer:IpAddr,expected:&PeerPath)->Result<()> {ensure!(&peer_path(peer)?==expected,"vpn_peer_underlay_changed");Ok(())}
pub fn verify_dns_routes(servers:&[IpAddr],owned_luid:u64)->Result<()> {
    for server in servers {ensure!(peer_path(*server)?.luid==owned_luid,"dns_route_does_not_use_owned_vpn");}Ok(())
}
impl OwnedNetwork {
    pub fn new(api: Arc<Wintun>) -> Self { Self { api:Some(api), adapters: HashMap::new(),closed_owned:HashMap::new(),recovery_identities:HashMap::new(), #[cfg(feature="privileged-validation")] diagnostics:None } }
    pub fn recovering() -> Self {Self {api:None,adapters:HashMap::new(),closed_owned:HashMap::new(),recovery_identities:HashMap::new(), #[cfg(feature="privileged-validation")] diagnostics:None}}
    pub(super) fn recovery_identities(&mut self,proofs:Vec<super::orphan::RecoveryIdentity>)->Result<()> {
        ensure!(self.api.is_none() && self.adapters.is_empty(),"inactive_recovery_requires_recovery_only_network");
        self.recovery_identities=proofs.into_iter().map(|proof|(proof.guid(),proof)).collect();Ok(())
    }
    pub fn closed_adapter(&self,guid:Uuid,luid:Option<u64>)->bool {self.closed_owned.get(&guid).is_some_and(|actual|luid.is_none_or(|expected|expected==*actual)) && !self.adapters.contains_key(&guid)}
    pub fn forget_closed_adapter(&mut self,guid:Uuid) {self.closed_owned.remove(&guid);}
    pub fn verify_interface(guid: Uuid, luid: u64) -> Result<bool> {
        let Some(actual_luid) = lookup_guid(guid)? else {return Ok(false);};
        ensure!(actual_luid == luid, "interface_identity_changed"); Ok(true)
    }
    fn require_owned(&self, guid: Uuid, luid: u64) -> Result<()> {
        ensure!(self.adapters.get(&guid).is_some_and(|a| a.luid == luid), "interface_not_owned_by_this_process");
        ensure!(Self::verify_interface(guid, luid)?, "owned_interface_disappeared"); Ok(())
    }
    pub fn mtu(guid: Uuid, luid: u64, ipv6: bool) -> Result<u32> {
        ensure!(Self::verify_interface(guid, luid)?, "owned_interface_disappeared");
        let mut row = interface(luid, ipv6); status(unsafe { GetIpInterfaceEntry(&mut row) })?; Ok(row.NlMtu)
    }
    pub fn dns(guid:Uuid,luid:u64)->Result<DnsSettings> {
        ensure!(Self::verify_interface(guid,luid)?,"owned_interface_disappeared");
        let mut raw=DNS_INTERFACE_SETTINGS {Version:DNS_INTERFACE_SETTINGS_VERSION1,..Default::default()};
        status(unsafe {GetInterfaceDnsSettings(to_guid(guid),&mut raw)}).context("GetInterfaceDnsSettings")?;
        let result=(|| {
            let names=unsafe {dns_text(raw.NameServer)}?;
            let servers=names.split(|c:char|c==',' || c.is_ascii_whitespace()).filter(|s|!s.is_empty()).map(|s|s.parse::<IpAddr>()).collect::<std::result::Result<Vec<_>,_>>().map_err(|_|anyhow::anyhow!("invalid_interface_dns_server"))?;
            ensure!(servers.len()<=8 && servers.iter().all(IpAddr::is_ipv4),"unsupported_interface_dns");
            let search=unsafe {dns_text(raw.SearchList)}?;
            let search_list=search.split(|c:char|c==',' || c.is_ascii_whitespace()).filter(|s|!s.is_empty()).map(crate::policy::search_domain).collect::<Result<Vec<_>>>()?;
            ensure!(search_list.len()<=8,"unsupported_interface_dns_search_list");
            Ok(DnsSettings {servers,search_domain:crate::policy::search_domain(&unsafe {dns_text(raw.Domain)}?)?,search_list})
        })();
        unsafe {FreeInterfaceDnsSettings(&mut raw)};result
    }
    pub fn metric(guid:Uuid,luid:u64)->Result<Metric> {
        ensure!(Self::verify_interface(guid,luid)?,"owned_interface_disappeared");
        let mut row=interface(luid,false);status(unsafe {GetIpInterfaceEntry(&mut row)})?;
        Ok(Metric {automatic:row.UseAutomaticMetric,value:row.Metric})
    }
    pub fn address_ready(&self,guid:Uuid,luid:u64,cidr:&Cidr)->Result<bool> {
        self.require_owned(guid,luid)?;
        let mut row=address(luid,cidr);status(unsafe {GetUnicastIpAddressEntry(&mut row)}).context("GetUnicastIpAddressEntry_readiness")?;
        ensure!(unsafe {row.InterfaceLuid.Value}==luid && row.OnLinkPrefixLength==cidr.prefix && row.PrefixOrigin==IpPrefixOriginManual && row.SuffixOrigin==IpSuffixOriginManual && !row.SkipAsSource,"owned_address_changed_before_ready");
        address_dad_ready(row.DadState)
    }
    pub(crate) fn verify_route(&self,guid:Uuid,luid:u64,destination:&Cidr,metric:u32,journal:Option<&crate::transaction::Journal>)->Result<()> {
        self.require_owned(guid,luid)?;
        let mut row=route(luid,destination,metric);status(unsafe {GetIpForwardEntry2(&mut row)}).context("GetIpForwardEntry2_owned_readback")?;
        require_diagnostic_route(&row,guid,luid,destination,metric,journal)
    }
    pub(crate) fn route_undo_readback(&self,guid:Uuid,luid:u64,destination:&Cidr,metric:u32)->Result<bool> {
        self.require_owned(guid,luid)?;
        let mut row=route(luid,destination,metric);let code=unsafe {GetIpForwardEntry2(&mut row)};
        diagnostic_undo_readback(code,&row,guid,luid,destination,metric)
    }
}
pub(crate) fn route_apply_order(routes:&[Cidr])->Vec<&Cidr> {
    let mut ordered=routes.iter().collect::<Vec<_>>();
    // Windows synthesizes broadcast /32s for on-link prefixes; install explicit hosts first.
    ordered.sort_by_key(|route|(std::cmp::Reverse(route.prefix),route.address));ordered
}
unsafe fn dns_text(value:*const u16)->Result<String> {
    if value.is_null() {return Ok(String::new());}
    for len in 0..4096 {if *value.add(len)==0 {return String::from_utf16(std::slice::from_raw_parts(value,len)).map_err(|_|anyhow::anyhow!("invalid_interface_dns_text"));}}
    anyhow::bail!("interface_dns_text_too_large")
}
fn dns_write(guid:Uuid,settings:&DnsSettings)->Result<()> {
    let mut servers=super::wide(&settings.servers.iter().map(ToString::to_string).collect::<Vec<_>>().join(","))?;
    let mut domain=super::wide(&settings.search_domain)?;
    let mut search=super::wide(settings.search_list.join(","))?;
    // SearchList is adapter-scoped; Domain alone does not configure its query suffix list.
    let raw=DNS_INTERFACE_SETTINGS {Version:DNS_INTERFACE_SETTINGS_VERSION1,Flags:(DNS_SETTING_NAMESERVER|DNS_SETTING_DOMAIN|DNS_SETTING_SEARCHLIST) as u64,NameServer:servers.as_mut_ptr(),Domain:domain.as_mut_ptr(),SearchList:search.as_mut_ptr(),..Default::default()};
    status(unsafe {SetInterfaceDnsSettings(to_guid(guid),&raw)}).context("SetInterfaceDnsSettings")
}
fn metric_write(luid:u64,value:&Metric)->Result<()> {
    let mut row=interface(luid,false);status(unsafe {GetIpInterfaceEntry(&mut row)})?;
    row.UseAutomaticMetric=value.automatic;row.Metric=value.value;row.SitePrefixLength=0;
    status(unsafe {SetIpInterfaceEntry(&mut row)}).context("SetIpInterfaceEntry_metric")
}
fn address_dad_ready(state:NL_DAD_STATE)->Result<bool> {
    if state==IpDadStatePreferred {return Ok(true);}if state==IpDadStateTentative {return Ok(false);}anyhow::bail!("owned_address_dad_failed: state={state}")
}
fn sockaddr(ip: IpAddr) -> SOCKADDR_INET {
    match ip {
        IpAddr::V4(v) => SOCKADDR_INET { Ipv4: SOCKADDR_IN { sin_family: AF_INET, sin_addr: IN_ADDR { S_un: IN_ADDR_0 { S_addr: u32::from_ne_bytes(v.octets()) } }, ..Default::default() } },
        IpAddr::V6(v) => SOCKADDR_INET { Ipv6: SOCKADDR_IN6 { sin6_family: AF_INET6, sin6_addr: IN6_ADDR { u: IN6_ADDR_0 { Byte: v.octets() } }, ..Default::default() } },
    }
}
fn address(luid: u64, cidr: &Cidr) -> MIB_UNICASTIPADDRESS_ROW {
    let mut row = MIB_UNICASTIPADDRESS_ROW::default(); unsafe { InitializeUnicastIpAddressEntry(&mut row); }
    row.InterfaceLuid = NET_LUID_LH { Value: luid }; row.Address = sockaddr(cidr.address); row.OnLinkPrefixLength = cidr.prefix;
    row.PrefixOrigin = IpPrefixOriginManual; row.SuffixOrigin = IpSuffixOriginManual;
    row.ValidLifetime = u32::MAX; row.PreferredLifetime = u32::MAX; row.SkipAsSource = false; row
}
fn route(luid: u64, cidr: &Cidr, metric: u32) -> MIB_IPFORWARD_ROW2 {
    let mut row = MIB_IPFORWARD_ROW2::default(); unsafe { InitializeIpForwardEntry(&mut row); }
    row.InterfaceLuid = NET_LUID_LH { Value: luid };
    row.DestinationPrefix = IP_ADDRESS_PREFIX { Prefix: sockaddr(cidr.address), PrefixLength: cidr.prefix };
    row.NextHop = sockaddr(if cidr.address.is_ipv4() { Ipv4Addr::UNSPECIFIED.into() } else { Ipv6Addr::UNSPECIFIED.into() });
    row.Metric = metric; row.Protocol = MIB_IPPROTO_NETMGMT; row.ValidLifetime = u32::MAX; row.PreferredLifetime = u32::MAX;
    row.SitePrefixLength=cidr.prefix;row.Loopback=false;row.AutoconfigureAddress=false;row.Publish=false;row.Immortal=true;row
}
fn interface(luid: u64, ipv6: bool) -> MIB_IPINTERFACE_ROW {
    let mut row = MIB_IPINTERFACE_ROW::default(); unsafe { InitializeIpInterfaceEntry(&mut row); }
    row.InterfaceLuid = NET_LUID_LH { Value: luid }; row.Family = if ipv6 { AF_INET6 } else { AF_INET }; row
}
fn mtu_update(mut row: MIB_IPINTERFACE_ROW, mtu: u32) -> MIB_IPINTERFACE_ROW {
    row.NlMtu=mtu;
    // SetIpInterfaceEntry requires zero for IPv4 even after GetIpInterfaceEntry.
    if row.Family == AF_INET {row.SitePrefixLength=0;}
    row
}
fn absent(code: u32) -> bool { code == ERROR_NOT_FOUND || code == ERROR_FILE_NOT_FOUND }
pub(crate) fn route_row_text(row:&MIB_IPFORWARD_ROW2)->String {
    let ip=|value:SOCKADDR_INET|unsafe {match value.si_family {
        AF_INET=>Ipv4Addr::from(value.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes()).to_string(),
        AF_INET6=>Ipv6Addr::from(value.Ipv6.sin6_addr.u.Byte).to_string(),
        family=>format!("unknown_family_{family}"),
    }};
    format!("destination={}/{} nextHop={} luid={} index={} protocol={} origin={} metric={} valid={} preferred={} sitePrefix={} loopback={} autoconfigure={} publish={} immortal={} age={}",
        ip(row.DestinationPrefix.Prefix),row.DestinationPrefix.PrefixLength,ip(row.NextHop),unsafe {row.InterfaceLuid.Value},row.InterfaceIndex,row.Protocol,row.Origin,row.Metric,row.ValidLifetime,row.PreferredLifetime,row.SitePrefixLength,row.Loopback,row.AutoconfigureAddress,row.Publish,row.Immortal,row.Age)
}
fn require_route_absent(code:u32,row:&MIB_IPFORWARD_ROW2,destination:&Cidr,luid:u64,stage:&str)->Result<()> {
    if absent(code) {return Ok(());}
    // Failed queries leave input fields in place; only success supplies a returned row.
    let returned=if code==0 {route_row_text(row)} else {"unavailable".into()};
    anyhow::bail!("route_already_exists_or_unqueryable: stage={stage} code={code} destination={} luid={luid} returned=[{returned}]",destination.text())
}
fn require_owned_route(row:&MIB_IPFORWARD_ROW2,destination:&Cidr,metric:u32)->Result<()> {
    ensure!(row.Metric == metric && row.Protocol == MIB_IPPROTO_NETMGMT && row.ValidLifetime == u32::MAX && row.PreferredLifetime == u32::MAX && row.SitePrefixLength == destination.prefix && !row.Loopback && !row.Publish && !row.AutoconfigureAddress && row.Immortal,
        "owned_route_changed: metric={} protocol={} valid={} preferred={} sitePrefix={} loopback={} autoconfigure={} publish={} immortal={}",row.Metric,row.Protocol,row.ValidLifetime,row.PreferredLifetime,row.SitePrefixLength,row.Loopback,row.AutoconfigureAddress,row.Publish,row.Immortal);Ok(())
}
fn require_diagnostic_route(row:&MIB_IPFORWARD_ROW2,guid:Uuid,luid:u64,destination:&Cidr,metric:u32,journal:Option<&crate::transaction::Journal>)->Result<()> {
    let ip=|value:SOCKADDR_INET|unsafe {match value.si_family {AF_INET=>Some(IpAddr::V4(Ipv4Addr::from(value.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes()))),AF_INET6=>Some(IpAddr::V6(Ipv6Addr::from(value.Ipv6.sin6_addr.u.Byte))),_=>None}};
    let next_hop=if destination.address.is_ipv4() {IpAddr::V4(Ipv4Addr::UNSPECIFIED)} else {IpAddr::V6(Ipv6Addr::UNSPECIFIED)};
    ensure!(unsafe {row.InterfaceLuid.Value}==luid && ip(row.DestinationPrefix.Prefix)==Some(destination.address) && row.DestinationPrefix.PrefixLength==destination.prefix && ip(row.NextHop)==Some(next_hop) && row.Origin==NlroManual,"diagnostic_route_identity_or_origin_changed");
    let mut observed=*row;
    if row.Protocol==MIB_IPPROTO_LOCAL {
        let journal=journal.context("diagnostic_local_route_requires_owned_parent")?;
        ensure!(journal.entries.iter().any(|entry|!entry.undone && entry.object==Object::Adapter {guid,name:format!("TermTerm VPN {}",guid.simple())}) && journal.entries.iter().any(|entry|!entry.undone && entry.object==Object::Route {guid,luid,destination:destination.clone(),metric}),"diagnostic_local_route_not_in_owned_journal");
        let parent=destination.prefix==32 && journal.entries.iter().any(|entry|match &entry.object {
            Object::Route {guid:parent_guid,luid:parent_luid,destination:parent,metric:parent_metric} if !entry.undone && *parent_guid==guid && *parent_luid==luid && *parent_metric==metric && parent.prefix>0 && parent.prefix<32 && parent==&parent.network()=>match (destination.address,parent.address) {
                (IpAddr::V4(host),IpAddr::V4(network))=>u32::from(host)==(u32::from(network)|(u32::MAX>>parent.prefix)),_=>false,
            },_=>false,
        });
        ensure!(parent,"diagnostic_local_route_requires_broadcast_endpoint");
        // Only this readback recognizes Windows' temporary broadcast metadata; undo stays strict.
        observed.Protocol=MIB_IPPROTO_NETMGMT;
    }
    require_owned_route(&observed,destination,metric)
}
fn diagnostic_undo_readback(code:u32,row:&MIB_IPFORWARD_ROW2,guid:Uuid,luid:u64,destination:&Cidr,metric:u32)->Result<bool> {
    if absent(code) {return Ok(false);}
    status(code).context("GetIpForwardEntry2_undo_readback")?;
    require_diagnostic_route(row,guid,luid,destination,metric,None)?;Ok(true)
}
fn mtu_cleanup_result(code: u32, verify: impl FnOnce() -> Result<bool>) -> Result<bool> {
    if code == 0 {return Ok(true);}
    // Only documented missing-interface codes plus fresh GUID absence prove an idempotent undo.
    if absent(code) && !verify()? {return Ok(false);}
    status(code)?;Ok(true)
}
fn wait_for_adapter_absence(
    expected_luid:Option<u64>,
    mut observe:impl FnMut()->Result<(Option<u64>,bool)>,
    mut elapsed:impl FnMut()->std::time::Duration,
    mut pause:impl FnMut(),
)->Result<()> {
    loop {
        let (observed_luid,devnode_absent)=observe()?;
        ensure!(expected_luid.is_none() || observed_luid.is_none() || expected_luid==observed_luid,"interface_identity_changed_after_owned_adapter_close");
        if observed_luid.is_none() && devnode_absent {return Ok(());}
        if expected_luid.is_some() && expected_luid==observed_luid && devnode_absent {return Err(AdapterUnregisterPending.into());}
        let elapsed=elapsed();
        ensure!(elapsed<std::time::Duration::from_secs(30),"owned_adapter_still_present_cleanup_required: expectedLuid={expected_luid:?} observedLuid={observed_luid:?} devnodeAbsent={devnode_absent} elapsedMs={}",elapsed.as_millis());
        pause();
    }
}
impl Network for OwnedNetwork {
    fn prepare(&self, object: &Object) -> Result<()> {
        match object {
            Object::Adapter { guid, name } => {
                ensure!(name == &format!("TermTerm VPN {}", guid.simple()), "adapter_name_mismatch");
                ensure!(!self.adapters.contains_key(guid), "adapter_already_owned");
                ensure!(lookup_guid(*guid)?.is_none(), "adapter_guid_already_exists");
                ensure!(super::orphan::devnode_absent(*guid)?, "adapter_devnode_already_exists");
            }
            Object::Address { guid, luid, cidr } => {
                self.require_owned(*guid, *luid)?;
                ensure!(absent(unsafe { GetUnicastIpAddressEntry(&mut address(*luid, cidr)) }), "address_already_exists_or_unqueryable");
            }
            Object::Route { guid, luid, destination, metric } => {
                self.require_owned(*guid, *luid)?;
                let mut row=route(*luid,destination,*metric);let code=unsafe {GetIpForwardEntry2(&mut row)};
                require_route_absent(code,&row,destination,*luid,"prepare")?;
            }
            Object::Mtu { guid, luid, ipv6, before, .. } => {
                self.require_owned(*guid, *luid)?;
                ensure!(Self::mtu(*guid, *luid, *ipv6)? == *before, "interface_mtu_changed");
            }
            Object::Dns {guid,luid,before,..} => {self.require_owned(*guid,*luid)?;ensure!(&Self::dns(*guid,*luid)?==before,"interface_dns_changed");}
            Object::Metric {guid,luid,before,..} => {self.require_owned(*guid,*luid)?;ensure!(&Self::metric(*guid,*luid)?==before,"interface_metric_changed");}
        }
        Ok(())
    }
    fn apply(&mut self, object: &Object) -> Result<()> {
        match object {
            Object::Adapter { guid, name } => {
                ensure!(!self.adapters.contains_key(guid), "adapter_already_owned");
                let adapter = self.api.as_ref().context("recovery_cannot_create_adapters")?.create(*guid, name)?;
                self.adapters.insert(*guid, Arc::new(adapter)); Ok(())
            }
            Object::Address { guid, luid, cidr } => {
                self.require_owned(*guid, *luid)?;
                let mut row = address(*luid, cidr); let code = unsafe { GetUnicastIpAddressEntry(&mut row) };
                ensure!(absent(code), "address_already_exists_or_unqueryable");
                status(unsafe { CreateUnicastIpAddressEntry(&address(*luid, cidr)) }).context("CreateUnicastIpAddressEntry")
            }
            Object::Route { guid, luid, destination, metric } => {
                self.require_owned(*guid, *luid)?;
                let mut row = route(*luid, destination, *metric); let code = unsafe { GetIpForwardEntry2(&mut row) };
                require_route_absent(code,&row,destination,*luid,"apply")?;
                status(unsafe { CreateIpForwardEntry2(&route(*luid, destination, *metric)) }).context("CreateIpForwardEntry2")
            }
            Object::Mtu { guid, luid, ipv6, before, after } => {
                self.require_owned(*guid, *luid)?;
                let mut row = interface(*luid, *ipv6); status(unsafe { GetIpInterfaceEntry(&mut row) }).context("GetIpInterfaceEntry_before_set")?;
                ensure!(row.NlMtu == *before, "interface_mtu_changed");row=mtu_update(row,*after);
                status(unsafe { SetIpInterfaceEntry(&mut row) }).context("SetIpInterfaceEntry")
            }
            Object::Dns {guid,luid,before,after} => {
                self.require_owned(*guid,*luid)?;ensure!(&Self::dns(*guid,*luid)?==before,"interface_dns_changed");
                dns_write(*guid,after)?;ensure!(&Self::dns(*guid,*luid)?==after,"interface_dns_readback_mismatch");Ok(())
            }
            Object::Metric {guid,luid,before,after} => {
                self.require_owned(*guid,*luid)?;ensure!(&Self::metric(*guid,*luid)?==before,"interface_metric_changed");
                metric_write(*luid,after)?;ensure!(&Self::metric(*guid,*luid)?==after,"interface_metric_readback_mismatch");Ok(())
            }
        }
    }
    fn undo(&mut self, object: &Object) -> Result<()> {
        let guid=match object {Object::Adapter {guid,..}|Object::Address {guid,..}|Object::Route {guid,..}|Object::Mtu {guid,..}|Object::Dns {guid,..}|Object::Metric {guid,..}=>guid};
        if let Some(proof)=self.recovery_identities.get(guid) {proof.verify()?;}
        match object {
            Object::Adapter { guid, name } => {
                let expected_luid=self.adapters.get(guid).map(|adapter|adapter.luid).or_else(||self.closed_owned.get(guid).copied()).or_else(||self.recovery_identities.get(guid).map(|proof|proof.luid()));
                let already_closed=self.closed_owned.contains_key(guid) || self.recovery_identities.contains_key(guid);
                #[cfg(feature="privileged-validation")]
                if !already_closed {if let Some(diagnostic)=&self.diagnostics {diagnostic.record("adapter_close_begin",None,Some(*guid),expected_luid,None);}}
                if let Some(adapter) = self.adapters.get_mut(guid) {
                    ensure!(Arc::strong_count(adapter) == 1, "adapter_still_in_use");
                    Arc::get_mut(adapter).context("adapter_still_referenced")?.close().context("owned_adapter_close")?;
                    self.closed_owned.insert(*guid,adapter.luid);
                }
                if let Some(adapter) = self.adapters.remove(guid) {
                    drop(adapter);
                }
                #[cfg(feature="privileged-validation")]
                if !already_closed {if let Some(diagnostic)=&self.diagnostics {diagnostic.record("adapter_handle_closed",None,Some(*guid),expected_luid,None);}}
                let started=std::time::Instant::now();
                // Windows can remove the devnode before its exact registered interface disappears.
                let result=wait_for_adapter_absence(expected_luid,|| {
                    if let Some(luid)=expected_luid {super::wintun::verify_recovery_interface(*guid,luid,name)?;}
                    Ok((lookup_guid(*guid)?,super::orphan::devnode_absent(*guid)?))
                },||if already_closed {std::time::Duration::from_secs(30)} else {started.elapsed()},||std::thread::sleep(std::time::Duration::from_millis(50)))
                    .with_context(||format!("owned_adapter_close guid={guid}"));
                #[cfg(feature="privileged-validation")]
                if !already_closed || !result.as_ref().is_err_and(|error|error.is::<AdapterUnregisterPending>()) {if let Some(diagnostic)=&self.diagnostics {diagnostic.record("adapter_close_finished",None,Some(*guid),expected_luid,result.as_ref().err());}}
                result
            }
            Object::Address { guid, luid, cidr } => {
                if !Self::verify_interface(*guid, *luid)? { return Ok(()); }
                let mut row = address(*luid, cidr); let code = unsafe { GetUnicastIpAddressEntry(&mut row) };
                if absent(code) { return Ok(()); } status(code)?;
                ensure!(row.OnLinkPrefixLength == cidr.prefix && row.PrefixOrigin == IpPrefixOriginManual && row.SuffixOrigin == IpSuffixOriginManual && row.ValidLifetime == u32::MAX && row.PreferredLifetime == u32::MAX && !row.SkipAsSource, "owned_address_changed");
                status(unsafe { DeleteUnicastIpAddressEntry(&row) })
            }
            Object::Route { guid, luid, destination, metric } => {
                if !Self::verify_interface(*guid, *luid)? { return Ok(()); }
                let mut row = route(*luid, destination, *metric); let code = unsafe { GetIpForwardEntry2(&mut row) };
                if absent(code) { return Ok(()); } status(code)?;
                require_owned_route(&row,destination,*metric)?;
                status(unsafe { DeleteIpForwardEntry2(&row) })
            }
            Object::Mtu { guid, luid, ipv6, before, after } => {
                if !Self::verify_interface(*guid, *luid)? { return Ok(()); }
                let mut row = interface(*luid, *ipv6);
                let code=unsafe {GetIpInterfaceEntry(&mut row)};
                if !mtu_cleanup_result(code,||Self::verify_interface(*guid,*luid)).with_context(||format!("GetIpInterfaceEntry_restore guid={guid} luid={luid} ipv6={ipv6} before={before} after={after} code={code}"))? {return Ok(());}
                if row.NlMtu == *before { return Ok(()); }
                ensure!(row.NlMtu == *after, "owned_mtu_changed");row=mtu_update(row,*before);
                let code=unsafe {SetIpInterfaceEntry(&mut row)};
                mtu_cleanup_result(code,||Self::verify_interface(*guid,*luid)).with_context(||format!("SetIpInterfaceEntry_restore guid={guid} luid={luid} ipv6={ipv6} before={before} after={after} code={code}"))?;Ok(())
            }
            Object::Dns {guid,luid,before,after} => {
                if !Self::verify_interface(*guid,*luid)? {return Ok(());}
                if restore_needed(&Self::dns(*guid,*luid)?,before,after)? {dns_write(*guid,before)?;ensure!(&Self::dns(*guid,*luid)?==before,"interface_dns_restore_readback_mismatch");}Ok(())
            }
            Object::Metric {guid,luid,before,after} => {
                if !Self::verify_interface(*guid,*luid)? {return Ok(());}
                if restore_needed(&Self::metric(*guid,*luid)?,before,after)? {metric_write(*luid,before)?;ensure!(&Self::metric(*guid,*luid)?==before,"interface_metric_restore_readback_mismatch");}Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_local_metadata_requires_exact_journal_broadcast_while_undo_stays_strict() {
        use crate::transaction::{Entry,Journal};
        let guid=Uuid::new_v4();let luid=17;let destination=Cidr::parse("198.51.100.1/32").unwrap();let parent=Cidr::parse("198.51.100.0/31").unwrap();
        let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();
        journal.entries=vec![Entry {object:Object::Adapter {guid,name:format!("TermTerm VPN {}",guid.simple())},undone:false},Entry {object:Object::Route {guid,luid,destination:destination.clone(),metric:5},undone:false},Entry {object:Object::Route {guid,luid,destination:parent,metric:5},undone:false}];
        let initial=route(luid,&destination,5);require_diagnostic_route(&initial,guid,luid,&destination,5,None).unwrap();
        let mut local=initial;local.Protocol=MIB_IPPROTO_LOCAL;
        assert!(require_diagnostic_route(&local,guid,luid,&destination,5,None).is_err());
        require_diagnostic_route(&local,guid,luid,&destination,5,Some(&journal)).unwrap();
        assert!(require_owned_route(&local,&destination,5).is_err(),"production undo must still reject LOCAL");
        for removed in 0..journal.entries.len() {let mut changed=journal.clone();changed.entries.remove(removed);assert!(require_diagnostic_route(&local,guid,luid,&destination,5,Some(&changed)).is_err());}
        for index in 0..journal.entries.len() {let mut changed=journal.clone();changed.entries[index].undone=true;assert!(require_diagnostic_route(&local,guid,luid,&destination,5,Some(&changed)).is_err());}
        assert!(require_diagnostic_route(&local,Uuid::new_v4(),luid,&destination,5,Some(&journal)).is_err());
        assert!(require_diagnostic_route(&local,guid,luid+1,&destination,5,Some(&journal)).is_err());
        let mutations:&[fn(&mut MIB_IPFORWARD_ROW2)]=&[
            |r|r.InterfaceLuid=NET_LUID_LH {Value:18},|r|r.DestinationPrefix.Prefix=sockaddr("198.51.100.2".parse().unwrap()),|r|r.DestinationPrefix.PrefixLength=31,
            |r|r.NextHop=sockaddr("192.0.2.1".parse().unwrap()),|r|r.NextHop=sockaddr("::".parse().unwrap()),|r|r.Origin=NlroDHCP,|r|r.Metric=6,|r|r.Protocol=99,
            |r|r.ValidLifetime=1,|r|r.PreferredLifetime=1,|r|r.SitePrefixLength=31,|r|r.Loopback=true,|r|r.AutoconfigureAddress=true,|r|r.Publish=true,|r|r.Immortal=false,
        ];
        for mutate in mutations {let mut changed=local;mutate(&mut changed);assert!(require_diagnostic_route(&changed,guid,luid,&destination,5,Some(&journal)).is_err(),"{}",route_row_text(&changed));}
        for other in [Object::Route {guid:Uuid::new_v4(),luid,destination:Cidr::parse("198.51.100.0/31").unwrap(),metric:5},Object::Route {guid,luid:luid+1,destination:Cidr::parse("198.51.100.0/31").unwrap(),metric:5},Object::Route {guid,luid,destination:Cidr::parse("198.51.100.2/31").unwrap(),metric:5},Object::Route {guid,luid,destination:Cidr::parse("198.51.100.0/31").unwrap(),metric:6}] {
            let mut changed=journal.clone();changed.entries[2].object=other;assert!(require_diagnostic_route(&local,guid,luid,&destination,5,Some(&changed)).is_err());
        }
        let foreign=OwnedNetwork::recovering();assert!(foreign.verify_route(guid,luid,&destination,5,Some(&journal)).unwrap_err().to_string().contains("interface_not_owned_by_this_process"));
    }
    #[test]
    fn diagnostic_undo_accepts_only_strict_owned_rows_or_documented_absence() {
        let guid=Uuid::new_v4();let destination=Cidr::parse("198.51.100.1/32").unwrap();let mut row=route(17,&destination,5);
        assert!(diagnostic_undo_readback(0,&row,guid,17,&destination,5).unwrap());
        row.Protocol=MIB_IPPROTO_LOCAL;assert!(diagnostic_undo_readback(0,&row,guid,17,&destination,5).is_err());
        for code in [ERROR_NOT_FOUND,ERROR_FILE_NOT_FOUND] {assert!(!diagnostic_undo_readback(code,&row,guid,17,&destination,5).unwrap());}
        for code in [ERROR_ACCESS_DENIED,ERROR_INVALID_PARAMETER,ERROR_INVALID_HANDLE] {assert!(diagnostic_undo_readback(code,&row,guid,17,&destination,5).is_err());}
        assert!(OwnedNetwork::recovering().route_undo_readback(guid,17,&destination,5).unwrap_err().to_string().contains("interface_not_owned_by_this_process"));
    }
    #[test]
    fn route_order_is_specific_first_deterministic_and_preserves_inputs() {
        let routes=["198.51.100.0/24","198.51.100.1/32","198.51.100.0/31","198.51.100.0/32","192.0.2.0/24"].map(|s|Cidr::parse(s).unwrap());
        let expected=["198.51.100.0/32","198.51.100.1/32","198.51.100.0/31","192.0.2.0/24","198.51.100.0/24"];
        for reversed in [false,true] {for rotation in 0..routes.len() {
            let mut permutation=routes.to_vec();permutation.rotate_left(rotation);if reversed {permutation.reverse();}
            assert_eq!(route_apply_order(&permutation).iter().map(|r|r.text()).collect::<Vec<_>>(),expected);
            assert_eq!(permutation.len(),routes.len());
        }}
        let duplicates=vec![routes[0].clone(),routes[0].clone()];assert_eq!(route_apply_order(&duplicates),duplicates.iter().collect::<Vec<_>>());
        assert!(route_apply_order(&[]).is_empty());
        let plan=|routes|crate::policy::Plan::from_engine(serde_json::json!({"address":"192.0.2.241","netmask":"255.255.255.255","routes":routes}));
        assert!(plan(vec!["198.51.100.0/33"]).is_err());assert!(plan(vec!["198.51.100.0/255.0.255.0"]).is_err());
        assert!(plan(Vec::<&str>::new()).unwrap().routes.is_empty());assert_eq!(plan(vec!["198.51.100.0/24","198.51.100.0/24"]).unwrap().routes.len(),1);
    }
    #[test]
    fn middle_route_failure_recovers_reverse_intents_without_owning_foreign_conflicts() {
        use crate::transaction::{Journal,Store};use std::collections::BTreeSet;
        #[derive(Default)]struct Disk {last:Option<Journal>}
        impl Store for Disk {fn save(&mut self,journal:&Journal)->Result<()> {self.last=Some(journal.clone());Ok(())}}
        struct Fake {present:BTreeSet<Cidr>,foreign:bool,target:Cidr,undone:Vec<Cidr>,fail_cleanup:bool}
        let destination=|object:&Object|match object {Object::Route {destination,..}=>destination.clone(),_=>panic!("route fixture only")};
        impl Network for Fake {
            fn prepare(&self,object:&Object)->Result<()> {let Object::Route {destination,..}=object else {unreachable!()};ensure!(!self.present.contains(destination),"existing_route");Ok(())}
            fn apply(&mut self,object:&Object)->Result<()> {let Object::Route {destination,..}=object else {unreachable!()};self.present.insert(destination.clone());ensure!(destination!=&self.target,"uncertain_middle_apply");Ok(())}
            fn undo(&mut self,object:&Object)->Result<()> {let Object::Route {destination,..}=object else {unreachable!()};ensure!(!self.fail_cleanup,"cleanup_failed");assert!(!self.foreign || destination!=&self.target,"foreign route must never be owned");self.present.remove(destination);self.undone.push(destination.clone());Ok(())}
        }
        let routes=["198.51.100.0/24","198.51.100.0/27","198.51.100.0/28","198.51.100.0/29","198.51.100.0/30","198.51.100.0/31","198.51.100.0/32","198.51.100.1/32"].map(|s|Cidr::parse(s).unwrap());
        for foreign in [false,true] {
            let target=routes[2].clone();let mut network=Fake {present:if foreign {[target.clone()].into_iter().collect()} else {BTreeSet::new()},foreign,target,undone:vec![],fail_cleanup:false};
            let mut disk=Disk::default();let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();let mut failed=false;
            for destination in route_apply_order(&routes) {
                if journal.apply(Object::Route {guid:Uuid::nil(),luid:17,destination:destination.clone(),metric:5},&mut disk,&mut network).is_err() {failed=true;break;}
            }
            assert!(failed);assert_eq!(journal.entries.len(),if foreign {5} else {6});
            let expected=journal.entries.iter().rev().map(|entry|destination(&entry.object)).collect::<Vec<_>>();
            let mut recovered:Journal=serde_json::from_slice(&serde_json::to_vec(disk.last.as_ref().unwrap()).unwrap()).unwrap();
            network.fail_cleanup=true;assert!(recovered.cleanup(&mut disk,&mut network).is_err());assert!(!recovered.complete());assert!(network.undone.is_empty());
            network.fail_cleanup=false;recovered.cleanup(&mut disk,&mut network).unwrap();assert_eq!(network.undone,expected);assert!(recovered.complete());
            assert_eq!(network.present,if foreign {[network.target.clone()].into_iter().collect()} else {BTreeSet::new()});
        }
    }
    #[test]
    fn adapter_close_waits_for_both_native_views_and_rejects_drift_or_unknown_state() {
        use std::{cell::Cell,time::Duration};
        let polls=Cell::new(0_u64);
        wait_for_adapter_absence(Some(17),||Ok(if polls.get()<60 {(Some(17),false)} else if polls.get()<65 {(None,false)} else {(None,true)}),||Duration::from_millis(polls.get()*50),||polls.set(polls.get()+1)).unwrap();
        assert_eq!(polls.get(),65,"a devnode that is still present must not be classified as unregister pending");
        for observed in [(None,false),(Some(17),false)] {
            polls.set(0);
            assert!(wait_for_adapter_absence(Some(17),||Ok(observed),||Duration::from_millis(polls.get()*50),||polls.set(polls.get()+1)).unwrap_err().to_string().contains("owned_adapter_still_present_cleanup_required"));
            assert_eq!(polls.get(),600,"a pending adapter must remain bounded to 30 seconds");
        }
        assert!(wait_for_adapter_absence(Some(17),||Ok((Some(18),true)),||panic!("changed identity must fail immediately"),||panic!("must not retry changed identity")).is_err());
        polls.set(0);
        let pending=wait_for_adapter_absence(Some(17),||Ok((Some(17),true)),||panic!("first exact pending observation must not block"),||polls.set(polls.get()+1)).unwrap_err();
        assert!(pending.is::<AdapterUnregisterPending>());assert_eq!(polls.get(),0);
        assert!(wait_for_adapter_absence(Some(17),||Ok((Some(17),true)),||Duration::from_secs(30),||panic!("a closed adapter retry must not block the actor")).unwrap_err().is::<AdapterUnregisterPending>());
        wait_for_adapter_absence(Some(17),||Ok((None,true)),||panic!("actual dual absence completes immediately"),||panic!("must not wait after dual absence")).unwrap();
        assert!(!wait_for_adapter_absence(None,||Ok((Some(17),true)),||Duration::from_secs(30),||panic!("missing ownership must not retry")).unwrap_err().is::<AdapterUnregisterPending>());
        assert!(wait_for_adapter_absence(Some(17),||anyhow::bail!("inventory denied"),||panic!("unknown state must fail immediately"),||panic!("must not retry unknown state")).unwrap_err().to_string().contains("inventory denied"));
    }
    #[test]
    fn peer_path_verification_reads_the_real_underlay_and_rejects_changed_identity() {
        let peer="127.0.0.1".parse().unwrap();let mut path=peer_path(peer).unwrap();
        verify_peer_path(peer,&path).unwrap();path.luid^=1;assert!(verify_peer_path(peer,&path).is_err());
        verify_dns_routes(&[peer],path.luid^1).unwrap();assert!(verify_dns_routes(&[peer],path.luid).is_err());
        assert!(peer_path("::1".parse().unwrap()).is_err());
    }
    #[test]
    fn address_readiness_waits_for_dad_and_never_treats_duplicates_or_invalid_as_ready() {
        assert!(address_dad_ready(IpDadStatePreferred).unwrap());assert!(!address_dad_ready(IpDadStateTentative).unwrap());
        for state in [IpDadStateInvalid,IpDadStateDuplicate,IpDadStateDeprecated,99] {assert!(address_dad_ready(state).is_err());}
    }
    #[test]
    fn mtu_teardown_requires_documented_missing_code_and_fresh_exact_absence() {
        for code in [ERROR_FILE_NOT_FOUND,ERROR_NOT_FOUND] {
            assert!(!mtu_cleanup_result(code,||Ok(false)).unwrap());
            assert!(mtu_cleanup_result(code,||Ok(true)).is_err());
            assert!(mtu_cleanup_result(code,||anyhow::bail!("interface_identity_changed")).is_err());
        }
        for code in [ERROR_INVALID_PARAMETER,ERROR_ACCESS_DENIED,ERROR_INVALID_HANDLE] {
            assert!(mtu_cleanup_result(code,||Ok(false)).is_err());
            assert!(mtu_cleanup_result(code,||Ok(true)).is_err());
        }
        assert!(mtu_cleanup_result(0,||panic!("successful API needs no absence fallback")).unwrap());
    }
    #[test]
    fn loopback_guid_inventory_matches_only_its_actual_luid() {
        unsafe {
            let mut table=std::ptr::null_mut();status(GetIfTable2(&mut table)).unwrap();
            let row=*std::slice::from_raw_parts((*table).Table.as_ptr(),(*table).NumEntries as usize).iter().find(|row|row.Type == 24).expect("Windows software loopback");
            FreeMibTable(table.cast());
            let guid=super::super::wintun::from_guid(row.InterfaceGuid);let luid=row.InterfaceLuid.Value;
            assert_eq!(lookup_guid(guid).unwrap(),Some(luid));assert!(OwnedNetwork::verify_interface(guid,luid).unwrap());
            assert!(OwnedNetwork::verify_interface(guid,luid ^ 1).is_err());
            assert!(!OwnedNetwork::verify_interface(Uuid::new_v4(),luid).unwrap());
        }
    }
    #[test]
    fn mtu_roundtrip_changes_only_mtu_and_required_ipv4_api_normalization() {
        let mut row=interface(17,false);row.NlMtu=1500;row.SitePrefixLength=32;row.Metric=77;
        let after=mtu_update(row,1400);assert_eq!(after.NlMtu,1400);assert_eq!(after.SitePrefixLength,0);assert_eq!(after.Metric,77);
        let restored=mtu_update(after,1500);assert_eq!(restored.NlMtu,1500);assert_eq!(restored.Metric,77);
        let mut ipv6=interface(17,true);ipv6.SitePrefixLength=64;assert_eq!(mtu_update(ipv6,1400).SitePrefixLength,64);
    }
    #[test]
    fn created_route_builder_matches_the_cleanup_ownership_contract() {
        let row=route(17,&Cidr::parse("198.51.100.241/32").unwrap(),5);
        assert_eq!(row.Metric,5);assert_eq!(row.Protocol,MIB_IPPROTO_NETMGMT);assert_eq!(row.ValidLifetime,u32::MAX);assert_eq!(row.PreferredLifetime,u32::MAX);
        assert!(!row.Publish,"Publish initialized true");assert!(!row.AutoconfigureAddress,"AutoconfigureAddress initialized true");assert!(!row.Loopback);assert!(row.Immortal);assert_eq!(row.SitePrefixLength,32);
    }
    #[test]
    fn route_absence_guard_keeps_conflicts_and_query_errors_fatal_with_bounded_diagnostics() {
        let destination=Cidr::parse("198.51.100.0/27").unwrap();
        let mut row=route(19,&Cidr::parse("198.51.100.31/32").unwrap(),256);
        row.Protocol=MIB_IPPROTO_LOCAL;row.Origin=3;row.Loopback=true;row.ValidLifetime=71;row.PreferredLifetime=61;
        for stage in ["prepare","apply"] {
            for code in [ERROR_NOT_FOUND,ERROR_FILE_NOT_FOUND] {assert!(require_route_absent(code,&row,&destination,17,stage).is_ok());}
            for code in [0,ERROR_ACCESS_DENIED,ERROR_INVALID_PARAMETER,ERROR_INVALID_HANDLE] {
                let message=require_route_absent(code,&row,&destination,17,stage).unwrap_err().to_string();
                let returned=if code==0 {route_row_text(&row)} else {"unavailable".into()};
                assert_eq!(message,format!("route_already_exists_or_unqueryable: stage={stage} code={code} destination=198.51.100.0/27 luid=17 returned=[{returned}]"));
                assert!(message.len()<640);
            }
        }
        let text=route_row_text(&row);
        for field in ["destination=198.51.100.31/32","nextHop=0.0.0.0","luid=19","protocol=2","origin=3","metric=256","valid=71","preferred=61","loopback=true"] {assert!(text.contains(field),"{text}");}
    }
}
