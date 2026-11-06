//! Strict wg-quick import and an in-memory, numeric-endpoint WireGuard UAPI request.
use anyhow::{bail, ensure, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{collections::BTreeSet, net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs}};
use zeroize::{Zeroize, Zeroizing};

pub struct Config {
    private_key: Zeroizing<[u8; 32]>,
    pub addresses: Vec<String>,
    pub dns: Vec<IpAddr>,
    pub search_domain: String,
    pub mtu: u16,
    pub listen_port: u16,
    pub peers: Vec<Peer>,
}
pub struct Peer {
    public_key: [u8; 32],
    preshared_key: Option<Zeroizing<[u8; 32]>>,
    pub allowed_ips: Vec<String>,
    pub endpoint: String,
    keepalive: u16,
}
fn key(value: &str) -> Result<[u8; 32]> {
    let mut bytes = Zeroizing::new(STANDARD.decode(value).map_err(|_|anyhow::anyhow!("wireguard_invalid_key"))?);
    ensure!(bytes.len() == 32 && STANDARD.encode(&*bytes) == value && bytes.iter().any(|v|*v != 0), "wireguard_invalid_key");
    let result = bytes.as_slice().try_into()?; bytes.zeroize(); Ok(result)
}
fn cidr(value: &str, network: bool) -> Result<String> {
    let (address, prefix) = value.split_once('/').ok_or_else(||anyhow::anyhow!("wireguard_cidr_required"))?;
    let address: IpAddr = address.parse().map_err(|_|anyhow::anyhow!("wireguard_invalid_ip"))?;
    ensure!(address.is_ipv4(),"wireguard_ipv6_unsupported");
    let prefix: u8 = prefix.parse().map_err(|_|anyhow::anyhow!("wireguard_invalid_prefix"))?;
    ensure!(prefix <= if address.is_ipv4() {32} else {128}, "wireguard_invalid_prefix");
    if !network { ensure!(prefix > 0 && !address.is_unspecified() && !address.is_multicast() && !address.is_loopback(), "wireguard_invalid_address"); }
    let address = if network { match address {
        IpAddr::V4(ip) => IpAddr::V4((u32::from(ip) & if prefix == 0 {0} else {u32::MAX << (32-prefix)}).into()),
        IpAddr::V6(ip) => IpAddr::V6((u128::from(ip) & if prefix == 0 {0} else {u128::MAX << (128-prefix)}).into()),
    }} else {address};
    Ok(format!("{address}/{prefix}"))
}
fn domain(value: &str) -> Result<String> {
    let value=value.trim_end_matches('.').to_ascii_lowercase();
    ensure!(!value.is_empty() && value.len()<=253 && value.split('.').all(|label| !label.is_empty() && label.len()<=63 && !label.starts_with('-') && !label.ends_with('-') && label.bytes().all(|c|c.is_ascii_alphanumeric() || c==b'-')),"wireguard_invalid_dns");
    Ok(value)
}
fn endpoint(value: &str) -> Result<()> {
    let (host,port)=value.rsplit_once(':').ok_or_else(||anyhow::anyhow!("wireguard_invalid_endpoint"))?;
    ensure!(port.parse::<u16>().is_ok_and(|p|p>0),"wireguard_invalid_endpoint");
    if host.starts_with('[') { ensure!(host.ends_with(']') && host[1..host.len()-1].parse::<std::net::Ipv6Addr>().is_ok(),"wireguard_invalid_endpoint");bail!("wireguard_ipv6_endpoint_unsupported"); }
    else { domain(host)?; }
    Ok(())
}
pub fn parse(text: &str) -> Result<Config> {
    ensure!(text.len()<=1024*1024 && !text.contains('\0'),"wireguard_invalid_configuration");
    let mut config=Config {private_key:Zeroizing::new([0;32]),addresses:vec![],dns:vec![],search_domain:String::new(),mtu:1420,listen_port:0,peers:vec![]};
    let mut section=""; let mut interface=false; let mut seen=BTreeSet::new(); let mut peer_keys=BTreeSet::new();
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let line=raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with(';') {continue;}
        if line=="[Interface]" {ensure!(!interface && section.is_empty(),"wireguard_duplicate_interface");interface=true;section="interface";seen.clear();continue;}
        if line=="[Peer]" {ensure!(interface && config.peers.len()<64,"wireguard_invalid_peer_count");section="peer";seen.clear();config.peers.push(Peer {public_key:[0;32],preshared_key:None,allowed_ips:vec![],endpoint:String::new(),keepalive:0});continue;}
        let (name,value)=line.split_once('=').ok_or_else(||anyhow::anyhow!("wireguard_invalid_field"))?; let name=name.trim();let value=value.trim();
        ensure!(!value.is_empty() && !section.is_empty(),"wireguard_empty_field");
        ensure!(["Address","DNS","AllowedIPs"].contains(&name) || seen.insert(name.to_string()),"wireguard_duplicate_field");
        match (section,name) {
            ("interface","PrivateKey")=>config.private_key=Zeroizing::new(key(value)?),
            ("interface","Address")=>for item in value.split(',') {config.addresses.push(cidr(item.trim(),false)?);},
            ("interface","MTU")=>{config.mtu=value.parse().map_err(|_|anyhow::anyhow!("wireguard_invalid_mtu"))?;ensure!((576..=9000).contains(&config.mtu),"wireguard_invalid_mtu");},
            ("interface","ListenPort")=>config.listen_port=value.parse().map_err(|_|anyhow::anyhow!("wireguard_invalid_listen_port"))?,
            ("interface","DNS")=>for item in value.split(',').map(str::trim) {match item.parse::<IpAddr>() {Ok(ip)=>{ensure!(ip.is_ipv4(),"wireguard_ipv6_unsupported");ensure!(!ip.is_unspecified() && !ip.is_multicast() && !ip.is_loopback(),"wireguard_invalid_dns");config.dns.push(ip);},Err(_)=>{ensure!(config.search_domain.is_empty(),"wireguard_multiple_search_domains_unsupported");config.search_domain=domain(item)?;}}},
            ("peer",name)=>{let peer=config.peers.last_mut().unwrap();match name {
                "PublicKey"=>{peer.public_key=key(value)?;ensure!(peer_keys.insert(peer.public_key),"wireguard_duplicate_peer");},
                "PresharedKey"=>peer.preshared_key=Some(Zeroizing::new(key(value)?)),
                "AllowedIPs"=>for item in value.split(',') {peer.allowed_ips.push(cidr(item.trim(),true)?);},
                "Endpoint"=>{endpoint(value)?;peer.endpoint=value.into();},
                "PersistentKeepalive"=>peer.keepalive=value.parse().map_err(|_|anyhow::anyhow!("wireguard_invalid_keepalive"))?,
                _=>bail!("wireguard_unsupported_field"),
            }},
            _=>bail!("wireguard_unsupported_field"),
        }
    }
    ensure!(interface && config.private_key.iter().any(|v|*v!=0) && !config.addresses.is_empty() && config.addresses.len()<=8 && config.dns.len()<=8 && !config.peers.is_empty(),"wireguard_missing_interface_fields");
    ensure!(config.search_domain.is_empty() || !config.dns.is_empty(),"wireguard_search_domain_requires_dns");
    let mut routes=0;
    for peer in &config.peers {ensure!(peer.public_key.iter().any(|v|*v!=0) && !peer.allowed_ips.is_empty() && !peer.endpoint.is_empty(),"wireguard_peer_requires_key_routes_endpoint");routes+=peer.allowed_ips.len();}
    ensure!(routes<=1024,"wireguard_too_many_routes");
    let unique=config.addresses.iter().collect::<BTreeSet<_>>();ensure!(unique.len()==config.addresses.len(),"wireguard_duplicate_address");
    Ok(config)
}
fn hex(key: &[u8;32]) -> String {use std::fmt::Write;let mut value=String::with_capacity(64);for byte in key {write!(value,"{byte:02x}").unwrap();}value}
impl Config {
    /// Resolve once before network reservation; no keys, files or commands cross argv.
    pub fn engine_config(&self) -> Result<Value> {
        let endpoints=self.peers.iter().map(|peer| {
            let mut choices=peer.endpoint.to_socket_addrs().map_err(|_|anyhow::anyhow!("wireguard_endpoint_resolution_failed"))?.collect::<Vec<_>>();
            ensure!(!choices.is_empty() && choices.len()<=64 && choices.iter().all(SocketAddr::is_ipv4),"wireguard_ipv6_endpoint_unsupported");
            choices.sort(); choices.dedup();let chosen=choices[0];
            ensure!(!chosen.ip().is_unspecified() && !chosen.ip().is_multicast() && chosen.ip()!=IpAddr::V4(Ipv4Addr::BROADCAST),"wireguard_invalid_endpoint");Ok(chosen)
        }).collect::<Result<Vec<_>>>()?;
        self.engine_config_pinned(&endpoints)
    }
    pub fn engine_config_pinned(&self,endpoints:&[SocketAddr]) -> Result<Value> {
        ensure!(endpoints.len()==self.peers.len() && endpoints.iter().all(|e|e.is_ipv4() && e.port()>0 && !e.ip().is_unspecified() && !e.ip().is_multicast() && e.ip()!=IpAddr::V4(Ipv4Addr::BROADCAST)),"wireguard_invalid_pinned_endpoints");
        ensure!(self.addresses.iter().chain(self.peers.iter().flat_map(|p|&p.allowed_ips)).all(|value|!value.contains(':')) && self.dns.iter().all(IpAddr::is_ipv4),"wireguard_ipv6_unsupported");
        for server in &self.dns {
            let IpAddr::V4(server)=server else {unreachable!()};
            ensure!(self.peers.iter().flat_map(|p|&p.allowed_ips).any(|route|{
                let (ip,prefix)=route.split_once('/').unwrap();let prefix:u8=prefix.parse().unwrap();let mask=if prefix==0{0}else{u32::MAX<<(32-prefix)};
                u32::from(*server)&mask==u32::from(ip.parse::<Ipv4Addr>().unwrap())&mask
            }),"wireguard_dns_not_in_allowed_ips");
        }
        let mut uapi=Zeroizing::new(format!("private_key={}\nlisten_port={}\nreplace_peers=true\n",hex(&self.private_key),self.listen_port));
        for (peer,endpoint) in self.peers.iter().zip(endpoints) {
            uapi.push_str(&format!("public_key={}\n",hex(&peer.public_key)));
            if let Some(key)=&peer.preshared_key {uapi.push_str(&format!("preshared_key={}\n",hex(key)));}
            uapi.push_str(&format!("endpoint={endpoint}\npersistent_keepalive_interval={}\nreplace_allowed_ips=true\n",peer.keepalive));
            for route in &peer.allowed_ips {uapi.push_str(&format!("allowed_ip={route}\n"));}
        }
        let plan=json!({"addresses":self.addresses.iter().map(|cidr|json!({"cidr":cidr,"gateway":""})).collect::<Vec<_>>(),"routes":self.peers.iter().flat_map(|p|&p.allowed_ips).collect::<Vec<_>>(),"dns":self.dns,"searchDomain":self.search_domain,"remote":endpoints[0].ip().to_string(),"excludedRoutes":endpoints.iter().map(|e|format!("{}/32",e.ip())).collect::<BTreeSet<_>>(),"mtu":self.mtu});
        Ok(json!({"uapi":&*uapi,"networkPlan":plan,"endpoints":endpoints.iter().map(ToString::to_string).collect::<Vec<_>>(),"peerCount":self.peers.len()}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid()->String {format!("[Interface]\nPrivateKey = {}\nAddress = 10.97.0.2/32\nDNS = 10.97.0.1, corp.example\n[Peer]\nPublicKey = {}\nEndpoint = 127.0.0.1:31820\nAllowedIPs = 10.97.0.0/24\nPersistentKeepalive = 25\n",STANDARD.encode([1;32]),STANDARD.encode([2;32]))}
    #[test]
    fn validates_full_configuration_and_pins_without_paths_or_hooks() {
        let config=parse(&valid()).unwrap();let engine=config.engine_config().unwrap();assert_eq!(engine["endpoints"][0],"127.0.0.1:31820");assert_eq!(engine["networkPlan"]["dns"][0],"10.97.0.1");assert_eq!(engine["peerCount"],1);assert!(engine["uapi"].as_str().unwrap().contains("allowed_ip=10.97.0.0/24\n"));
        for (from,to) in [("Address = 10.97.0.2/32","Address = 10.1.0.1/33"),("10.97.0.0/24","not-a-route"),("31820","65536"),("PersistentKeepalive = 25","PersistentKeepalive = 65536"),("Address = 10.97.0.2/32","PostUp = arbitrary"),("DNS = 10.97.0.1, corp.example","DNS = corp.example"),("[Peer]","[Unexpected]")] {assert!(parse(&valid().replace(from,to)).is_err(),"{to}");}
        assert!(parse(&(valid()+"PrivateKey = secret\n")).is_err());
        assert!(parse(&valid().replace(&STANDARD.encode([1;32]),&STANDARD.encode([0;32]))).is_err());
        assert!(parse(&valid().replace("Address = 10.97.0.2/32","Address = fd00::2/64")).err().unwrap().to_string().contains("ipv6"));
        assert!(parse(&valid().replace("10.97.0.0/24","::/0")).is_err());
        assert!(parse(&valid().replace("127.0.0.1:31820","[::1]:31820")).is_err());
        assert!(parse(&valid().replace("10.97.0.1, corp.example","2001:db8::53")).is_err());
        assert!(parse(&(valid()+"PersistentKeepalive = 20\n")).is_err());
        assert!(parse(&valid().replace("10.97.0.1, corp.example","10.98.0.53")).unwrap().engine_config().is_err());
    }
}
