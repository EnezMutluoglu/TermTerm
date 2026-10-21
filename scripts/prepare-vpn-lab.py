#!/usr/bin/env python3
"""Prepare isolated Linux lab helpers, never install or uninstall the desktop app."""
import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess as sp

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = json.loads((ROOT / "vpn-engine/dependencies.json").read_text())


def run(*args, **kwargs):
    return sp.run(list(map(str, args)), check=True, **kwargs)


def verified_source(path, spec):
    if not path.exists():
        run("git", "init", path)
        run("git", "-C", path, "remote", "add", "origin", spec["url"])
        run("git", "-C", path, "fetch", "--depth", "1", "origin", spec["commit"])
        run("git", "-C", path, "checkout", "--detach", "FETCH_HEAD")
    head = run("git", "-C", path, "rev-parse", "HEAD", capture_output=True, text=True).stdout.strip()
    dirty = run("git", "-C", path, "status", "--porcelain", capture_output=True, text=True).stdout
    if head != spec["commit"] or dirty:
        raise RuntimeError(f"Source does not match the locked clean commit: {path}")
    with sp.Popen(["git", "-C", str(path), "archive", "--format=tar", "HEAD"], stdout=sp.PIPE) as process:
        digest = hashlib.file_digest(process.stdout, "sha256").hexdigest()
        if process.wait() != 0 or digest != spec["gitArchiveSha256"]:
            raise RuntimeError("VPN source archive integrity check failed")
    if hashlib.sha256((path / spec["licenseFile"]).read_bytes()).hexdigest() != spec["licenseSha256"]:
        raise RuntimeError("VPN source license changed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-dir", type=Path, required=True)
    parser.add_argument("--install-dependencies", action="store_true")
    args = parser.parse_args()
    os_release = Path("/etc/os-release").read_text()
    if 'VERSION_ID="24.04"' not in os_release or 'ID=ubuntu' not in os_release:
        parser.error("This reproducible lab recipe requires Ubuntu 24.04; desktop support is separate")
    if args.install_dependencies:
        if os.geteuid() != 0:
            parser.error("Dependency installation requires root")
        run("apt-get", "update")
        run("apt-get", "install", "-y", "cmake", "git", "python3", "openssl", "iproute2", "iputils-ping",
            "libasio-dev", "libfmt-dev", "libjsoncpp-dev", "libcap-dev", "liblz4-dev", "libxxhash-dev",
            "libssl-dev", "pkg-config", "ninja-build", "g++", "libprotobuf-c1", "libtalloc2",
            "libradcli4", "liboath0", "libev4", "libhttp-parser2.9",
            "libopenconnect-dev=" + MANIFEST["ubuntu2404Lab"]["openconnectPackage"],
            "openvpn=" + MANIFEST["ubuntu2404Lab"]["openvpnServerPackage"])
    build = args.build_dir.resolve()
    if build in [Path("/"), Path.home(), ROOT] or build.is_relative_to(Path("/usr")):
        parser.error("Choose a dedicated disposable build directory")
    build.mkdir(parents=True, exist_ok=True)
    for package, version in [("libopenconnect-dev", "openconnectPackage"), ("openvpn", "openvpnServerPackage")]:
        actual = run("dpkg-query", "-W", "-f=${Version}", package, capture_output=True, text=True).stdout
        if actual != MANIFEST["ubuntu2404Lab"][version]:
            raise RuntimeError(f"Unexpected lab package version: {package} {actual}")
    verified_source(build / "openvpn3", MANIFEST["openvpnCore"])
    version = MANIFEST["ubuntu2404Lab"]["ocservPackage"]
    deb = build / f"ocserv_{version}_amd64.deb"
    if not deb.exists():
        run("apt-get", "download", f"ocserv={version}", cwd=build)
    if hashlib.sha256(deb.read_bytes()).hexdigest() != MANIFEST["ubuntu2404Lab"]["ocservDebSha256"]:
        raise RuntimeError("ocserv lab package integrity check failed")
    run("dpkg-deb", "-x", deb, build / "ocserv")
    run("cmake", "-S", ROOT / "vpn-engine", "-B", build / "engine", "-G", "Ninja",
        "-DOPENVPN_CORE=" + str(build / "openvpn3"), "-DCMAKE_BUILD_TYPE=Release")
    run("cmake", "--build", build / "engine", "-j2")
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
    if not Path(cargo).is_file():
        raise RuntimeError("Install the project Rust toolchain before building the VPN policy helper")
    run(cargo, "build", "--locked", "--manifest-path", ROOT / "vpn-core/Cargo.toml",
        "--target-dir", build / "policy-target")
    print("Lab helpers ready. No VPN connection, desktop installer or global route change was performed.")


if __name__ == "__main__":
    main()
