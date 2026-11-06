use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeSet, HashMap}, net::{IpAddr, Ipv4Addr}};
use termterm_vpn_core::network::{NetworkPlan, Reservations};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Cidr {
    pub address: IpAddr,
    pub prefix: u8,
}
impl Cidr {
    pub fn parse(value: &str) -> Result<Self> {
        let (ip, prefix) = value.split_once('/').ok_or_else(|| anyhow::anyhow!("cidr_required"))?;
        let address: IpAddr = ip.parse()?;
        let prefix = if prefix.contains('.') {
            ensure!(address.is_ipv4(), "ipv4_netmask_requires_ipv4_address");
            let mask = u32::from(prefix.parse::<Ipv4Addr>()?);
            let bits = mask.leading_ones() as u8;
            ensure!(mask == if bits == 0 {0} else {u32::MAX << (32 - bits)}, "non_contiguous_ipv4_netmask");
            bits
        } else { prefix.parse()? };
        ensure!(prefix <= if address.is_ipv4() { 32 } else { 128 }, "cidr_invalid");
        Ok(Self { address, prefix })
    }
    pub fn network(&self) -> Self {
        let address = match self.address {
            IpAddr::V4(v) => IpAddr::V4((u32::from(v) & if self.prefix == 0 { 0 } else { u32::MAX << (32 - self.prefix) }).into()),
            IpAddr::V6(v) => IpAddr::V6((u128::from(v) & if self.prefix == 0 { 0 } else { u128::MAX << (128 - self.prefix) }).into()),
        };
        Self { address, prefix: self.prefix }
    }
    pub fn text(&self) -> String { format!("{}/{}", self.address, self.prefix) }
    pub fn overlaps(&self, other: &Self) -> bool {
        if self.address.is_ipv4() != other.address.is_ipv4() {return false;}
        let prefix = self.prefix.min(other.prefix);
        Self {address:self.address,prefix}.network().address == Self {address:other.address,prefix}.network().address
    }
    pub fn contains(&self, ip: IpAddr) -> bool { self.overlaps(&Self {address:ip,prefix:if ip.is_ipv4() {32} else {128}}) }
}

/// Subtraction changes only our includes; it never installs a physical bypass route.
fn subtract(routes: Vec<Cidr>, exclusions: &[Cidr]) -> Result<Vec<Cidr>> {
    let mut current: BTreeSet<_> = routes.into_iter().collect();
    for exclude in exclusions {
        let mut next = BTreeSet::new();
        let mut pending: Vec<_> = current.into_iter().collect();
        while let Some(route) = pending.pop() {
            if !route.overlaps(exclude) { next.insert(route); }
            else if exclude.prefix > route.prefix {
                let prefix = route.prefix + 1;
                let other = match route.address {
                    IpAddr::V4(ip) => IpAddr::V4((u32::from(ip) | (1 << (32-prefix))).into()),
                    IpAddr::V6(ip) => IpAddr::V6((u128::from(ip) | (1 << (128-prefix))).into()),
                };
                pending.push(Cidr {address:route.address,prefix}); pending.push(Cidr {address:other,prefix});
            }
            ensure!(next.len()+pending.len() <= 1024, "excluded_route_expansion_too_large");
        }
        current = next;
    }
    Ok(current.into_iter().collect())
}

pub fn search_domain(value: &str) -> Result<String> {
    ensure!(value.is_empty() || !value.trim_end_matches('.').is_empty(),"invalid_search_domain");
    let value = value.trim_end_matches('.').to_ascii_lowercase();
    ensure!(value.len() <= 253 && (value.is_empty() || value.split('.').all(|label| !label.is_empty() && label.len() <= 63
        && !label.starts_with('-') && !label.ends_with('-') && label.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-'))), "invalid_search_domain");
    Ok(value)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Address {
    cidr: String,
    #[serde(default)] gateway: String,
    #[serde(default)] net30: bool,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnginePlan {
    #[serde(default)] addresses: Vec<Address>,
    #[serde(default)] address: String,
    #[serde(default)] netmask: String,
    #[serde(default)] address6: String,
    routes: Vec<String>,
    #[serde(default)] excluded_routes: Vec<String>,
    #[serde(default)] dns: Vec<String>,
    #[serde(default)] dns_domains: Vec<String>,
    #[serde(default)] dns_options: serde_json::Value,
    #[serde(default)] search_domain: String,
    #[serde(default)] proxy_pac: String,
    #[serde(default)] remote: String,
    #[serde(default)] mtu: u32,
    #[serde(default)] interface: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct UnsupportedPolicy {
    dns_servers: usize,
    dns_domains: usize,
    search_domain: bool,
    proxy_pac: bool,
    excluded_routes: usize,
    dns_options: bool,
    default_route_present: bool,
}
impl std::fmt::Display for UnsupportedPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "dns_proxy_exclusions_unsupported: dnsServers={} dnsDomains={} searchDomain={} proxyPac={} excludedRoutes={} dnsOptions={} defaultRoutePresent={}",
            self.dns_servers, self.dns_domains, self.search_domain, self.proxy_pac, self.excluded_routes, self.dns_options,self.default_route_present)
    }
}
impl std::error::Error for UnsupportedPolicy {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub addresses: Vec<Assignment>,
    pub routes: Vec<Cidr>,
    pub mtu: u32,
    #[serde(default)] pub dns_servers: Vec<IpAddr>,
    #[serde(default)] pub search_domain: String,
    #[serde(default)] pub excluded_routes: Vec<Cidr>,
    #[serde(default)] pub remote: Option<IpAddr>,
    #[serde(default)] pub full_tunnel: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Assignment { pub cidr: Cidr, pub gateway: Option<IpAddr> }
fn empty_policy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(v) => !v,
        serde_json::Value::Number(v) => v.as_u64() == Some(0),
        serde_json::Value::String(v) => v.is_empty(),
        serde_json::Value::Array(v) => v.iter().all(empty_policy),
        serde_json::Value::Object(v) => v.values().all(empty_policy),
    }
}
impl Plan {
    pub fn from_engine(value: serde_json::Value) -> Result<Self> {
        let raw: EnginePlan = serde_json::from_value(value)?;
        ensure!(raw.addresses.len() <= 8 && raw.routes.len() <= 1024 && raw.excluded_routes.len() <= 1024 && raw.dns.len() <= 8, "network_plan_too_large");
        let routes = raw.routes.iter().map(|v| Cidr::parse(v).map(|c| c.network())).collect::<Result<Vec<_>>>().map_err(|_|anyhow::anyhow!("invalid_route"))?;
        let unsupported = UnsupportedPolicy {
            dns_servers: raw.dns.len(), dns_domains: raw.dns_domains.len(),
            search_domain: !raw.search_domain.is_empty(), proxy_pac: !raw.proxy_pac.is_empty(),
            excluded_routes: raw.excluded_routes.len(), dns_options: !empty_policy(&raw.dns_options),
            default_route_present: routes.iter().any(|r|r.prefix==0),
        };
        if unsupported.dns_domains>0 || unsupported.proxy_pac || unsupported.dns_options { return Err(unsupported.into()); }
        ensure!(raw.address6.is_empty(), "openconnect_ipv6_address_unsupported");
        ensure!(raw.interface.is_empty(), "engine_must_not_choose_interface");
        let remote = if raw.remote.is_empty() {None} else {Some(raw.remote.parse::<IpAddr>().map_err(|_|anyhow::anyhow!("invalid_remote_ip"))?)};
        let mut addresses = vec![];
        for item in raw.addresses {
            ensure!(!item.net30, "net30_topology_unsupported");
            let cidr = Cidr::parse(&item.cidr)?;
            let gateway = if item.gateway.is_empty() { None } else { Some(item.gateway.parse::<IpAddr>()?) };
            ensure!(gateway.is_none_or(|v| v.is_ipv4() == cidr.address.is_ipv4()), "gateway_address_family_mismatch");
            // Wintun carries raw L3 packets; routes are on-link and need no ARP next hop.
            addresses.push(Assignment { cidr, gateway });
        }
        if !raw.address.is_empty() {
            let mask: Ipv4Addr = raw.netmask.parse()?;
            let n = u32::from(mask);
            let prefix = n.leading_ones() as u8;
            ensure!(n == if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) }, "invalid_netmask");
            let ip: Ipv4Addr = raw.address.parse()?;
            addresses.push(Assignment { cidr: Cidr { address: ip.into(), prefix }, gateway: None });
        }
        let excluded_routes = raw.excluded_routes.iter().map(|r|Cidr::parse(r).map(|c|c.network())).collect::<Result<BTreeSet<_>>>().map_err(|_|anyhow::anyhow!("invalid_excluded_route"))?.into_iter().collect::<Vec<_>>();
        let mut dns_servers = raw.dns.iter().map(|d|d.parse::<IpAddr>()).collect::<std::result::Result<Vec<_>,_>>().map_err(|_|anyhow::anyhow!("invalid_dns_server"))?;
        let mut seen=BTreeSet::new();dns_servers.retain(|server|seen.insert(*server));
        let full_tunnel=routes.iter().any(|r|r.prefix==0);
        let mut holes=excluded_routes.clone();
        if full_tunnel {
            ensure!(routes.iter().all(|r|r.address.is_ipv4()),"ipv6_full_tunnel_unsupported");
            let peer=remote.ok_or_else(||anyhow::anyhow!("full_tunnel_requires_numeric_peer"))?;
            ensure!(peer.is_ipv4() && !peer.is_unspecified() && !peer.is_multicast() && peer!=IpAddr::V4(Ipv4Addr::BROADCAST),"invalid_full_tunnel_peer");
            holes.push(Cidr {address:peer,prefix:32});
            // Local loopback and non-unicast traffic never belong in an IPv4 VPN.
            for value in ["127.0.0.0/8","224.0.0.0/4","255.255.255.255/32"] {holes.push(Cidr::parse(value)?);}
        }
        let mut plan = Self { addresses, routes:subtract(routes,&holes)?, mtu: if raw.mtu == 0 { 1500 } else { raw.mtu }, dns_servers, search_domain:search_domain(&raw.search_domain)?,excluded_routes,remote,full_tunnel };
        // Windows creates the exact connected route when assigning the address.
        if full_tunnel {plan.routes.retain(|route|!plan.addresses.iter().any(|a|a.cidr.network()==*route));}
        // DNS addresses need tunnel reachability even when not covered by the server's includes.
        for server in &plan.dns_servers {
            if !plan.routes.iter().chain(plan.addresses.iter().map(|a|&a.cidr)).any(|r|r.contains(*server)) {
                plan.routes.push(Cidr {address:*server,prefix:32});
            }
        }
        plan.validate()?;
        Ok(plan)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.addresses.is_empty() && self.addresses.len() <= 8 && self.routes.len() <= 1024, "invalid_addresses_or_routes");
        ensure!((576..=9000).contains(&self.mtu), "invalid_mtu");
        ensure!(self.excluded_routes.len()<=1024 && self.dns_servers.len()<=8,"network_plan_too_large");
        ensure!(search_domain(&self.search_domain)?==self.search_domain,"search_domain_not_canonical");
        ensure!(self.search_domain.is_empty() || !self.dns_servers.is_empty(),"search_domain_requires_dns");
        if self.full_tunnel {
            ensure!(self.remote.is_some_and(|peer|peer.is_ipv4() && !peer.is_unspecified() && !peer.is_multicast() && peer!=IpAddr::V4(Ipv4Addr::BROADCAST)),"full_tunnel_requires_numeric_peer");
            ensure!(!self.routes.is_empty() && self.routes.iter().all(|r|r.address.is_ipv4()) && self.addresses.iter().all(|a|a.cidr.address.is_ipv4()),"invalid_ipv4_full_tunnel");
            for value in ["127.0.0.0/8","224.0.0.0/4","255.255.255.255/32"] {let hole=Cidr::parse(value)?;ensure!(!self.routes.iter().any(|r|r.overlaps(&hole)),"full_tunnel_captures_nonunicast");}
        }
        for server in &self.dns_servers {
            ensure!(server.is_ipv4() && !server.is_unspecified() && !server.is_multicast() && !server.is_loopback() && *server != IpAddr::V4(Ipv4Addr::BROADCAST),"unsupported_dns_server");
            ensure!(!self.excluded_routes.iter().any(|r|r.contains(*server)),"dns_server_excluded");
            ensure!(self.routes.iter().chain(self.addresses.iter().map(|a|&a.cidr)).any(|r|r.contains(*server)),"dns_server_not_tunnel_routed");
        }
        for assignment in &self.addresses {
            let address = &assignment.cidr;
            ensure!(address.prefix > 0 && address.prefix <= if address.address.is_ipv4() {32} else {128}, "invalid_address_prefix");
            ensure!(!address.address.is_unspecified() && !address.address.is_multicast() && !address.address.is_loopback(), "invalid_assigned_address");
            ensure!(address.address.is_ipv4() || self.mtu >= 1280, "ipv6_mtu_too_small");
        }
        for route in &self.routes {
            ensure!(route.prefix > 0, "full_tunnel_unsupported");
            ensure!(route.prefix <= if route.address.is_ipv4() {32} else {128}, "invalid_route_prefix");
            ensure!(route == &route.network(), "route_not_canonical");
        }
        for excluded in &self.excluded_routes {
            ensure!(excluded.prefix<=if excluded.address.is_ipv4(){32}else{128} && excluded==&excluded.network(),"invalid_excluded_route");
            ensure!(!self.addresses.iter().any(|a|a.cidr.overlaps(excluded)),"exclusion_overlaps_connected_prefix");
            ensure!(!self.routes.iter().any(|r|r.overlaps(excluded)),"excluded_route_still_captured");
        }
        if let Some(remote)=self.remote {ensure!(!self.routes.iter().chain(self.addresses.iter().map(|a|&a.cidr)).any(|r|r.contains(remote)),"vpn_endpoint_captured");}
        Ok(())
    }
    fn conflict_plan(&self) -> NetworkPlan {
        NetworkPlan { routes: self.routes.iter().chain(self.addresses.iter().map(|a| &a.cidr)).map(|c| c.network().text()).collect(), dns_servers: self.dns_servers.clone(), dns_domains: vec![] }
    }
    pub fn host_route_conflicts(&self, existing:&Cidr, connected:bool)->bool {
        self.addresses.iter().any(|a|a.cidr.overlaps(existing)) || self.routes.iter().any(|route|route.overlaps(existing)
            && !(self.full_tunnel && connected && existing.prefix>route.prefix))
    }
}

#[derive(Default)]
pub struct Registry {
    reservations: Reservations,
    sessions: HashMap<Uuid, u64>,
    plans: HashMap<Uuid, Plan>,
}
impl Registry {
    pub fn reserve(&mut self, id: Uuid, generation: u64, plan: &Plan) -> Result<()> {
        ensure!(generation > 0 && generation < (1_u64 << 53), "invalid_generation");
        ensure!(self.sessions.len() < crate::MAX_CONNECTIONS, "maximum_three_connections");
        ensure!(!self.sessions.contains_key(&id), "session_already_reserved");
        plan.validate()?;
        for old in self.plans.values() {
            ensure!(!(plan.full_tunnel && old.full_tunnel),"competing_full_tunnel_session");
            ensure!(plan.dns_servers.is_empty() || old.dns_servers.is_empty(),"competing_dns_session");
            ensure!(!old.remote.is_some_and(|remote|plan.routes.iter().chain(plan.addresses.iter().map(|a|&a.cidr)).any(|r|r.contains(remote)))
                && !plan.remote.is_some_and(|remote|old.routes.iter().chain(old.addresses.iter().map(|a|&a.cidr)).any(|r|r.contains(remote))),"vpn_endpoint_conflicts_with_other_vpn");
            ensure!(!plan.excluded_routes.iter().any(|r|old.routes.iter().chain(old.addresses.iter().map(|a|&a.cidr)).any(|o|r.overlaps(o)))
                && !old.excluded_routes.iter().any(|r|plan.routes.iter().chain(plan.addresses.iter().map(|a|&a.cidr)).any(|o|r.overlaps(o))),"exclusion_conflicts_with_other_vpn");
        }
        self.reservations.reserve(&id.to_string(), generation, plan.conflict_plan(), false).map_err(|_|anyhow::anyhow!("network_reservation_conflict"))?;
        self.sessions.insert(id, generation);
        self.plans.insert(id,plan.clone());
        Ok(())
    }
    pub fn check(&self, id: Uuid, generation: u64) -> Result<()> {
        ensure!(self.sessions.get(&id) == Some(&generation), "stale_session_generation");
        Ok(())
    }
    /// The caller must have completed reverse cleanup; failure keeps the reservation.
    pub fn release_after_cleanup(&mut self, id: Uuid, generation: u64) -> Result<()> {
        self.check(id, generation)?;
        ensure!(self.reservations.release(&id.to_string(), generation), "reservation_missing");
        self.sessions.remove(&id);
        self.plans.remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wireguard_plan_reserves_all_pinned_endpoints_and_rejects_capture_by_another_vpn() {
        let raw=serde_json::json!({"addresses":[{"cidr":"10.97.0.2/32"}],"routes":["0.0.0.0/0"],"remote":"192.0.2.1","excludedRoutes":["192.0.2.1/32","198.51.100.1/32"],"mtu":1420});
        let wireguard_plan=Plan::from_engine(raw).unwrap();for endpoint in ["192.0.2.1","198.51.100.1"] {assert!(!wireguard_plan.routes.iter().any(|r|r.contains(endpoint.parse().unwrap())));}
        let mut registry=Registry::default();registry.reserve(Uuid::new_v4(),1,&wireguard_plan).unwrap();
        let mut other=plan(3);other.addresses[0].cidr=Cidr::parse("10.99.0.2/32").unwrap();other.routes=vec![Cidr::parse("198.51.100.1/32").unwrap()];
        assert!(registry.reserve(Uuid::new_v4(),1,&other).is_err());
    }
    fn plan(n: u8) -> Plan { Plan { addresses: vec![Assignment { cidr: Cidr::parse(&format!("10.{n}.0.2/24")).unwrap(), gateway: None }], routes: vec![Cidr::parse(&format!("172.{n}.0.0/16")).unwrap()], mtu: 1500, dns_servers:vec![],search_domain:String::new(),excluded_routes:vec![],remote:None,full_tunnel:false } }
    #[test]
    fn three_pending_connections_and_stale_generations_are_bounded() {
        let mut r = Registry::default();
        let ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        r.reserve(ids[0], 1, &plan(1)).unwrap();
        assert!(r.reserve(ids[1], 1, &plan(1)).is_err());
        r.reserve(ids[1], 1, &plan(2)).unwrap();
        r.reserve(ids[2], 1, &plan(3)).unwrap();
        assert!(r.reserve(ids[3], 1, &plan(4)).is_err());
        assert!(r.release_after_cleanup(ids[0], 2).is_err());
        assert!(r.reserve(ids[3], 1, &plan(1)).is_err());
        r.release_after_cleanup(ids[0], 1).unwrap();
        r.reserve(ids[3], 2, &plan(1)).unwrap();
    }
    fn base() -> serde_json::Value { serde_json::json!({"address":"10.1.0.2","netmask":"255.255.255.0","routes":["172.16.0.0/16"],"mtu":1400}) }
    #[test]
    fn dns_suffix_exclusions_and_remote_are_retained_in_exact_plan() {
        let mut input=base();input["dns"]=serde_json::json!(["172.16.1.53","172.16.1.54","172.16.1.53"]);input["searchDomain"]="CORP.Example.".into();
        input["excludedRoutes"]=serde_json::json!(["172.16.2.0/24"]);input["remote"]="192.0.2.1".into();
        let p=Plan::from_engine(input).unwrap();assert_eq!(p.dns_servers.len(),2);assert_eq!(p.search_domain,"corp.example");assert_eq!(p.remote,Some("192.0.2.1".parse().unwrap()));
        assert!(p.routes.iter().all(|r|!r.contains("172.16.2.1".parse().unwrap())));assert!(p.routes.iter().any(|r|r.contains("172.16.3.1".parse().unwrap())));
        assert_eq!(p.conflict_plan().dns_servers,p.dns_servers);assert!(p.conflict_plan().dns_domains.is_empty());
    }
    #[test]
    fn unsupported_summary_has_all_counts_and_no_values_including_original_default() {
        let mut input=base();input["routes"]=serde_json::json!(["0.0.0.0/0"]);input["excludedRoutes"]=serde_json::json!(["0.0.0.0/0"]);
        input["dns"]=serde_json::json!(["private-dns"]);input["dnsDomains"]=serde_json::json!(["private-domain"]);input["searchDomain"]="private-suffix".into();input["proxyPac"]="private-pac".into();input["dnsOptions"]=serde_json::json!({"private-key":"private-value"});
        let error=Plan::from_engine(input).unwrap_err();assert_eq!(error.to_string(),"dns_proxy_exclusions_unsupported: dnsServers=1 dnsDomains=1 searchDomain=true proxyPac=true excludedRoutes=1 dnsOptions=true defaultRoutePresent=true");assert!(!format!("{error:#?}").contains("private-"));
        for field in ["dnsDomains","proxyPac","dnsOptions"] {let mut input=base();input[field]=if field=="dnsDomains" {serde_json::json!(["secret"])} else {serde_json::json!("secret")};assert!(Plan::from_engine(input).unwrap_err().downcast_ref::<UnsupportedPolicy>().is_some());}
    }
    #[test]
    fn cidr_subtraction_equal_supernet_subnet_disjoint_families_and_bound() {
        let routes=||vec![Cidr::parse("172.16.0.0/16").unwrap()];
        for excluded in ["172.16.0.0/16","172.0.0.0/8"] {assert!(subtract(routes(),&[Cidr::parse(excluded).unwrap()]).unwrap().is_empty());}
        assert_eq!(subtract(routes(),&[Cidr::parse("192.0.2.0/24").unwrap()]).unwrap(),routes());
        assert_eq!(subtract(routes(),&[Cidr::parse("fd00::/8").unwrap()]).unwrap(),routes());
        let p=subtract(routes(),&[Cidr::parse("172.16.1.0/24").unwrap()]).unwrap();assert_eq!(p.len(),8);assert!(!p.iter().any(|r|r.contains("172.16.1.2".parse().unwrap())));
        let v6=subtract(vec![Cidr::parse("fd00::/64").unwrap()],&[Cidr::parse("fd00::1/128").unwrap()]).unwrap();assert_eq!(v6.len(),64);assert!(!v6.iter().any(|r|r.contains("fd00::1".parse().unwrap())));
        let many=(0..1024).map(|n|Cidr::parse(&format!("172.16.{}.0/32",n%256)).unwrap()).collect::<Vec<_>>();assert!(subtract(routes(),&many).is_err());
    }
    #[test]
    fn invalid_dns_suffix_exclusions_and_captured_endpoint_fail_closed() {
        for dns in ["private-value","127.0.0.1","0.0.0.0","224.0.0.1","255.255.255.255","fd00::53"] {let mut input=base();input["dns"]=serde_json::json!([dns]);assert!(Plan::from_engine(input).is_err(),"{dns}");}
        for suffix in ["has space.example","-bad.example","bad..example","a\n.example"] {let mut input=base();input["dns"]=serde_json::json!(["172.16.1.53"]);input["searchDomain"]=suffix.into();assert!(Plan::from_engine(input).is_err());}
        let mut input=base();input["searchDomain"]="corp.example".into();assert!(Plan::from_engine(input).is_err());
        let mut input=base();input["excludedRoutes"]=serde_json::json!(["10.1.0.128/25"]);assert_eq!(Plan::from_engine(input).unwrap_err().to_string(),"exclusion_overlaps_connected_prefix");
        let mut input=base();input["dns"]=serde_json::json!(["172.16.1.53"]);input["excludedRoutes"]=serde_json::json!(["172.16.1.0/24"]);assert_eq!(Plan::from_engine(input).unwrap_err().to_string(),"dns_server_excluded");
        let mut input=base();input["remote"]="172.16.1.1".into();assert_eq!(Plan::from_engine(input).unwrap_err().to_string(),"vpn_endpoint_captured");
        let mut input=base();input["dns"]=serde_json::json!(["198.51.100.53"]);let p=Plan::from_engine(input).unwrap();assert!(p.routes.iter().any(|r|r.text()=="198.51.100.53/32"));
    }
    #[test]
    fn dns_pending_reservation_conflicts_before_routes_and_survives_stale_cleanup() {
        let mut r=Registry::default();let id=Uuid::new_v4();let mut p=plan(1);p.dns_servers=vec!["172.1.1.53".parse().unwrap()];r.reserve(id,1,&p).unwrap();
        let mut other=plan(2);other.dns_servers=vec!["172.2.1.53".parse().unwrap()];assert_eq!(r.reserve(Uuid::new_v4(),1,&other).unwrap_err().to_string(),"competing_dns_session");
        assert!(r.release_after_cleanup(id,2).is_err());assert!(r.reserve(Uuid::new_v4(),1,&other).is_err());r.release_after_cleanup(id,1).unwrap();r.reserve(Uuid::new_v4(),1,&other).unwrap();
    }
    #[test]
    fn openvpn_gateway_is_retained_and_empty_dns_options_are_accepted() {
        let value = serde_json::json!({"addresses":[{"cidr":"10.90.0.2/24","gateway":"10.90.0.1","net30":false}], "routes":["10.88.0.2/32"], "dnsOptions":{"servers":{},"search_domains":[]}});
        let plan = Plan::from_engine(value.clone()).unwrap();
        assert_eq!(plan.addresses[0].gateway, Some("10.90.0.1".parse().unwrap()));
        let mut bad = value.clone(); bad["addresses"][0]["net30"] = true.into(); assert!(Plan::from_engine(bad).is_err());
        let mut bad = value; bad["dnsOptions"]["servers"]["0"] = serde_json::json!({"address":"10.90.0.1"}); assert!(Plan::from_engine(bad).is_err());
    }
    #[test]
    fn openconnect_dotted_route_masks_are_normalized_without_loosening_cidr_validation() {
        assert_eq!(Cidr::parse("10.88.0.1/255.255.255.255").unwrap().text(),"10.88.0.1/32");
        assert_eq!(Cidr::parse("10.88.4.1/255.255.0.0").unwrap().network().text(),"10.88.0.0/16");
        for invalid in ["10.88.0.1/255.0.255.0","10.88.0.1/255.255.255.256","fd00::1/255.255.255.0","10.88.0.1/33"] {assert!(Cidr::parse(invalid).is_err(),"{invalid}");}
        let plan=Plan::from_engine(serde_json::json!({"address":"10.89.0.130","netmask":"255.255.255.0","routes":["10.88.0.1/255.255.255.255"],"remote":"127.0.0.1","mtu":1434})).unwrap();
        assert_eq!(plan.routes[0].text(),"10.88.0.1/32");
    }
    fn full() -> serde_json::Value {serde_json::json!({"address":"10.96.0.2","netmask":"255.255.255.0","routes":["0.0.0.0/0"],"remote":"192.0.2.5"})}
    #[test]
    fn full_tunnel_is_explicit_bounded_and_preserves_transport_and_exclusions() {
        let mut value=full();value["excludedRoutes"]=serde_json::json!(["10.88.0.0/24"]);
        let plan=Plan::from_engine(value).unwrap();assert!(plan.full_tunnel);assert!(plan.routes.len()<128);
        for ip in ["192.0.2.5","10.88.0.53","127.0.0.1","224.0.0.1","255.255.255.255"] {assert!(!plan.routes.iter().any(|r|r.contains(ip.parse().unwrap())),"{ip}");}
        assert!(plan.routes.iter().any(|r|r.contains("198.51.100.54".parse().unwrap())));
        for peer in ["","vpn.example","0.0.0.0","::1","224.0.0.1"] {let mut value=full();value["remote"]=peer.into();assert!(Plan::from_engine(value).is_err());}
        let mut value=full();value["routes"]=serde_json::json!(["::/0"]);assert!(Plan::from_engine(value).is_err());
        let mut value=full();let exclusions=subtract(vec![Cidr::parse("0.0.0.0/0").unwrap()],&[Cidr::parse("10.96.0.0/24").unwrap(),Cidr::parse("10.88.0.0/24").unwrap()]).unwrap();
        value["excludedRoutes"]=serde_json::json!(exclusions.iter().map(Cidr::text).collect::<Vec<_>>());
        let p=Plan::from_engine(value).unwrap();assert_eq!(p.routes,vec![Cidr::parse("10.88.0.0/24").unwrap()]);
        let mut value=full();value["excludedRoutes"]=serde_json::json!(["0.0.0.0/0"]);assert!(Plan::from_engine(value).is_err());
        let mut invalid=plan.clone();invalid.remote=None;assert!(invalid.validate().is_err());
        let mut r=Registry::default();r.reserve(Uuid::new_v4(),1,&plan).unwrap();assert_eq!(r.reserve(Uuid::new_v4(),1,&plan).unwrap_err().to_string(),"competing_full_tunnel_session");
        assert!(r.reserve(Uuid::new_v4(),1,&self::plan(3)).is_err());
    }
    #[test]
    fn full_tunnel_preserves_only_strictly_more_specific_connected_host_routes() {
        let mut p=plan(1);p.full_tunnel=true;p.remote=Some("192.0.2.5".parse().unwrap());p.routes=vec![Cidr::parse("172.0.0.0/8").unwrap()];
        for prefix in ["172.16.0.0/16","172.16.1.1/32"] {let host=Cidr::parse(prefix).unwrap();assert!(!p.host_route_conflicts(&host,true));assert!(p.host_route_conflicts(&host,false));}
        for prefix in ["172.0.0.0/8","128.0.0.0/1","10.1.0.0/24"] {assert!(p.host_route_conflicts(&Cidr::parse(prefix).unwrap(),true));}
        p.full_tunnel=false;assert!(p.host_route_conflicts(&Cidr::parse("172.16.0.0/16").unwrap(),true));
    }
}
