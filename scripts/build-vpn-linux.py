#!/usr/bin/env python3
"""Build packet engines only in the separately prepared Jammy guest; never install a broker or connect."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess as sp
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("vpn_inventory", ROOT / "scripts/collect-vpn-runtime.py")
inventory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inventory)
GUEST = Path("/root/termterm-vpn-desktop/jammy")


def build(inputs, run):
    if os.name == "nt" or os.geteuid() != 0:
        raise ValueError("Run under the isolated WSL build root owner, not on Windows or macOS")
    if not re.fullmatch(r"[a-z0-9-]{1,40}", run):
        raise ValueError("Use a simple new build receipt name")
    inventory.no_links(GUEST)
    if (GUEST / ".termterm-vpn-build-root").read_text() != "isolated-vpndev-jammy\n":
        raise ValueError("The separate VPN Jammy build marker is required")
    if (GUEST / "usr/sbin/policy-rc.d").read_text() != "#!/bin/sh\nexit 101\n":
        raise ValueError("Guest service autostart must remain blocked")
    inputs = inputs.absolute()
    manifest = json.loads((inputs / "INPUTS.json").read_bytes())
    if manifest.get("version") != 1 or manifest.get("guest") != str(GUEST):
        raise ValueError("Unsupported vendor build input receipt")
    inventory.verified_files(inputs, manifest.get("files"), ("INPUTS.json",))
    installed = json.loads((GUEST.parent / "TOOLS.json").read_bytes())
    if installed["inputManifestSha256"] != inventory.digest(inputs / "INPUTS.json"):
        raise ValueError("Installed tools do not match these verified build inputs")
    work = GUEST / "opt/termterm-vpn-build" / run
    work.mkdir()
    source = work / "source"
    for relative in manifest["files"]:
        if relative.startswith("source/"):
            target = work / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(inputs / relative, target)
    vendor = work / "vendor"
    vendor.mkdir()
    core = vendor / "openvpn3"
    core.mkdir()
    for name, target in [("openvpn3-source.tar", core), ("openconnect-9.21.tar.gz", vendor), ("asio-1.38.2.tar.gz", vendor), ("fmt-12.2.0.tar.gz", vendor), ("lz4-1.10.0.tar.gz", vendor)]:
        with tarfile.open(inputs / name) as archive:
            archive.extractall(target, filter="data")
    prefix = GUEST / "opt/termterm-vpn-dev"
    prefix.mkdir(exist_ok=True)
    log_path = GUEST.parent / ("engine-" + run + ".log")
    environment = {"PATH": "/usr/sbin:/usr/bin:/bin", "HOME": "/root", "LC_ALL": "C"}
    def virtual(path):
        return "/" + path.relative_to(GUEST).as_posix()
    with log_path.open("x") as log:
        def command(arguments, cwd=work):
            sp.run(["/usr/sbin/chroot", str(GUEST), "/usr/bin/env", "-C", virtual(cwd),
                    "PATH=/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin", "HOME=/root", "LC_ALL=C",
                    "PKG_CONFIG_PATH=/opt/termterm-vpn-dev/lib/pkgconfig", "CMAKE_PREFIX_PATH=/opt/termterm-vpn-dev",
                    "LD_LIBRARY_PATH=/opt/termterm-vpn-dev/lib", "GOTOOLCHAIN=local",
                    "GOROOT=/opt/go", "GOTELEMETRY=off",
                    "GOPATH=/opt/termterm-vpn-build/go-home", *map(str, arguments)],
                   env=environment, check=True, stdout=log, stderr=sp.STDOUT)
        def patch(path, target):
            copy = work / path.name
            shutil.copyfile(path, copy)
            command(["patch", "--fuzz=0", "-p1", "-i", virtual(copy)], target)
        def cmake(name, path, extra=()):
            print("Building " + name + " in Jammy", flush=True)
            directory = work / ("build-" + name)
            command(["cmake", "-S", virtual(path), "-B", virtual(directory), "-G", "Ninja",
                     "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_INSTALL_PREFIX=/opt/termterm-vpn-dev",
                     "-DCMAKE_INSTALL_LIBDIR=lib", "-DCMAKE_BUILD_RPATH=/opt/termterm-vpn-dev/lib",
                     "-DCMAKE_INSTALL_RPATH=$ORIGIN/../lib", *extra])
            command(["cmake", "--build", virtual(directory), "--parallel", "2"])
            if name != "engines":
                command(["cmake", "--install", virtual(directory)])
            return directory
        asio = vendor / "asio-asio-1-38-2"
        overlay = core / "deps/vcpkg-ports/asio"
        for path in sorted(overlay.glob("*.patch")):
            patch(path, asio)
        shutil.copyfile(overlay / "CMakeLists.txt", asio / "CMakeLists.txt")
        cmake("asio", asio, ["-DPACKAGE_VERSION=1.38.2"])
        shutil.copyfile(overlay / "asio-config.cmake", prefix / "share/asio/asio-config.cmake")
        patch(inputs / "target-lz4-lz4.diff", vendor / "lz4-1.10.0")
        cmake("lz4", vendor / "lz4-1.10.0/build/cmake", ["-DLZ4_BUILD_CLI=OFF", "-DBUILD_SHARED_LIBS=ON"])
        patch(inputs / "fmt-backport-4813.patch", vendor / "fmt-12.2.0")
        cmake("fmt", vendor / "fmt-12.2.0", ["-DFMT_TEST=OFF", "-DFMT_DOC=OFF", "-DBUILD_SHARED_LIBS=ON"])
        print("Building patched OpenConnect9.21 in Jammy", flush=True)
        oc = vendor / "openconnect-9.21"
        patch(source / "vpn-engine/patches/openconnect-9.21-gp-sso.patch", oc)
        command(["./configure", "--prefix=/opt/termterm-vpn-dev", "--disable-nls", "--without-java", "--without-libproxy",
                 "--without-stoken", "--without-lz4", "--without-pskc", "--without-pcsclite", "--disable-static", "--enable-shared",
                 "--with-vpnc-script=/termterm/no-script"], oc)
        command(["make", "-j2", "libopenconnect.la"], oc)
        command(["make", "install-libLTLIBRARIES", "install-pkgconfigDATA", "install-includeHEADERS"], oc)
        directory = cmake("engines", source / "vpn-engine", ["-DOPENVPN_CORE=" + virtual(core)])
        (prefix / "bin").mkdir(exist_ok=True)
        for name in ["termterm-openconnect", "termterm-openvpn"]:
            shutil.copyfile(directory / name, prefix / "bin" / name)
            (prefix / "bin" / name).chmod(0o755)
        print("Building and unit testing WireGuard in Jammy", flush=True)
        command(["go", "test", "-count=1", "./..."], source / "vpn-wireguard")
        command(["go", "vet", "./..."], source / "vpn-wireguard")
        command(["python3", "scripts/build-wireguard.py", "--target", "linux", "--arch", "amd64", "--output", virtual(source / ".tools/termterm-wireguard")], source)
        shutil.copyfile(source / ".tools/termterm-wireguard", prefix / "bin/termterm-wireguard")
        (prefix / "bin/termterm-wireguard").chmod(0o755)
    binaries = {}
    for name in ["termterm-openconnect", "termterm-openvpn", "termterm-wireguard"]:
        path = prefix / "bin" / name
        binaries[name] = {"sha256": inventory.digest(path), "bytes": path.stat().st_size,
                          "dynamic": sp.check_output(["/usr/sbin/chroot", str(GUEST), "ldd", virtual(path)], env=environment, text=True, stderr=sp.STDOUT).strip() if name != "termterm-wireguard" else "CGO_ENABLED=0"}
    receipt = {"guest": str(GUEST), "run": run, "inputManifestSha256": inventory.digest(inputs / "INPUTS.json"),
               "binaries": binaries, "nativeVpnTested": False, "brokerBuilt": False, "desktopBuilt": False}
    (work / "ENGINE-RECEIPT.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))


def self_test():
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "input"
        path.write_bytes(b"inert vendor input")
        inventory.verified_files(Path(directory), {"input": inventory.digest(path)})
        path.write_bytes(b"changed")
        try:
            inventory.verified_files(Path(directory), {"input": "0" * 64})
        except ValueError:
            pass
        else:
            raise AssertionError("Changed build input was accepted")
    print("Inert Linux vendor input checks passed; no guest or network action")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inputs", type=Path)
    parser.add_argument("--run", default="vendor-v1")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    elif args.inputs:
        build(args.inputs, args.run)
    else:
        parser.error("Provide verified --inputs or --self-test")
