#!/usr/bin/env python3
"""Freeze WireGuard acceptance bundles for review; never elevates or opens adapters."""
import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess as sp
from vpn_lab_json import read_json

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / ".tools/vpn-windows"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(*args, **kwargs):
    return sp.run(list(map(str, args)), cwd=ROOT, check=True, **kwargs)


def source_paths():
    paths = []
    for crate in ["vpn-core", "vpn-windows-broker"]:
        paths += [ROOT / crate / name for name in ["Cargo.toml", "Cargo.lock"]]
        paths += sorted((ROOT / crate / "src").rglob("*.rs"))
        if (ROOT / crate / "tests").exists():
            paths += sorted((ROOT / crate / "tests").glob("*.rs"))
    paths += sorted((ROOT / "vpn-wireguard").glob("*.go"))
    paths += [ROOT / "vpn-wireguard" / name for name in ["go.mod", "go.sum", "dependencies.json"]]
    paths += [ROOT / "scripts" / name for name in ["build-wireguard.py", "prepare-wireguard-windows.py", "test-wireguard-lab.py", "wireguard-windows-relay.py", "vpn-dns-fixture.py", "vpn_lab_json.py", "test-vpn-lab-json.py", "vpn-windows-dependencies.json"]]
    paths += [ROOT / "vpn-windows-broker/scripts" / name for name in ["validate-wireguard-v1.ps1", "validate-full-broker-v5.ps1", "validate-dns-v1.ps1", "validate-dns-journal-audit-v1.ps1"]]
    return paths


def prepare(fixture, prefix, liveness=False, modes=None):
    if os.name != "nt":
        raise ValueError("Run this inert packaging step on Windows")
    fixture = fixture.resolve(strict=True)
    prefix = prefix.resolve()
    if not fixture.is_relative_to((ROOT / ".lab").resolve()) or fixture.name != "profiles.json" or prefix.parent != TOOLS.resolve():
        raise ValueError("Use this worktree's private .lab fixture and a new .tools/vpn-windows bundle prefix")
    destinations = {mode: prefix.with_name(prefix.name + "-" + mode) for mode in (modes or (["silent-loss"] if liveness else ["normal", "client-death", "wrong-key"]))}
    if any(path.exists() for path in destinations.values()):
        raise ValueError("Preserve previous bundles; choose a new prefix")
    data = read_json(fixture)
    ready = read_json(fixture.parent / "ready.json")
    relay = read_json(fixture.parent / "relay-receipt.json")
    if ready["fixtureId"] != data["fixtureId"] or relay["fixtureId"] != data["fixtureId"] or relay["stopped"] or (fixture.parent / "stopped.json").exists():
        raise ValueError("Generated kernel peer and loopback relay must be live")
    baseline_path = fixture.parent / "linux-baseline-before.json"
    if digest(baseline_path) != ready["baselineSha256"]:
        raise ValueError("Original Linux baseline digest changed")
    baseline = read_json(baseline_path)
    if baseline["fixtureId"] != data["fixtureId"] or baseline["incarnation"] != ready["linuxIncarnation"]:
        raise ValueError("Original Linux baseline identity changed")
    expires = datetime.fromtimestamp((fixture.parent / "ready.json").stat().st_mtime, timezone.utc) + timedelta(seconds=ready["durationSeconds"])
    if expires < datetime.now(timezone.utc) + timedelta(minutes=30) or ready["hostRoutesDnsChanged"]:
        raise ValueError("Prepare a fresh unchanged-network fixture with at least 30 minutes left")
    if any(digest(ROOT / name) != expected for name, expected in ready["sourceSha256"].items()):
        raise ValueError("Live fixture sources changed; prepare a fresh kernel fixture")
    relay_created = run("powershell.exe", "-NoProfile", "-NonInteractive", "-Command", f"(Get-Process -Id {int(relay['pid'])}).StartTime.ToUniversalTime().ToString('o')", capture_output=True, text=True).stdout.strip()
    linux_identity = json.loads(run("wsl.exe", "-d", "Ubuntu-24.04", "-u", "root", "--", "python3", "-c",
                                   f"import pathlib,hashlib,json;p=pathlib.Path('/proc/{int(ready['pid'])}');print(json.dumps({{'startTicks':p.joinpath('stat').read_text().split()[21],'commandLineSha256':hashlib.sha256(p.joinpath('cmdline').read_bytes()).hexdigest(),'incarnation':{{'bootId':pathlib.Path('/proc/sys/kernel/random/boot_id').read_text().strip(),'pid1StartTicks':pathlib.Path('/proc/1/stat').read_text().split()[21]}}}}))", capture_output=True).stdout)
    if linux_identity["incarnation"] != baseline["incarnation"]:
        raise ValueError("Linux process namespace changed since the original baseline")
    before = {str(path.relative_to(ROOT)).replace("\\", "/"): digest(path) for path in source_paths()}
    cargo = Path.home() / ".cargo/bin/cargo.exe"
    run(cargo, "build", "--locked", "--manifest-path", "vpn-windows-broker/Cargo.toml", "--features", "privileged-validation")
    run(shutil.which("python") or "python", "scripts/build-wireguard.py")
    after = {str(path.relative_to(ROOT)).replace("\\", "/"): digest(path) for path in source_paths()}
    if before != after:
        raise ValueError("VPN source changed during build; no reviewed bundle was frozen")
    broker = ROOT / "vpn-windows-broker/target/debug/termterm-vpn-broker.exe"
    capabilities = json.loads(run(broker, "--capabilities", capture_output=True).stdout)
    if capabilities.get("version") != 2 or capabilities.get("networkLease") is not True or capabilities.get("quiesce") is not True:
        raise ValueError("Protocol 2 broker is required; no legacy fallback")
    sources = {"termterm-vpn-broker.exe": broker, "termterm-wireguard.exe": TOOLS / "engine/termterm-wireguard.exe",
               "wintun.dll": TOOLS / "wintun/wintun/bin/amd64/wintun.dll", "WINTUN-LICENSE.txt": TOOLS / "wintun/wintun/LICENSE.txt"}
    for name in ["validate-wireguard-v1.ps1", "validate-full-broker-v5.ps1", "validate-dns-v1.ps1", "validate-dns-journal-audit-v1.ps1"]:
        sources[name] = ROOT / "vpn-windows-broker/scripts" / name
    wireguard = json.loads((ROOT / "vpn-wireguard/dependencies.json").read_bytes())
    for module in wireguard["modules"]:
        notice = ROOT / "docs/licenses/vpn" / module["notice"]
        if digest(notice) != module["licenseSha256"]:
            raise ValueError("WireGuard notice differs from the locked source")
        sources[notice.name] = notice
    sources["WireGuard-Go-runtime-LICENSE.txt"] = ROOT / "docs/licenses/vpn/WireGuard-Go-runtime-LICENSE.txt"
    lock = json.loads((ROOT / "scripts/vpn-windows-dependencies.json").read_bytes())
    for name, expected in [("wintun.dll", lock["wintun"]["dllSha256"]), ("WINTUN-LICENSE.txt", lock["wintun"]["licenseSha256"]), ("WireGuard-Go-runtime-LICENSE.txt", wireguard["goLicenseSha256"])]:
        if digest(sources[name]) != expected:
            raise ValueError("Pinned driver/runtime license integrity mismatch")
    if any(path.is_symlink() or not path.is_file() for path in sources.values()):
        raise ValueError("Only regular reviewed bundle members are allowed")
    result = {"version": 1, "utc": datetime.now(timezone.utc).isoformat(), "fixture": str(fixture), "fixtureSha256": digest(fixture),
              "fixtureId": data["fixtureId"], "requiresIndependentReview": True, "elevationPerformed": False,
              "fixtureLinuxPid": ready["pid"], "relayWindowsPid": relay["pid"], "fixtureExpiresUtc": expires.isoformat(),
              "fixtureLinuxIdentity": linux_identity, "relayCreatedUtc": relay_created,
              "linuxBaselineSha256": ready["baselineSha256"], "linuxBaselineIncarnation": baseline["incarnation"],
              "fixtureStopMarker": str(fixture.parent / "stop"), "relayStopMarker": str(fixture.parent / "relay-stop"),
              "windowsAdapterAcceptanceRun": False, "existing505RouteAcceptanceCompleted": False, "bundles": {}}
    for mode, destination in destinations.items():
        destination.mkdir(exist_ok=False)
        for name, source in sources.items():
            shutil.copyfile(source, destination / name)
        receipt = {"version": 1, "sourceSha256": after, "sourceUnchangedDuringBuild": True, "developmentOnly": True,
                   "brokerFeatures": ["privileged-validation"], "goVersion": wireguard["goVersion"], "capabilities": capabilities}
        (destination / "source-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        manifest = {"version": 1, "protocolVersion": 2, "developmentOnly": True, "mode": mode, "fixtureId": data["fixtureId"],
                    "files": [{"name": path.name, "sha256": digest(path)} for path in sorted(destination.iterdir())]}
        manifest_path = destination / "manifest.json"
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        result["bundles"][mode] = {"directory": str(destination), "manifestSha256": digest(manifest_path),
                                  "helperSha256": digest(destination / "termterm-vpn-broker.exe"), "runnerSha256": digest(destination / "validate-wireguard-v1.ps1")}
    preparation = prefix.with_name(prefix.name + "-preparation.json")
    with preparation.open("x", encoding="utf-8") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    print(json.dumps({"preparation": str(preparation), "fixtureId": data["fixtureId"], "bundleCount": len(destinations), "elevationPerformed": False}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--destination-prefix", type=Path, required=True)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--liveness", action="store_true")
    modes.add_argument("--mode", action="append", choices=["normal", "client-death", "wrong-key", "silent-loss"])
    args = parser.parse_args()
    prepare(args.fixture, args.destination_prefix, args.liveness, args.mode)
