//! Read-only, bounded probes on a separate SSH channel. No terminal input or logs.
use crate::{connections::SshConnection, state::Shared};
use anyhow::{anyhow, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::{watch, OnceCell};

const LINUX_FAST: &str = "LC_ALL=C; export LC_ALL; head -n 1 /proc/stat; cat /proc/meminfo";
const MAC_FAST: &str = "LC_ALL=C; export LC_ALL; top -l 2 -s 1 -n 0 | grep 'CPU usage' | tail -n 1; echo TTMEM; sysctl -n hw.memsize; vm_stat";
const WINDOWS_FAST: &str = "$ErrorActionPreference='Stop';$o=Get-CimInstance Win32_OperatingSystem;$c=Get-CimInstance Win32_PerfFormattedData_PerfOS_Processor -Filter \"Name='_Total'\";@{cpu=$c.PercentProcessorTime;total=[uint64]$o.TotalVisibleMemorySize*1024;available=[uint64]$o.FreePhysicalMemory*1024}|ConvertTo-Json -Compress";
const WINDOWS_DISKS: &str = "$ErrorActionPreference='Stop';$r=@();foreach($v in Get-CimInstance Win32_Volume){if($null -eq $v.Capacity -or $v.Capacity -eq 0){continue};$paths=@($v.Name);foreach($m in @(Get-CimAssociatedInstance -InputObject $v -Association Win32_MountPoint -ErrorAction SilentlyContinue)){if($m.Name){$paths+=($m.Name.TrimEnd('\\')+'\\')}};foreach($p in ($paths|Select-Object -Unique)){$r+=@{mountPoint=$p;device=$v.DeviceID;filesystem=$v.FileSystem;total=[uint64]$v.Capacity;used=([uint64]$v.Capacity-[uint64]$v.FreeSpace);available=[uint64]$v.FreeSpace;root=($p -eq ($env:SystemDrive+'\\'));virtual=$false}}};foreach($v in Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=4'){$r+=@{mountPoint=$v.DeviceID+'\\';device=$v.ProviderName;filesystem=$v.FileSystem;total=[uint64]$v.Size;used=([uint64]$v.Size-[uint64]$v.FreeSpace);available=[uint64]$v.FreeSpace;root=$false;virtual=$false}};ConvertTo-Json -InputObject @($r) -Compress -Depth 4";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub mount_point: String,
    pub device: String,
    pub filesystem: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    #[serde(default)]
    pub root: bool,
    #[serde(default, rename = "virtual")]
    pub virtual_: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub total: u64,
    pub used: u64,
    pub available: u64,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub session_id: String,
    pub source: String,
    pub os: String,
    pub timestamp: i64,
    pub cpu_percent: Option<f64>,
    pub memory: Option<Memory>,
    pub disks: Vec<Disk>,
    pub sampled_at: Option<i64>,
    pub disks_at: Option<i64>,
    pub error: Option<String>,
    pub disk_error: Option<String>,
    pub supported: bool,
}
#[derive(Clone)]
pub enum Target {
    Ssh(Arc<SshConnection>, String),
    Local,
    Wsl,
    Unsupported(String),
}
pub struct Monitor {
    enabled: watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<()>,
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.task.abort();
    }
}
struct Runner {
    target: Target,
    os: OnceCell<String>,
    windows_ready: std::sync::atomic::AtomicBool,
}
impl Runner {
    async fn command(&self, command: &str, windows: bool) -> Result<String> {
        // CIM/PowerShell startup under load needs a separate initial budget.
        // Sampling loops await each probe; a timeout cannot accumulate new probes.
        let cold = windows
            && !self
                .windows_ready
                .load(std::sync::atomic::Ordering::Acquire);
        let budget = Duration::from_secs(if cold {
            20
        } else if windows {
            8
        } else {
            5
        });
        let raw_command = command;
        let command = if windows {
            let bytes: Vec<u8> = command.encode_utf16().flat_map(u16::to_le_bytes).collect();
            format!(
                "powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand {}",
                STANDARD.encode(bytes)
            )
        } else {
            command.into()
        };
        match &self.target {
            Target::Ssh(ssh, _) => {
                let channel =
                    tokio::time::timeout(Duration::from_secs(5), ssh.handle.channel_open_session())
                        .await
                        .context("Metrics channel timed out")??;
                let mut guard = ChannelGuard(Some(channel));
                let channel = guard.0.as_mut().unwrap();
                let result = tokio::time::timeout(budget, async {
                    channel.exec(true, command.as_str()).await?;
                    let mut out = Vec::new();
                    let mut error = Vec::new();
                    let mut status = 0;
                    while let Some(msg) = channel.wait().await {
                        match msg {
                            ChannelMsg::Data { data } => {
                                ensure!(
                                    out.len() + data.len() <= 512 * 1024,
                                    "Metrics response too large"
                                );
                                out.extend_from_slice(&data);
                            }
                            ChannelMsg::ExtendedData { data, .. } => {
                                if error.len() < 4096 {
                                    error.extend_from_slice(
                                        &data[..data.len().min(4096 - error.len())],
                                    );
                                }
                            }
                            ChannelMsg::ExitStatus { exit_status } => status = exit_status,
                            ChannelMsg::Close => break,
                            _ => {}
                        }
                    }
                    ensure!(
                        status == 0,
                        "Probe failed: {}",
                        String::from_utf8_lossy(&error)
                    );
                    Ok::<_, anyhow::Error>(String::from_utf8_lossy(&out).into_owned())
                })
                .await
                .context("Metrics probe timed out")?;
                if let Some(c) = guard.0.take() {
                    let _ = c.close().await;
                }
                if windows && result.is_ok() {
                    self.windows_ready
                        .store(true, std::sync::atomic::Ordering::Release);
                }
                result
            }
            Target::Local | Target::Wsl => {
                let mut cmd = if matches!(self.target, Target::Wsl) {
                    let mut c = tokio::process::Command::new("wsl.exe");
                    c.args(["--exec", "sh", "-c", &command]);
                    c
                } else if windows {
                    let mut c = tokio::process::Command::new("powershell.exe");
                    let bytes: Vec<u8> = raw_command
                        .encode_utf16()
                        .flat_map(u16::to_le_bytes)
                        .collect();
                    c.args([
                        "-NoLogo",
                        "-NoProfile",
                        "-NonInteractive",
                        "-EncodedCommand",
                        &STANDARD.encode(bytes),
                    ]);
                    c
                } else {
                    let mut c = tokio::process::Command::new("/bin/sh");
                    c.args(["-c", &command]);
                    c
                };
                #[cfg(windows)]
                cmd.creation_flags(0x08000000);
                cmd.kill_on_drop(true);
                let output = tokio::time::timeout(budget, cmd.output())
                    .await
                    .context("Local probe timed out")??;
                ensure!(
                    output.status.success(),
                    "Probe unavailable: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                ensure!(
                    output.stdout.len() <= 512 * 1024,
                    "Metrics response too large"
                );
                if windows {
                    self.windows_ready
                        .store(true, std::sync::atomic::Ordering::Release);
                }
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            }
            Target::Unsupported(reason) => anyhow::bail!("{reason}"),
        }
    }
    async fn os(&self) -> Result<&str> {
        Ok(self
            .os
            .get_or_try_init(|| async {
                match &self.target {
                    Target::Local => Ok(std::env::consts::OS.into()),
                    Target::Wsl => Ok("linux".into()),
                    Target::Unsupported(reason) => Err(anyhow!(reason.clone())),
                    _ => {
                        if let Ok(v) = self.command("uname -s", false).await {
                            match v.trim() {
                                "Linux" => return Ok("linux".into()),
                                "Darwin" => return Ok("macos".into()),
                                _ => {}
                            }
                        }
                        let v = self
                            .command("[Environment]::OSVersion.Platform.ToString()", true)
                            .await?;
                        ensure!(
                            v.trim() == "Win32NT",
                            "This remote operating system is not supported"
                        );
                        Ok("windows".into())
                    }
                }
            })
            .await?
            .as_str())
    }
}
struct ChannelGuard(Option<russh::Channel<russh::client::Msg>>);
impl Drop for ChannelGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.take() {
            tauri::async_runtime::spawn(async move {
                let _ = c.signal(russh::Sig::TERM).await;
                let _ = c.close().await;
            });
        }
    }
}

pub fn register(app: &AppHandle, state: &Shared, id: &str, target: Target) {
    stop(state, id);
    let (enabled, rx) = watch::channel(false);
    let app = app.clone();
    let id = id.to_string();
    let source = match &target {
        Target::Ssh(_, label) => label.clone(),
        Target::Wsl => "Local WSL".into(),
        Target::Local => "This computer".into(),
        Target::Unsupported(s) => s.clone(),
    };
    let supported = !matches!(target, Target::Unsupported(_));
    let snapshot = Arc::new(Mutex::new(Snapshot {
        session_id: id.clone(),
        source,
        supported,
        ..Default::default()
    }));
    let runner = Arc::new(Runner {
        target,
        os: OnceCell::new(),
        windows_ready: std::sync::atomic::AtomicBool::new(false),
    });
    let task_id = id.clone();
    let task = tauri::async_runtime::spawn(async move {
        tokio::join!(
            sample_loop(
                app.clone(),
                runner.clone(),
                snapshot.clone(),
                rx.clone(),
                false
            ),
            sample_loop(app, runner, snapshot, rx, true)
        );
    });
    if let Ok(mut m) = state.metrics.lock() {
        m.insert(task_id, Monitor { enabled, task });
    }
}
pub fn set(app: &AppHandle, state: &Shared, id: &str, enabled: bool) -> Result<()> {
    let m = state.metrics.lock().map_err(|_| anyhow!("Metrics lock"))?;
    if let Some(m) = m.get(id) {
        m.enabled.send_replace(enabled);
    } else if enabled {
        app.emit(
            "session-metrics",
            Snapshot {
                session_id: id.into(),
                source: "Shared or unsupported session".into(),
                error: Some("No resource source is attached to this terminal".into()),
                ..Default::default()
            },
        )?;
    }
    Ok(())
}
pub fn stop(state: &Shared, id: &str) {
    if let Ok(mut m) = state.metrics.lock() {
        m.remove(id);
    }
}
pub fn stop_all(state: &Shared) {
    if let Ok(mut m) = state.metrics.lock() {
        m.clear();
    }
}

async fn sample_loop(
    app: AppHandle,
    runner: Arc<Runner>,
    snapshot: Arc<Mutex<Snapshot>>,
    mut enabled: watch::Receiver<bool>,
    disks: bool,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(if disks { 10 } else { 2 }));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut previous = None;
    let mut system = sysinfo::System::new();
    let mut native_primed = false;
    loop {
        if !*enabled.borrow() {
            previous = None;
            native_primed = false;
            if enabled.changed().await.is_err() {
                break;
            }
            tick.reset_immediately();
            continue;
        }
        tokio::select! {_ = tick.tick()=>{},changed=enabled.changed()=>{if changed.is_err(){break;}tick.reset_immediately();continue;}}
        if !*enabled.borrow() {
            continue;
        }
        let result = tokio::select! {changed=enabled.changed()=>{if changed.is_err(){break;}continue;},result=async {
            let os=runner.os().await?.to_owned();
            if disks {
                let command=match os.as_str(){"windows"=>WINDOWS_DISKS,"linux"=>"LC_ALL=C; export LC_ALL; if command -v timeout >/dev/null 2>&1; then timeout 4 df -P -k -T; else df -P -k -T; fi",_=>"LC_ALL=C; export LC_ALL; df -P -k; echo TTMOUNTS; mount"};
                let text=runner.command(command,os=="windows").await?;
                let mut values=if os=="windows"{serde_json::from_str::<Vec<Disk>>(&text)?}else{parse_disks(&text,&os)?};
                values.sort_by(|a,b|b.root.cmp(&a.root).then(a.mount_point.cmp(&b.mount_point)));values.dedup_by(|a,b|a.mount_point==b.mount_point);
                let mut s=snapshot.lock().unwrap();s.disks=values;s.disks_at=Some(now());s.disk_error=None;s.os=os;
            } else {
                let (cpu,memory)=if matches!(runner.target,Target::Local){
                    system.refresh_cpu_usage();system.refresh_memory();let cpu=if native_primed{Some(system.global_cpu_usage() as f64)}else{None};native_primed=true;
                    (cpu,Memory{total:system.total_memory(),available:system.available_memory(),used:system.total_memory().saturating_sub(system.available_memory())})
                }else {let command=match os.as_str(){"linux"=>LINUX_FAST,"macos"=>MAC_FAST,_=>WINDOWS_FAST};let text=runner.command(command,os=="windows").await?;parse_fast(&text,&os,&mut previous)?};
                let mut s=snapshot.lock().unwrap();s.cpu_percent=cpu;s.memory=Some(memory);s.sampled_at=Some(now());s.error=None;s.os=os;
            }
            Ok::<(),anyhow::Error>(())
        }=>result};
        if !*enabled.borrow() {
            continue;
        }
        if let Ok(mut s) = snapshot.lock() {
            if let Err(e) = result {
                let message = Some(format!("{e:#}"));
                if disks {
                    s.disk_error = message;
                } else {
                    s.error = message;
                }
            }
            s.timestamp = now();
            let _ = app.emit("session-metrics", s.clone());
        }
    }
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
pub fn parse_fast(
    text: &str,
    os: &str,
    previous: &mut Option<(u64, u64)>,
) -> Result<(Option<f64>, Memory)> {
    if os == "windows" {
        let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
        let total = v["total"]
            .as_u64()
            .ok_or_else(|| anyhow!("Missing memory counter"))?;
        let available = v["available"]
            .as_u64()
            .ok_or_else(|| anyhow!("Missing available memory"))?;
        return Ok((
            v["cpu"].as_f64().map(|v| v.clamp(0., 100.)),
            Memory {
                total,
                available,
                used: total.saturating_sub(available),
            },
        ));
    }
    if os == "linux" {
        let counters: Vec<u64> = text
            .lines()
            .find(|l| l.starts_with("cpu "))
            .ok_or_else(|| anyhow!("CPU counters unavailable"))?
            .split_whitespace()
            .skip(1)
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        ensure!(counters.len() >= 4, "Truncated CPU counters");
        let total = counters.iter().take(8).sum::<u64>();
        let idle = counters[3] + counters.get(4).copied().unwrap_or(0);
        let cpu = previous.and_then(|(old_total, old_idle)| {
            let dt = total.checked_sub(old_total)?;
            let di = idle.checked_sub(old_idle)?;
            (dt > 0).then(|| 100. * (dt.saturating_sub(di)) as f64 / dt as f64)
        });
        *previous = Some((total, idle));
        let value = |key: &str| -> Option<u64> {
            text.lines()
                .find_map(|l| l.strip_prefix(key))
                .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
                .map(|v| v * 1024)
        };
        let total = value("MemTotal:").ok_or_else(|| anyhow!("Memory total unavailable"))?;
        let available = value("MemAvailable:")
            .or_else(|| {
                Some(
                    value("MemFree:")?
                        + value("Buffers:").unwrap_or(0)
                        + value("Cached:").unwrap_or(0),
                )
            })
            .ok_or_else(|| anyhow!("Available memory unavailable"))?
            .min(total);
        return Ok((
            cpu,
            Memory {
                total,
                available,
                used: total - available,
            },
        ));
    }
    let cpu = text
        .lines()
        .find(|l| l.contains("CPU usage:"))
        .and_then(|l| l.split(',').find(|p| p.contains("idle")))
        .and_then(|p| p.trim().split('%').next()?.parse::<f64>().ok())
        .map(|idle| (100. - idle).clamp(0., 100.));
    let total = text
        .split("TTMEM")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("macOS memory total unavailable"))?;
    let page_size = text
        .lines()
        .find(|l| l.contains("page size of"))
        .and_then(|l| l.split("page size of ").nth(1))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("macOS page size unavailable"))?;
    let pages = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.trim().trim_end_matches('.').parse::<u64>().ok())
            .unwrap_or(0)
    };
    let available =
        ((pages("Pages free:") + pages("Pages inactive:") + pages("Pages speculative:"))
            * page_size)
            .min(total);
    Ok((
        cpu,
        Memory {
            total,
            available,
            used: total - available,
        },
    ))
}
pub fn parse_disks(text: &str, os: &str) -> Result<Vec<Disk>> {
    let (df, mounts) = text.split_once("TTMOUNTS").unwrap_or((text, ""));
    let mut disks = vec![];
    for line in df.lines().skip(1) {
        let mut remainder = line;
        let mut parts = std::iter::from_fn(|| {
            remainder = remainder.trim_start();
            if remainder.is_empty() {
                return None;
            }
            let end = remainder
                .find(char::is_whitespace)
                .unwrap_or(remainder.len());
            let field = &remainder[..end];
            remainder = &remainder[end..];
            Some(field)
        });
        let Some(device) = parts.next() else { continue };
        let filesystem = if os == "linux" {
            parts.next().unwrap_or("").to_string()
        } else {
            String::new()
        };
        let total = parts.next().and_then(|v| v.parse::<u64>().ok());
        let used = parts.next().and_then(|v| v.parse::<u64>().ok());
        let available = parts.next().and_then(|v| v.parse::<u64>().ok());
        let _percentage = parts.next();
        drop(parts);
        let mount_point = remainder
            .trim_start()
            .replace("\\040", " ")
            .replace("\\011", "\t")
            .replace("\\134", "\\");
        let (Some(total), Some(used), Some(available)) = (total, used, available) else {
            continue;
        };
        if total == 0 || mount_point.is_empty() {
            continue;
        }
        let filesystem = if os == "macos" {
            mounts
                .lines()
                .find_map(|l| {
                    l.split_once(" on ")
                        .filter(|(d, _)| *d == device)
                        .and_then(|(_, v)| v.rsplit_once(" ("))
                        .filter(|(p, _)| *p == mount_point)
                        .map(|(_, v)| v.split([',', ')']).next().unwrap_or("").to_string())
                })
                .unwrap_or_default()
        } else {
            filesystem
        };
        let virtual_ = [
            "tmpfs", "devtmpfs", "proc", "sysfs", "devfs", "cgroup", "cgroup2", "overlay",
            "squashfs", "autofs", "rootfs",
        ]
        .contains(&filesystem.as_str());
        disks.push(Disk {
            root: mount_point == "/",
            mount_point,
            device: device.into(),
            filesystem,
            total: total.saturating_mul(1024),
            used: used.saturating_mul(1024),
            available: available.saturating_mul(1024),
            virtual_,
        });
    }
    ensure!(
        !disks.is_empty(),
        "No mounted filesystem counters available"
    );
    Ok(disks)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn native_probe_commands_return_real_counters() {
        let runner = Runner {
            target: Target::Local,
            os: OnceCell::new(),
            windows_ready: std::sync::atomic::AtomicBool::new(false),
        };
        let os = std::env::consts::OS;
        let cmd = match os {
            "windows" => WINDOWS_FAST,
            "macos" => MAC_FAST,
            _ => LINUX_FAST,
        };
        let text = runner.command(cmd, os == "windows").await.unwrap();
        let (_, memory) = parse_fast(&text, os, &mut None).unwrap();
        assert!(memory.total > 0);
        assert!(memory.used <= memory.total);
    }
    #[test]
    fn linux_deltas_and_reclaimable_memory() {
        let mut prev = None;
        let a = "cpu 10 0 10 80 0 0 0 0\nMemTotal: 1000 kB\nMemAvailable: 400 kB\n";
        let (c, m) = parse_fast(a, "linux", &mut prev).unwrap();
        assert!(c.is_none());
        assert_eq!(m.used, 600 * 1024);
        let b = a.replace("10 0 10 80", "20 0 20 160");
        assert_eq!(parse_fast(&b, "linux", &mut prev).unwrap().0, Some(20.));
        assert!(parse_fast("broken", "linux", &mut prev).is_err());
    }
    #[test]
    fn mount_paths_and_virtual_disks() {
        let d=parse_disks("Filesystem Type 1024-blocks Used Available Capacity Mounted on\n/dev/sda ext4 100 40 60 40% /\nserver:/data nfs4 200 50 150 25% /mnt/Türkçe Alan\ntmpfs tmpfs 20 0 20 0% /run\n","linux").unwrap();
        assert!(d[0].root);
        assert_eq!(d[1].mount_point, "/mnt/Türkçe Alan");
        assert!(d[2].virtual_);
    }
    #[test]
    fn mac_and_windows_samples() {
        let mut p = None;
        let (c,m)=parse_fast("CPU usage: 10.0% user, 5.0% sys, 85.0% idle\nTTMEM\n163840\nMach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 2.\nPages inactive: 1.\n","macos",&mut p).unwrap();
        assert_eq!(c, Some(15.));
        assert_eq!(m.available, 49152);
        let (c, m) = parse_fast(
            r#"{"cpu":25,"total":1000,"available":400}"#,
            "windows",
            &mut p,
        )
        .unwrap();
        assert_eq!(c, Some(25.));
        assert_eq!(m.used, 600);
    }
}
