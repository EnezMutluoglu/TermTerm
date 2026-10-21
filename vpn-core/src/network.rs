use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::IpAddr, str::FromStr};

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
    for domain in &next.dns_domains {
        let domain = domain.trim_end_matches('.');
        ensure!(
            domain.len() <= 253
                && domain.split('.').all(|label| !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')),
            "Geçersiz DNS alanı"
        );
    }
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

/// Pending reservations count as active: simultaneous handshakes cannot both
/// approve overlapping routes. Release only after the OS cleanup has completed.
#[derive(Default)]
pub struct Reservations {
    entries: HashMap<String, (u64, NetworkPlan)>,
}
impl Reservations {
    pub fn reserve(
        &mut self,
        id: &str,
        generation: u64,
        plan: NetworkPlan,
        split_dns: bool,
    ) -> Result<()> {
        ensure!(
            !id.is_empty() && id.len() <= 128 && generation > 0,
            "Geçersiz VPN oturumu"
        );
        ensure!(
            !self.entries.contains_key(id),
            "VPN zaten bağlanıyor veya bağlı"
        );
        ensure!(self.entries.len() < 16, "Aynı anda en çok 16 VPN");
        let active: Vec<_> = self
            .entries
            .values()
            .map(|(_, plan)| plan.clone())
            .collect();
        check_conflicts(&plan, &active, split_dns)?;
        self.entries.insert(id.into(), (generation, plan));
        Ok(())
    }
    pub fn release(&mut self, id: &str, generation: u64) -> bool {
        if self
            .entries
            .get(id)
            .is_some_and(|(current, _)| *current == generation)
        {
            self.entries.remove(id);
            true
        } else {
            false
        }
    }
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

    #[test]
    fn third_vpn_checks_every_active_network_without_changing_them() {
        let active = vec![
            plan(&["10.10.0.0/16"]),
            plan(&["10.20.0.0/16", "fd20::/48"]),
        ];
        let before = serde_json::to_value(&active).unwrap();
        assert!(check_conflicts(&plan(&["10.30.0.0/16", "fd30::/48"]), &active, false).is_ok());
        for conflict in [
            "10.20.5.0/24",
            "10.10.0.0/16",
            "fd20:0:0:1::/64",
            "0.0.0.0/0",
        ] {
            assert!(check_conflicts(&plan(&[conflict]), &active, false).is_err());
        }
        assert_eq!(serde_json::to_value(&active).unwrap(), before);
    }
    #[test]
    fn reservations_cover_handshakes_and_stale_disconnects() {
        let mut registry = Reservations::default();
        registry
            .reserve("one", 1, plan(&["10.1.0.0/16"]), false)
            .unwrap();
        assert!(registry
            .reserve("two", 1, plan(&["10.1.2.0/24"]), false)
            .is_err());
        registry
            .reserve("two", 1, plan(&["10.2.0.0/16"]), false)
            .unwrap();
        registry
            .reserve("three", 1, plan(&["10.3.0.0/16"]), false)
            .unwrap();
        assert!(!registry.release("one", 2));
        assert!(registry.release("one", 1));
        registry
            .reserve("one", 2, plan(&["10.1.0.0/16"]), false)
            .unwrap();
        assert!(!registry.release("one", 1));
        assert!(registry
            .reserve("four", 1, plan(&["10.1.0.0/16"]), false)
            .is_err());
        assert_eq!(registry.entries.len(), 3);
    }
    #[test]
    fn malformed_dns_is_rejected_even_for_the_first_vpn() {
        for domain in ["", ".", "-corp.example", "a..b", "corp example", "~."] {
            let mut p = plan(&[]);
            p.dns_domains.push(domain.into());
            assert!(check_conflicts(&p, &[], true).is_err());
        }
    }
    #[test]
    fn simultaneous_handshakes_reserve_a_network_exactly_once() {
        use std::sync::{Arc, Barrier, Mutex};
        let registry = Arc::new(Mutex::new(Reservations::default()));
        let start = Arc::new(Barrier::new(3));
        let handles: Vec<_> = (1..=3)
            .map(|id| {
                let registry = registry.clone();
                let start = start.clone();
                std::thread::spawn(move || {
                    start.wait();
                    registry
                        .lock()
                        .unwrap()
                        .reserve(&id.to_string(), 1, plan(&["10.7.0.0/16"]), false)
                        .is_ok()
                })
            })
            .collect();
        assert_eq!(
            handles
                .into_iter()
                .filter_map(|h| h.join().ok())
                .filter(|ok| *ok)
                .count(),
            1
        );
    }
}
