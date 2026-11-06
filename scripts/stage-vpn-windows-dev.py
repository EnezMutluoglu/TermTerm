#!/usr/bin/env python3
"""Stage a local VPN development/candidate desktop; never elevate, load DLLs or install."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / ".tools/vpn-windows"
PIN = "a728bd5a1efee8a3a06e5efec07dd13126ddbeb43964cbb4d8c34ec6ba14665c"
DESKTOP = "termterm-vpn-dev.exe"


def nsis_bytes(data):
    unknown = b"__TAURI_BUNDLE_TYPE_VAR_UNK"
    nsis = b"__TAURI_BUNDLE_TYPE_VAR_NSS"
    if data.count(unknown) != 1 or data.count(nsis):
        raise ValueError("Expected the single unbundled Tauri marker before NSIS packaging")
    return data.replace(unknown, nsis, 1)


def contract(candidate=None):
    if candidate not in (None, "0.4.3"):
        raise ValueError("Unsupported production candidate version")
    return ("termterm.exe", "vpn-bundle.json", "local.termterm.desktop") if candidate else (DESKTOP, "vpn-dev-bundle.json", "local.termterm.desktop.vpndev")


def verify_installer(installer, stage, candidate=None):
    desktop, manifest_name, _ = contract(candidate)
    manifest = (stage / manifest_name).read_bytes()
    files = json.loads(manifest)["files"]
    sevenzip = TOOLS / "downloads/tools/7zip-26.03-windows/7z.exe"
    for name, expected in [(desktop, (stage / desktop).read_bytes()), (manifest_name, manifest)]:
        actual = subprocess.run([str(sevenzip), "x", "-so", str(installer), name], check=True, capture_output=True).stdout
        if actual != expected:
            raise ValueError("NSIS payload differs from its exact staged inventory: " + name)
    if digest(stage / desktop) != files[desktop]:
        raise ValueError("NSIS desktop inventory hash mismatch")
    print("Verified exact NSIS desktop payload and inventory; installer was not executed")


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(65536):
            value.update(block)
    return value.hexdigest()


def inside(path, parent):
    path = path.resolve()
    if not path.is_relative_to(parent.resolve()):
        raise ValueError("Path must remain inside the VPN worktree tooling directory")
    return path


def stage(application, broker, destination, bundle_type="portable", candidate=None):
    desktop, manifest_name, identity = contract(candidate)
    application = inside(application, ROOT)
    broker = inside(broker, ROOT)
    destination = inside(destination, TOOLS)
    if application.name != desktop or broker.name != "termterm-vpn-broker.exe":
        raise ValueError("Use the fixed desktop and default broker executable names")
    if destination.exists():
        raise ValueError("Use a new destination; previous test bundles are preserved")
    lock = json.loads((ROOT / "scripts/vpn-windows-dependencies.json").read_bytes())
    sources = {
        desktop: application,
        "termterm-vpn-broker.exe": broker,
        "termterm-openvpn.exe": TOOLS / "engine/termterm-openvpn.exe",
        "termterm-openconnect.exe": TOOLS / "engine/termterm-openconnect.exe",
        "termterm-wireguard.exe": TOOLS / "engine/termterm-wireguard.exe",
        "wintun.dll": TOOLS / "wintun/wintun/bin/amd64/wintun.dll",
        "WINTUN-LICENSE.txt": TOOLS / "wintun/wintun/LICENSE.txt",
        "COPYING.OpenConnect.LGPL.txt": TOOLS / "COPYING.LGPL",
        "OpenVPN-LICENSE.md": TOOLS / "openvpn3/LICENSE.md",
    }
    wireguard = json.loads((ROOT / "vpn-wireguard/dependencies.json").read_bytes())
    go_notice = ROOT / "docs/licenses/vpn/WireGuard-Go-runtime-LICENSE.txt"
    if digest(go_notice) != wireguard["goLicenseSha256"]:
        raise ValueError("Go runtime license SHA256 mismatch")
    sources[go_notice.name] = go_notice
    for module in wireguard["modules"]:
        source = ROOT / "docs/licenses/vpn" / module["notice"]
        if digest(source) != module["licenseSha256"]:
            raise ValueError("WireGuard license SHA256 mismatch")
        sources[module["notice"]] = source
    for source in sorted((TOOLS / "engine").glob("*.dll")):
        if source.name.casefold() == "wintun.dll":
            raise ValueError("Wintun must come from the separately pinned official archive")
        sources[source.name] = source
    if len(sources) > 64:
        raise ValueError("Unexpected dependency inventory")
    for source in sources.values():
        if source.is_symlink() or not source.is_file() or not source.stat().st_size:
            raise ValueError(f"Missing regular source file: {source.name}")
    if digest(sources["wintun.dll"]) != lock["wintun"]["dllSha256"]:
        raise ValueError("Wintun SHA256 mismatch")
    if digest(sources["libopenconnect-5.dll"]) != PIN:
        raise ValueError("Audited packet-handle OpenConnect DLL SHA256 mismatch")
    if digest(sources["WINTUN-LICENSE.txt"]) != lock["wintun"]["licenseSha256"]:
        raise ValueError("Wintun license SHA256 mismatch")
    if digest(sources["COPYING.OpenConnect.LGPL.txt"]) != lock["openconnect"]["licenseSha256"]:
        raise ValueError("OpenConnect license SHA256 mismatch")
    destination.mkdir(parents=True, exist_ok=False)
    for name, source in sources.items():
        if name == desktop and bundle_type == "nsis":
            # Tauri patches only the installer copy; its exact hash differs from the portable executable.
            (destination / name).write_bytes(nsis_bytes(source.read_bytes()))
        else:
            shutil.copyfile(source, destination / name)
    manifest = {"version": 1, "applicationIdentifier": identity,
                "developmentOnly": not bool(candidate),
                "files": {name: digest(destination / name) for name in sorted(sources)}}
    if candidate:
        manifest["candidateVersion"] = candidate
    path = destination / manifest_name
    path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"directory": str(destination), "files": len(sources),
                      "manifestSha256": digest(path), "privilegedReadinessEnabled": False,
                      "distributionReady": False}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--application", type=Path)
    parser.add_argument("--broker", type=Path)
    parser.add_argument("--destination", type=Path)
    parser.add_argument("--bundle-type", choices=["portable", "nsis"], default="portable")
    parser.add_argument("--candidate", choices=["0.4.3"])
    parser.add_argument("--verify-installer", type=Path)
    args = parser.parse_args()
    if args.verify_installer and args.destination:
        verify_installer(args.verify_installer, args.destination, args.candidate)
    elif args.application and args.broker and args.destination:
        stage(args.application, args.broker, args.destination, args.bundle_type, args.candidate)
    else:
        parser.error("Provide application/broker/destination or verify-installer/destination")
