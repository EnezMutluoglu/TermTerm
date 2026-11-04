#!/usr/bin/env python3
"""Build the packet-only official WireGuard engine; never installs or changes network state."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess as sp
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "vpn-wireguard"
LOCK = json.loads((SOURCE / "dependencies.json").read_bytes())


def run(*args, **kwargs):
    return sp.run(list(map(str, args)), cwd=SOURCE, check=True, **kwargs)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(path, expected):
    if path.is_symlink() or not path.is_file() or digest(path) != expected:
        raise ValueError("WireGuard dependency integrity mismatch")


def validate_lock():
    assert LOCK["version"] == 1 and LOCK["distributionReady"] is False
    assert LOCK["goVersion"] == "go1.26.4"
    verify(ROOT / "docs/licenses/vpn/WireGuard-Go-runtime-LICENSE.txt", LOCK["goLicenseSha256"])
    assert len(LOCK["modules"]) == 5
    assert LOCK["modules"][0]["module"] == "golang.zx2c4.com/wireguard"
    for module in LOCK["modules"]:
        for field in ["archiveSha256", "licenseSha256"]:
            assert len(module[field]) == 64 and all(c in "0123456789abcdef" for c in module[field])
        assert module["sum"].startswith("h1:")
        verify(ROOT / "docs/licenses/vpn" / module["notice"], module["licenseSha256"])


def build(target, arch, destination):
    validate_lock()
    destination = destination.resolve()
    if not destination.is_relative_to((ROOT / ".tools").resolve()):
        raise ValueError("Output must remain in this worktree's disposable .tools directory")
    if destination.name != ("termterm-wireguard.exe" if target == "windows" else "termterm-wireguard"):
        raise ValueError("Use the fixed engine filename")
    version = run("go", "env", "GOVERSION", capture_output=True, text=True).stdout.strip()
    if version != LOCK["goVersion"]:
        raise ValueError("Use the pinned existing Go toolchain; this script never installs a toolchain")
    for module in LOCK["modules"]:
        downloaded = json.loads(run("go", "mod", "download", "-json", module["module"] + "@" + module["version"], capture_output=True).stdout)
        if downloaded["Sum"] != module["sum"]:
            raise ValueError("WireGuard module checksum mismatch")
        verify(Path(downloaded["Zip"]), module["archiveSha256"])
        verify(Path(downloaded["Dir"]) / module["licenseFile"], module["licenseSha256"])
    run("go", "mod", "verify")
    destination.parent.mkdir(parents=True, exist_ok=True)
    environment = {**os.environ, "GOOS": target, "GOARCH": arch, "CGO_ENABLED": "0", "GOTOOLCHAIN": "local"}
    run("go", "build", "-mod=readonly", "-trimpath", "-buildvcs=false", "-ldflags=-s -w", "-o", destination, ".", env=environment)
    print(json.dumps({"engine": destination.name, "target": target, "arch": arch, "sha256": digest(destination),
                      "wireguardVersion": LOCK["modules"][0]["version"], "distributionReady": False}))


def self_test():
    validate_lock()
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "bad-module.zip"
        path.write_bytes(b"not the approved dependency")
        try:
            verify(path, "0" * 64)
        except ValueError:
            pass
        else:
            raise AssertionError("Wrong dependency hash accepted")
    print("WireGuard dependency/notice integrity checks passed; no engine or network action.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--target", choices=["windows", "linux", "darwin"], default="windows" if os.name == "nt" else "linux")
    parser.add_argument("--arch", choices=["amd64", "arm64"], default="amd64")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        default = ROOT / (".tools/vpn-windows/engine/termterm-wireguard.exe" if args.target == "windows" else f".tools/vpn-wireguard/{args.target}-{args.arch}/termterm-wireguard")
        build(args.target, args.arch, args.output or default)
