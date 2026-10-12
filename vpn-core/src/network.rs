use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use std::{net::IpAddr, str::FromStr};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkPlan {
    pub routes: Vec<String>,
    pub dns_servers: Vec<IpAddr>,
    pub dns_domains: Vec<String>,
}
#[derive(Debug)]
struct Prefix {
    network: u128,
    bits: u32,
    v6: bool,
}
impl Prefix {
    fn parse(text: &str) -> Result<Self> {
        let (address, bits) = text
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("CIDR gerekli"))?;
        let ip = IpAddr::from_str(address)?;
        let v6 = ip.is_ipv6();
        let max = if v6 { 128 } else { 32 };
        let bits: u32 = bits.parse()?;
        ensure!(bits <= max, "Geçersiz CIDR");
        let value = match ip {
            IpAddr::V4(a) => u32::from(a) as u128,
            IpAddr::V6(a) => u128::from(a),
        };
        let mask = if bits == 0 {
            0
        } else {
            u128::MAX << (max - bits)
        };
        Ok(Self {
            network: value & mask,
            bits,
            v6,
        })
    }
    fn overlaps(&self, other: &Self) -> bool {
        if self.v6 != other.v6 {
            return false;
        }
        let bits = self.bits.min(other.bits);
        let max = if self.v6 { 128 } else { 32 };
        let mask = if bits == 0 {
            0
        } else {
            u128::MAX << (max - bits)
        };
        self.network & mask == other.network & mask
    }
}
/// Conservative first implementation: refuse ambiguous routes instead of silently hijacking traffic.
pub fn check_conflicts(next: &NetworkPlan, active: &[NetworkPlan], split_dns: bool) -> Result<()> {
    ensure!(
        next.routes.len() <= 1024 && next.dns_servers.len() <= 8 && next.dns_domains.len() <= 64,
        "Ağ planı çok büyük"
    );
    let routes: Vec<_> = next
        .routes
        .iter()
        .map(|r| Prefix::parse(r))
        .collect::<Result<_>>()?;
    for existing in active {
        for route in &existing.routes {
            let old = Prefix::parse(route)?;
            if routes.iter().any(|r| r.overlaps(&old)) {
                bail!("VPN rotaları çakışıyor: {route}");
            }
        }
        if !next.dns_servers.is_empty() && !existing.dns_servers.is_empty() {
            ensure!(
                split_dns && !next.dns_domains.is_empty() && !existing.dns_domains.is_empty(),
                "VPN DNS politikaları çakışıyor"
            );
            for a in &next.dns_domains {
                for b in &existing.dns_domains {
                    let a = a.trim_end_matches('.').to_lowercase();
                    let b = b.trim_end_matches('.').to_lowercase();
                    ensure!(
                        !a.is_empty()
                            && !b.is_empty()
                            && a != b
                            && !a.ends_with(&format!(".{b}"))
                            && !b.ends_with(&format!(".{a}")),
                        "VPN DNS alanları çakışıyor"
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(routes: &[&str]) -> NetworkPlan {
        NetworkPlan {
            routes: routes.iter().map(|s| s.to_string()).collect(),
            dns_servers: vec![],
            dns_domains: vec![],
        }
    }
    #[test]
    fn ipv4_ipv6_overlap_and_default_routes_are_rejected() {
        let a = plan(&["10.12.0.0/16", "fd12::/48"]);
        assert!(check_conflicts(&plan(&["10.13.0.0/16"]), &[a.clone()], false).is_ok());
        for route in ["10.12.8.0/24", "0.0.0.0/0", "fd12::1/128", "::/0"] {
            assert!(check_conflicts(&plan(&[route]), &[a.clone()], false).is_err());
        }
        assert!(check_conflicts(&plan(&["10.12.0.0/33"]), &[], false).is_err());
    }
    #[test]
    fn split_dns_requires_distinct_domains() {
        let mut a = plan(&["10.0.0.0/24"]);
        a.dns_servers.push("10.0.0.1".parse().unwrap());
        a.dns_domains.push("office.example".into());
        let mut b = plan(&["10.1.0.0/24"]);
        b.dns_servers.push("10.1.0.1".parse().unwrap());
        b.dns_domains.push("other.example".into());
        assert!(check_conflicts(&b, &[a.clone()], false).is_err());
        assert!(check_conflicts(&b, &[a.clone()], true).is_ok());
        b.dns_domains = vec!["SUB.OFFICE.EXAMPLE.".into()];
        assert!(check_conflicts(&b, &[a], true).is_err());
    }
}
