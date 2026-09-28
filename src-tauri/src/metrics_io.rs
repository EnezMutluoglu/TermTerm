//! Per-device throughput. Never sum stacked interfaces, partitions or mounts.
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const LINUX: &str = r#"LC_ALL=C; export LC_ALL
printf 'TTTIME '; cut -d ' ' -f 1 /proc/uptime
echo TTNETWORK
cat /proc/net/dev 2>/dev/null || echo TTERROR
printf 'TTDEFAULT '; awk '$2 == "00000000" {print $1; exit}' /proc/net/route 2>/dev/null
echo; echo TTDISKS
for p in /sys/block/*; do
 n=${p##*/}; case "$n" in loop*|ram*|zram*|dm-*|md*) continue;; esac
 [ -r "$p/stat" ] || continue
 printf '%s ' "$n"; cat "$p/stat"
done"#;
pub const WINDOWS: &str = r#"$ErrorActionPreference='Stop';$n=@();$d=@();$ne=$null;$de=$null
try{$n=@(Get-CimInstance Win32_PerfRawData_Tcpip_NetworkInterface|Where-Object{$_.Name -notmatch 'Loopback'}|ForEach-Object{@{name=$_.Name;read=[uint64]$_.BytesReceivedPersec;write=[uint64]$_.BytesSentPersec;time=([double]$_.Timestamp_PerfTime/[double]$_.Frequency_PerfTime)}})}catch{$ne=$_.Exception.Message}
try{$d=@(Get-CimInstance Win32_PerfRawData_PerfDisk_PhysicalDisk|Where-Object{$_.Name -ne '_Total'}|ForEach-Object{@{name=$_.Name;read=[uint64]$_.DiskReadBytesPersec;write=[uint64]$_.DiskWriteBytesPersec;time=([double]$_.Timestamp_PerfTime/[double]$_.Frequency_PerfTime)}})}catch{$de=$_.Exception.Message}
@{network=$n;disks=$d;networkError=$ne;diskError=$de}|ConvertTo-Json -Depth 4 -Compress"#;
pub const MAC: &str = r#"LC_ALL=C; export LC_ALL
echo TTNETWORK
netstat -ibn 2>/dev/null || echo TTERROR
printf 'TTDEFAULT '; route -n get default 2>/dev/null | awk '/interface:/ {print $2; exit}'
echo; echo TTDISKS
ioreg -r -c IOBlockStorageDriver -l -w 0 2>/dev/null || echo TTERROR"#;

#[derive(Clone, Debug, Deserialize)]
pub struct Counter {
    name: String,
    read: u64,
    write: u64,
    #[serde(default)]
    time: f64,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counters {
    network: Vec<Counter>,
    disks: Vec<Counter>,
    #[serde(default)]
    network_error: Option<String>,
    #[serde(default)]
    disk_error: Option<String>,
    #[serde(default)]
    default_interface: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rate {
    pub name: String,
    pub read_bytes_per_second: Option<f64>,
    pub write_bytes_per_second: Option<f64>,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub network: Vec<Rate>,
    pub disks: Vec<Rate>,
    pub network_error: Option<String>,
    pub disk_error: Option<String>,
    pub default_interface: Option<String>,
}
#[derive(Default)]
pub struct Sampler {
    network: HashMap<String, Counter>,
    disks: HashMap<String, Counter>,
}
impl Sampler {
    pub fn sample(&mut self, c: Counters) -> Activity {
        fn rates(counters: Vec<Counter>, old: &mut HashMap<String, Counter>) -> Vec<Rate> {
            let mut next = HashMap::new();
            let mut result = Vec::new();
            for c in counters {
                let rate = |value: u64, previous: u64, time: f64| {
                    let elapsed = c.time - time;
                    if elapsed.is_finite() && elapsed > 0. && elapsed < 30. {
                        value
                            .checked_sub(previous)
                            .map(|delta| delta as f64 / elapsed)
                    } else {
                        None
                    }
                };
                let previous = old.get(&c.name);
                result.push(Rate {
                    name: c.name.clone(),
                    read_bytes_per_second: previous.and_then(|p| rate(c.read, p.read, p.time)),
                    write_bytes_per_second: previous.and_then(|p| rate(c.write, p.write, p.time)),
                });
                next.insert(c.name.clone(), c);
            }
            *old = next; // Removed devices lose their baseline; reappearance cannot spike.
            result.sort_by(|a, b| a.name.cmp(&b.name));
            result
        }
        Activity {
            network: rates(c.network, &mut self.network),
            disks: rates(c.disks, &mut self.disks),
            network_error: c.network_error,
            disk_error: c.disk_error,
            default_interface: c.default_interface,
        }
    }
}

pub fn parse(text: &str, os: &str, monotonic_seconds: f64) -> Result<Counters> {
    if os == "windows" {
        let mut c: Counters = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
        validate(&mut c);
        return Ok(c);
    }
    let (network, disks) = text
        .split_once("TTDISKS\n")
        .ok_or_else(|| anyhow!("Missing I/O counters"))?;
    let mut c = Counters::default();
    c.default_interface = network
        .lines()
        .find_map(|l| l.strip_prefix("TTDEFAULT "))
        .filter(|v| !v.trim().is_empty())
        .map(|v| v.trim().to_string());
    let time = if os == "linux" {
        network
            .lines()
            .find_map(|l| l.strip_prefix("TTTIME "))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .ok_or_else(|| anyhow!("Missing monotonic counter time"))?
    } else {
        monotonic_seconds
    };
    if os == "linux" {
        for line in network.lines().filter(|l| l.contains(':')) {
            let (name, fields) = line.rsplit_once(':').unwrap();
            let name = name.trim();
            if name == "lo" {
                continue;
            }
            let f: Vec<_> = fields.split_whitespace().collect();
            match (
                f.first().and_then(|v| v.parse().ok()),
                f.get(8).and_then(|v| v.parse().ok()),
            ) {
                (Some(read), Some(write)) => c.network.push(Counter {
                    name: name.into(),
                    read,
                    write,
                    time,
                }),
                _ => c.network_error = Some("Malformed network counter".into()),
            }
        }
        for line in disks.lines() {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.is_empty() {
                continue;
            }
            // /sys/block/<whole-device>/stat, sectors are always 512 bytes.
            match (
                f.get(3)
                    .and_then(|v| v.parse::<u64>().ok())
                    .and_then(|v| v.checked_mul(512)),
                f.get(7)
                    .and_then(|v| v.parse::<u64>().ok())
                    .and_then(|v| v.checked_mul(512)),
            ) {
                (Some(read), Some(write)) => c.disks.push(Counter {
                    name: f[0].into(),
                    read,
                    write,
                    time,
                }),
                _ => c.disk_error = Some("Malformed disk I/O counter".into()),
            }
        }
    } else {
        let mut columns = None;
        for line in network.lines() {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.first() == Some(&"Name") {
                columns = f
                    .iter()
                    .position(|v| *v == "Ibytes")
                    .zip(f.iter().position(|v| *v == "Obytes"));
                continue;
            }
            if !f.iter().any(|v| v.starts_with("<Link#")) || f.first() == Some(&"lo0") {
                continue;
            }
            if let Some((rx, tx)) = columns {
                if let (Some(read), Some(write)) = (
                    f.get(rx).and_then(|v| v.parse().ok()),
                    f.get(tx).and_then(|v| v.parse().ok()),
                ) {
                    c.network.push(Counter {
                        name: f[0].trim_end_matches('*').into(),
                        read,
                        write,
                        time,
                    });
                } else {
                    c.network_error = Some("Malformed network counter".into());
                }
            }
        }
        let mut name = None;
        for line in disks.lines() {
            if let Some((_, tail)) = line.split_once("+-o ") {
                name = tail.split_once(" <class").map(|(n, _)| n.to_string());
                if let Some(id) = tail.split("id ").nth(1).and_then(|s| s.split(',').next()) {
                    name = name.map(|n| format!("{n} ({id})"));
                }
            }
            if line.contains("\"Statistics\"") {
                let value = |key: &str| {
                    line.split(key)
                        .nth(1)?
                        .split_once('=')?
                        .1
                        .trim_start()
                        .split(|c: char| !c.is_ascii_digit())
                        .next()?
                        .parse::<u64>()
                        .ok()
                };
                if let (Some(name), Some(read), Some(write)) = (
                    name.clone(),
                    value("\"Bytes (Read)\""),
                    value("\"Bytes (Write)\""),
                ) {
                    c.disks.push(Counter {
                        name,
                        read,
                        write,
                        time,
                    });
                }
            }
        }
    }
    validate(&mut c);
    Ok(c)
}
fn validate(c: &mut Counters) {
    for (values, error, label) in [
        (
            &mut c.network,
            &mut c.network_error,
            "Network counters unavailable",
        ),
        (
            &mut c.disks,
            &mut c.disk_error,
            "Disk I/O counters unavailable",
        ),
    ] {
        values.retain(|v| !v.name.is_empty() && v.time.is_finite() && v.time >= 0.);
        let mut names = std::collections::HashSet::new();
        values.retain(|v| names.insert(v.name.clone()));
        if values.is_empty() && error.is_none() {
            *error = Some(label.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn linux(t: u64, rx: u64, tx: u64, sectors: u64) -> String {
        format!("TTTIME {t}\nTTNETWORK\n lo: 500 0 0 0 0 0 0 0 500 0 0 0 0 0 0 0\n eth0: {rx} 0 0 0 0 0 0 0 {tx} 0 0 0 0 0 0 0\nTTDEFAULT eth0\nTTDISKS\nnvme0n1 1 0 {sectors} 0 2 0 {} 0 0 0 0\n",sectors*2)
    }
    #[test]
    fn linux_rates_reset_hotplug_and_missing_are_not_zero() {
        let mut s = Sampler::default();
        let a = s.sample(parse(&linux(10, 100, 200, 100), "linux", 0.).unwrap());
        assert!(a.network[0].read_bytes_per_second.is_none());
        assert_eq!(a.network.len(), 1);
        let a = s.sample(parse(&linux(12, 2_000_100, 4_000_200, 4196), "linux", 0.).unwrap());
        assert_eq!(a.network[0].read_bytes_per_second, Some(1_000_000.));
        assert_eq!(a.network[0].write_bytes_per_second, Some(2_000_000.));
        assert_eq!(a.disks[0].read_bytes_per_second, Some(1_048_576.));
        assert_eq!(a.disks[0].write_bytes_per_second, Some(2_097_152.));
        let a = s.sample(parse(&linux(14, 1, 2, 1), "linux", 0.).unwrap());
        assert!(a.network[0].read_bytes_per_second.is_none());
        s.sample(Counters::default());
        let a = s.sample(parse(&linux(15, 10, 20, 10), "linux", 0.).unwrap());
        assert!(a.disks[0].read_bytes_per_second.is_none());
        let a = s.sample(parse(&linux(50, 100, 200, 100), "linux", 0.).unwrap());
        assert!(a.disks[0].read_bytes_per_second.is_none());
        let c = parse("TTTIME 10\nTTNETWORK\nTTERROR\nTTDISKS\n", "linux", 0.).unwrap();
        assert!(c.network_error.is_some() && c.disk_error.is_some());
        assert!(parse("bad", "linux", 0.).is_err());
    }
    #[test]
    fn windows_and_mac_samples_are_separate_devices() {
        let c=parse(r#"{"network":[{"name":"Ethernet","read":100,"write":200,"time":1}],"disks":[{"name":"0 C:","read":200,"write":300,"time":1}]}"#,"windows",0.).unwrap();
        assert_eq!(c.disks.len(), 1);
        let c=parse("TTNETWORK\nName Mtu Network Address Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll\nen0 1500 <Link#4> aa:bb 1 0 123 2 0 456 0\nen0 1500 10.0.0 10.0.0.1 1 0 123 2 0 456 0\nTTDEFAULT en0\nTTDISKS\n+-o AppleNVMe <class IOBlockStorageDriver, id 0x123, registered>\n  \"Statistics\" = {\"Bytes (Read)\"=1000,\"Bytes (Write)\"=2000}\n","macos",10.).unwrap();
        assert_eq!(c.network.len(), 1);
        assert_eq!(c.network[0].read, 123);
        assert_eq!(c.disks[0].write, 2000);
    }
}
