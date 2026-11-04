#!/usr/bin/env python3
"""Build isolated Linux development packages; never install, start services or publish."""
import argparse
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess as sp
import tarfile
import tempfile
import zipfile

GUEST = Path("/root/termterm-vpn-desktop/jammy")
PREFIX = GUEST / "opt/termterm-vpn-dev"
ENGINE_BUILD = GUEST / "opt/termterm-vpn-build/vendor-v1"
BASE_LIBS = {"libc.so.6", "libm.so.6", "libstdc++.so.6", "libgcc_s.so.1", "libpthread.so.0", "libdl.so.2", "librt.so.1"}
IDENTITY = "local.termterm.desktop.vpndev"
DESKTOP_PACKAGE = "term-term-vpn-dev"


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def safe_name(name):
    path = PurePosixPath(name)
    require(name and "\\" not in name and ":" not in name and not path.is_absolute()
            and ".." not in path.parts and path.as_posix() == name, "Unsafe source member")
    require(not any(part in {".git", ".lab", ".tools", "node_modules", "target", "artifacts"} for part in path.parts), "Private/build source member")
    require(path.suffix.lower() not in {".pem", ".key", ".pfx", ".p12", ".dump", ".exe", ".dll"}
            and not any(part.startswith(".env") and part != ".env.vpn-dev" for part in path.parts)
            and not any(part.startswith(".") and part != ".env.vpn-dev" for part in path.parts)
            and not any(".ttvault" in part or ".ttbackup" in part for part in path.parts), "Private/binary source member")
    return path


def source_archive(path, expected, destination=None):
    require(not path.is_symlink() and path.is_file() and digest(path) == expected, "Source ZIP SHA256 mismatch")
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        require(len(names) == len(set(names)) and len(names) <= 2000 and "SOURCE-RECEIPT.json" in names, "Invalid source inventory")
        receipt = json.loads(archive.read("SOURCE-RECEIPT.json"))
        require(isinstance(receipt, dict) and receipt and set(names) == set(receipt) | {"SOURCE-RECEIPT.json"}, "Source ZIP inventory mismatch")
        for member in archive.infolist():
            safe_name(member.filename)
            require(not member.is_dir() and not stat.S_ISLNK(member.external_attr >> 16) and member.file_size <= 32 * 1024 * 1024, "Invalid source ZIP member")
            data = archive.read(member)
            if member.filename != "SOURCE-RECEIPT.json":
                require(hashlib.sha256(data).hexdigest() == receipt[member.filename], "Source member SHA256 mismatch")
            if destination is not None:
                target = destination / member.filename
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
        require(archive.read(".env.vpn-dev").strip() == b"VITE_VPN_DEV=1", "Unexpected development environment")
        return receipt


def verify_source(source, receipt):
    for name, expected in receipt.items():
        path = source / name
        require(not path.is_symlink() and path.is_file() and digest(path) == expected, "Frozen source changed: " + name)


def delivery_path(path, root):
    path = path.absolute()
    require(path.resolve() == path and not any(parent.is_symlink() for parent in [path, *path.parents]), "Delivery paths cannot contain links")
    require(path.name == "linux" and re.fullmatch(r"vpn-delivery-[0-9]{8}", path.parent.name)
            and path.parent.parent == root / "artifacts" and not path.exists(), "Choose a new directory inside this worktree's artifact delivery root")
    return path


def bundle_formats(client, backup, arguments, command):
    expected = digest(client)
    require(client.read_bytes().count(b"__TAURI_BUNDLE_TYPE_VAR_UNK") == 1, "Unmodified Tauri executable required before bundling")
    if backup.exists():
        require(not backup.is_symlink() and digest(backup) == expected, "Original executable backup changed")
    else:
        shutil.copyfile(client, backup)
    try:
        for kind in ["deb", "appimage"]:
            # Tauri mutates this marker in place; each format needs the original executable.
            shutil.copyfile(backup, client)
            require(digest(client) == expected, "Executable changed before bundling")
            command([*arguments, "--bundles", kind])
    finally:
        shutil.copyfile(backup, client)


def build(args):
    require(os.name != "nt" and os.geteuid() == 0, "Run as the isolated WSL build root owner")
    require(re.fullmatch(r"[a-z0-9-]{1,40}", args.run) is not None and args.uid > 0, "A simple build name and nonroot UID are required")
    require(GUEST.resolve() == GUEST and (GUEST / ".termterm-vpn-build-root").read_text() == "isolated-vpndev-jammy\n", "Separate marked Jammy build guest required")
    require((GUEST / "usr/sbin/policy-rc.d").read_text() == "#!/bin/sh\nexit 101\n", "Guest service autostart must remain blocked")
    output = delivery_path(args.output, Path(__file__).resolve().parents[1])
    receipt = source_archive(args.source_zip, args.source_sha256)
    require(receipt.get("scripts/package-vpn-linux.py") == digest(Path(__file__)), "Packaging script differs from the frozen public source")
    work = GUEST / "work" / args.run
    target = GUEST / "opt/termterm-vpn-build" / ("desktop-target-" + args.run.removeprefix("vpndev-"))
    inputs = {"sourceSha256": args.source_sha256, "uid": args.uid, "run": args.run}
    if work.exists():
        require(args.resume and json.loads((work / "BUILD-INPUTS.json").read_text()) == inputs, "Existing build requires --resume and identical inputs")
    else:
        require(not target.exists(), "Target directory already exists")
        work.mkdir(parents=True)
        (work / "BUILD-INPUTS.json").write_text(json.dumps(inputs, indent=2) + "\n")
        source_archive(args.source_zip, args.source_sha256, work / "source")
        # Seed only public package caches; never copy the host's Cargo configuration or credentials.
        cache = Path("/root/.cargo/registry")
        if cache.is_dir():
            shutil.copytree(cache, work / "cargo-home/registry")
        if Path("/root/.cache/tauri").is_dir():
            shutil.copytree("/root/.cache/tauri", work / "cache/tauri")
    source = work / "source"
    verify_source(source, receipt)
    dev = json.loads((source / "src-tauri/tauri.vpn-dev.conf.json").read_text())
    overlay = json.loads((source / "src-tauri/tauri.vpn-linux-package.conf.json").read_text())
    require(dev["identifier"] == IDENTITY and overlay["identifier"] == IDENTITY and overlay["mainBinaryName"] == "termterm-vpn-dev"
            and not dev["plugins"]["updater"]["endpoints"] and not overlay["bundle"]["createUpdaterArtifacts"]
            and not overlay["bundle"]["linux"]["deb"]["conflicts"] and not overlay["bundle"]["linux"]["deb"]["replaces"], "Isolated identity and release protections required")
    version = dev["version"]
    engine_receipt = json.loads((ENGINE_BUILD / "ENGINE-RECEIPT.json").read_text())
    vendor_inputs = args.vendor_inputs.absolute()
    vendor_manifest = json.loads((vendor_inputs / "INPUTS.json").read_text())
    require(digest(vendor_inputs / "INPUTS.json") == engine_receipt["inputManifestSha256"], "Vendor input receipt differs from the engine build")
    vendor_files = ["openconnect-9.21.tar.gz", "openvpn3-source.tar", "asio-1.38.2.tar.gz", "fmt-12.2.0.tar.gz", "lz4-1.10.0.tar.gz", "fmt-backport-4813.patch", "target-lz4-lz4.diff"]
    for name in vendor_files:
        require(not (vendor_inputs / name).is_symlink() and digest(vendor_inputs / name) == vendor_manifest["files"][name], "Vendor source changed: " + name)
    for name, pin in engine_receipt["binaries"].items():
        require(digest(PREFIX / "bin" / name) == pin["sha256"], "Previously built engine changed: " + name)
    def virtual(path):
        return "/" + path.relative_to(GUEST).as_posix()
    environment = {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "HOME": "/root", "LC_ALL": "C"}
    log = (work / "BUILD.log").open("a")
    def command(arguments, cwd=source, capture=False, extra=()):
        invocation = ["/usr/sbin/chroot", str(GUEST), "/usr/bin/env", "-C", virtual(cwd),
                      "PATH=/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin", "HOME=" + virtual(work), "LC_ALL=C",
                      "CARGO_HOME=" + virtual(work / "cargo-home"), "CARGO_TARGET_DIR=" + virtual(target),
                      "XDG_CACHE_HOME=" + virtual(work / "cache"), "TERMTERM_RELEASE_CHANNEL=development", "CARGO_BUILD_JOBS=2",
                      "GOROOT=/opt/go", "GOTELEMETRY=off", "GOTOOLCHAIN=local", "APPIMAGE_EXTRACT_AND_RUN=1", *extra, *map(str, arguments)]
        result = sp.run(invocation, env=environment, check=True, stdout=sp.PIPE if capture else log, stderr=log, text=True)
        log.flush()
        return result.stdout if capture else None
    print("Installing locked frontend dependencies in the isolated guest", flush=True)
    command(["pnpm", "install", "--frozen-lockfile"])
    print("Rebuilding packet helpers from the frozen application source", flush=True)
    engine_bin = work / "engine-bin"
    engine_bin.mkdir(exist_ok=True)
    core = work / "core-source"
    with tarfile.open(vendor_inputs / "openvpn3-source.tar") as archive:
        expected_core = {member.name: hashlib.sha256(archive.extractfile(member).read()).hexdigest() for member in archive.getmembers() if member.isfile()}
        if not core.exists():
            core.mkdir()
            archive.extractall(core, filter="data")
        require({path.relative_to(core).as_posix() for path in core.rglob("*") if path.is_file()} == set(expected_core), "Unexpected Core source file set")
        verify_source(core, expected_core)
    command(["cmake", "-S", "vpn-engine", "-B", virtual(work / "build-engines"), "-G", "Ninja", "-DCMAKE_BUILD_TYPE=Release",
             "-DOPENVPN_CORE=" + virtual(core), "-DCMAKE_PREFIX_PATH=/opt/termterm-vpn-dev",
             "-DCMAKE_BUILD_RPATH=/opt/termterm-vpn-dev/lib", "-DCMAKE_INSTALL_RPATH=$ORIGIN/../lib"], extra=("PKG_CONFIG_PATH=/opt/termterm-vpn-dev/lib/pkgconfig",))
    command(["cmake", "--build", virtual(work / "build-engines"), "--parallel", "2"])
    for name in ["termterm-openconnect", "termterm-openvpn"]:
        shutil.copyfile(work / "build-engines" / name, engine_bin / name)
    command(["go", "test", "-count=1", "./..."], cwd=source / "vpn-wireguard")
    wireguard = source / ".tools/vpn-linux/termterm-wireguard"
    command(["python3", "scripts/build-wireguard.py", "--target", "linux", "--arch", "amd64", "--output", virtual(wireguard)])
    shutil.copyfile(wireguard, engine_bin / "termterm-wireguard")
    print("Building the policy helper and isolated desktop", flush=True)
    command(["cargo", "build", "--locked", "--release", "--manifest-path", "vpn-core/Cargo.toml", "--bin", "vpn-policy-lab"])
    command(["pnpm", "exec", "tauri", "build", "--ci", "--no-bundle", "--config", "src-tauri/tauri.vpn-dev.conf.json", "--config", "src-tauri/tauri.vpn-linux-package.conf.json"])
    client = target / "release/termterm-vpn-dev"
    require(client.is_file() and client.read_bytes()[:4] == b"\x7fELF", "Isolated Linux executable missing")
    for asset in (source / "dist-vpn-dev/assets").glob("*.js"):
        require(not re.search(r"__nativeEvents|__E2E__", asset.read_text()), "Frontend test hook found")
    rootfs = work / "broker-rootfs"
    require(not rootfs.exists(), "Broker stage already exists; preserve the prior stage")
    helper = rootfs / "opt/termterm-vpn-dev/bin"
    helper.mkdir(parents=True)
    pins = {}
    for name in ["termterm-openconnect", "termterm-openvpn", "termterm-wireguard", "vpn-policy-lab"]:
        original = target / "release" / name if name == "vpn-policy-lab" else engine_bin / name
        shutil.copyfile(original, helper / name)
        (helper / name).chmod(0o755)
        pins[name] = digest(helper / name)
    for name in ["vpn-unix-broker.py", "vpn-lab-broker.py"]:
        shutil.copyfile(source / "scripts" / name, helper / name)
    spec = importlib.util.spec_from_file_location("prepare_vpn_unix", source / "scripts/prepare-vpn-unix.py")
    unix = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(unix)
    unit = rootfs / "usr/lib/systemd/system/termterm-vpn-dev.service"
    unit.parent.mkdir(parents=True)
    unit.write_text(unix.LINUX_UNIT.replace("[Unit]\n", "[Unit]\nConditionPathExists=/etc/termterm-vpn-dev/broker.json\n").replace("[Service]\n", "[Service]\nEnvironment=LD_LIBRARY_PATH=/opt/termterm-vpn-dev/lib\n"))
    config = rootfs / "etc/termterm-vpn-dev/broker.json"
    config.parent.mkdir(parents=True)
    config.write_text(json.dumps({"uid": args.uid, "client": "/usr/bin/termterm-vpn-dev", "clientSha256": digest(client),
                                 "engines": "/opt/termterm-vpn-dev/bin", "pins": pins, "journal": "/var/lib/termterm-vpn-dev"}, indent=2) + "\n")
    config.chmod(0o600)
    config.parent.chmod(0o700)
    packages = {}
    def system_package(path):
        candidates = [path, "/usr" + path] if path.startswith("/lib/") else [path]
        for candidate in candidates:
            try:
                owner = command(["dpkg-query", "-S", candidate], capture=True).splitlines()[0].split(": ", 1)[0]
                break
            except sp.CalledProcessError:
                if candidate == candidates[-1]:
                    raise
        metadata = command(["dpkg-query", "-W", "-f=${Package}\t${Version}\t${source:Package}\t${source:Version}\n", owner], capture=True).strip().split("\t")
        packages[owner] = dict(zip(["package", "version", "sourcePackage", "sourceVersion"], metadata))
        return owner.split(":", 1)[0]
    def libraries(binary, destination):
        listing = command(["ldd", virtual(binary)], capture=True, extra=("LD_LIBRARY_PATH=/opt/termterm-vpn-dev/lib",))
        require("not found" not in listing, "Missing dynamic dependency: " + str(binary))
        for name, path in re.findall(r"^\s*(\S+) => (/\S+) \(", listing, re.M):
            if name in BASE_LIBS:
                continue
            original = GUEST / path.lstrip("/")
            destination.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(original, destination / name)
            if not path.startswith("/opt/termterm-vpn-dev/"):
                system_package(path)
        return listing
    library_path = rootfs / "opt/termterm-vpn-dev/lib"
    dynamic = {name: libraries(helper / name, library_path) for name in ["termterm-openconnect", "termterm-openvpn", "vpn-policy-lab"]}
    shutil.copyfile(PREFIX / "lib/libfmt.so.12", library_path / "libfmt.so.12")
    mosh = work / "mosh"
    (mosh / "bin").mkdir(parents=True)
    shutil.copyfile(GUEST / "usr/bin/mosh-client", mosh / "bin/mosh-client")
    (mosh / "bin/mosh-client").chmod(0o755)
    system_package("/usr/bin/mosh-client")
    libraries(mosh / "bin/mosh-client", mosh / "lib")
    command(["patchelf", "--set-rpath", "$ORIGIN/../lib", virtual(mosh / "bin/mosh-client")])
    notices = work / "dependency-notices"
    notices.mkdir()
    for owner in packages:
        name = owner.split(":", 1)[0]
        original = GUEST / "usr/share/doc" / name / "copyright"
        require(original.is_file(), "Dependency copyright missing: " + name)
        shutil.copyfile(original, notices / (name + "-copyright.txt"))
    (notices / "PACKAGES.json").write_text(json.dumps(packages, indent=2) + "\n")
    for filename, member, destination in [("openconnect-9.21.tar.gz", "openconnect-9.21/COPYING.LGPL", "OpenConnect-LGPL.txt"),
                                           ("fmt-12.2.0.tar.gz", "fmt-12.2.0/LICENSE", "fmt-LICENSE.txt"),
                                           ("lz4-1.10.0.tar.gz", "lz4-1.10.0/lib/LICENSE", "lz4-LICENSE.txt")]:
        with tarfile.open(vendor_inputs / filename) as archive:
            (notices / destination).write_bytes(archive.extractfile(member).read())
    shutil.copytree(notices, mosh / "licenses")
    shutil.copytree(mosh, source / "src-tauri/resources/mosh-native")
    resources = {"../docs/licenses/": "licenses/", "../docs/THIRD_PARTY.md": "THIRD_PARTY.md", "resources/mosh-native/": "mosh/"}
    resource_config = work / "tauri.vpn-resources.conf.json"
    resource_config.write_text(json.dumps({"bundle": {"resources": resources}}, indent=2) + "\n")
    print("Bundling DEB and AppImage with the explicit resource map", flush=True)
    bundle_formats(client, work / "client-unbundled", ["pnpm", "exec", "tauri", "bundle", "--ci", "--no-sign", "--config", "src-tauri/tauri.vpn-dev.conf.json", "--config", "src-tauri/tauri.vpn-linux-package.conf.json", "--config", virtual(resource_config)], command)
    verify_source(source, receipt)
    output.mkdir(parents=True)
    desktop_deb = None
    for extension, directory in [("*.deb", "deb"), ("*.AppImage", "appimage")]:
        files = list((target / "release/bundle" / directory).glob(extension))
        require(len(files) == 1, "Expected one " + directory + " desktop package")
        shutil.copyfile(files[0], output / files[0].name)
        if directory == "deb":
            desktop_deb = files[0]
    metadata = sp.check_output(["dpkg-deb", "--field", str(desktop_deb)], text=True)
    require(re.search(r"^Package: " + re.escape(DESKTOP_PACKAGE) + "$", metadata, re.M) and re.search(r"^Version: " + re.escape(version) + "$", metadata, re.M)
            and not re.search(r"^(Conflicts|Replaces):\s*\S", metadata, re.M), "Desktop DEB identity or compatibility boundary changed")
    with tarfile.open(fileobj=io.BytesIO(sp.check_output(["dpkg-deb", "--fsys-tarfile", str(desktop_deb)]))) as archive:
        members = [member for member in archive if member.name in {"usr/bin/termterm-vpn-dev", "./usr/bin/termterm-vpn-dev"}]
        require(len(members) == 1 and members[0].isfile(), "Installed development client must be one regular file")
        member = members[0]
        installed_client_sha256 = hashlib.sha256(archive.extractfile(member).read()).hexdigest()
    installed_config = json.loads(config.read_text())
    installed_config["clientSha256"] = installed_client_sha256
    config.write_text(json.dumps(installed_config, indent=2) + "\n")
    documentation = rootfs / "usr/share/doc/termterm-vpn-dev-broker"
    shutil.copytree(source / "docs/licenses/vpn", documentation / "licenses")
    shutil.copytree(notices, documentation / "system-dependencies")
    shutil.copyfile(source / "docs/VPN_UNIX_KURULUM_TR.md", documentation / "VPN_UNIX_KURULUM_TR.md")
    control = rootfs / "DEBIAN"
    control.mkdir()
    (control / "control").write_text(f"Package: termterm-vpn-dev-broker\nVersion: {version}\nArchitecture: amd64\nMaintainer: TermTerm Development\nDepends: {DESKTOP_PACKAGE} (= {version}), python3 (>= 3.10), iproute2, systemd-resolved, binutils, ca-certificates, libc6 (>= 2.35), libstdc++6, libgcc-s1\nSection: net\nPriority: optional\nDescription: Isolated TermTerm VPN development broker (inactive until manually configured)\n")
    (control / "conffiles").write_text("/etc/termterm-vpn-dev/broker.json\n")
    broker_deb = output / f"termterm-vpn-dev-broker_{version}_amd64.deb"
    command(["dpkg-deb", "--root-owner-group", "--build", virtual(rootfs), virtual(work / broker_deb.name)])
    shutil.copyfile(work / broker_deb.name, broker_deb)
    shutil.copyfile(args.source_zip, output / "termterm-vpn-source.zip")
    shutil.copyfile(ENGINE_BUILD / "ENGINE-RECEIPT.json", output / "ENGINE-RECEIPT.json")
    (output / "SOURCE-RECEIPT.json").write_text(json.dumps(receipt, indent=2) + "\n")
    dependency_archive = output / "termterm-vpn-linux-dependency-source.tar.gz"
    with tarfile.open(dependency_archive, "w:gz") as archive:
        for name in vendor_files + ["INPUTS.json"]:
            archive.add(vendor_inputs / name, arcname="vendor/" + name)
        archive.add(notices, arcname="system-dependencies")
    (output / "README.txt").write_text(f"TermTerm VPN Dev {version}, Linux x86_64, development evaluation only.\nBuilt in isolated Ubuntu 22.04 with pinned Rust/Node and OpenConnect 9.21.\nDesktop DEB and AppImage have the separate termterm-vpn-dev executable and no updater.\nThe broker DEB contains no maintainer scripts and never enables/starts a service.\nIts config pins /usr/bin/termterm-vpn-dev and nonroot UID {args.uid}. A root administrator must verify the selected UID and all ownership/dependencies before manually starting termterm-vpn-dev.service.\nThe AppImage is a desktop preview; its transient executable path is deliberately not authorized by this broker config. Install the separate desktop DEB for broker pairing.\nDo not run this as an upgrade to stable TermTerm. No stable application/vault is part of this payload.\nA matching public desktop source ZIP and patched vendor source are included. Ubuntu shared-library source package names/versions and original copyright/source-location notices are in the dependency-source archive. System libraries and the AppImage runtime retain their own licensing.\nNo package installation, desktop launch, native VPN connection, broker startup or production authentication was performed for this build. Native acceptance remains required; distributionReady=false.\nSee the packaged VPN_UNIX_KURULUM_TR.md for DNS prerequisites and cleanup policy.\n")
    package_receipt = {"version": version, "identity": IDENTITY, "developmentOnly": True, "distributionReady": False,
                       "installed": False, "serviceStarted": False, "desktopLaunched": False, "nativeVpnTested": False,
                       "sourceSha256": args.source_sha256, "unbundledClientSha256": digest(client), "debClientSha256": installed_client_sha256, "brokerUid": args.uid,
                       "guest": str(GUEST), "tools": json.loads((GUEST.parent / "TOOLS.json").read_text()),
                       "engineReceiptSha256": digest(ENGINE_BUILD / "ENGINE-RECEIPT.json"), "dynamicDependencies": dynamic,
                       "packetHelpersRebuiltFromSourceSnapshot": True, "systemDependencyPackages": packages, "brokerPins": pins,
                       "files": {path.name: {"sha256": digest(path), "bytes": path.stat().st_size} for path in sorted(output.iterdir())}}
    (output / "PACKAGE-RECEIPT.json").write_text(json.dumps(package_receipt, indent=2) + "\n")
    (output / "SHA256SUMS.txt").write_text("".join(digest(path) + "  " + path.name + "\n" for path in sorted(output.iterdir()) if path.name != "SHA256SUMS.txt"))
    print("Prepared local Linux development packages only: " + str(output), flush=True)


def self_test():
    for name in ["../private", "/absolute", "C:private", ".tools/private", "host.pem", ".env", "x/../../x", "a\\b", "vault.ttvault"]:
        try:
            safe_name(name)
        except ValueError:
            pass
        else:
            raise AssertionError("Unsafe package source accepted: " + name)
    with tempfile.TemporaryDirectory() as temporary:
        client = Path(temporary) / "client"
        backup = Path(temporary) / "backup"
        original = b"inert __TAURI_BUNDLE_TYPE_VAR_UNK fixture"
        client.write_bytes(original)
        formats = []
        def fake_bundle(arguments):
            assert client.read_bytes() == original
            formats.append(arguments[-1])
            client.write_bytes(original.replace(b"_UNK", b"_DEB"))
        bundle_formats(client, backup, ["inert-tool"], fake_bundle)
        assert formats == ["deb", "appimage"] and client.read_bytes() == original and backup.read_bytes() == original
        def failed_bundle(arguments):
            client.write_bytes(b"failed bundle mutation")
            raise RuntimeError("inert bundle failure")
        try:
            bundle_formats(client, backup, [], failed_bundle)
        except RuntimeError:
            assert client.read_bytes() == original
        else:
            raise AssertionError("Failed bundler did not propagate its error")
        try:
            delivery_path(Path(temporary) / "vpn-delivery-20261103/linux", Path(temporary) / "repository")
        except ValueError:
            pass
        else:
            raise AssertionError("Output escaped the repository delivery root")
        path = Path(temporary) / "source.zip"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr(".env.vpn-dev", b"VITE_VPN_DEV=1\n")
            archive.writestr("SOURCE-RECEIPT.json", json.dumps({".env.vpn-dev": "0" * 64}))
        try:
            source_archive(path, digest(path))
        except ValueError as error:
            assert "SHA256" in str(error)
        else:
            raise AssertionError("Changed source member accepted")
    print("Inert Linux packaging source/path checks passed; no guest or service action")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-zip", type=Path)
    parser.add_argument("--source-sha256")
    parser.add_argument("--vendor-inputs", type=Path, default=Path(__file__).resolve().parents[1] / ".tools/vpn-linux-inputs/v1")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--run", default="vpndev-20261103")
    parser.add_argument("--uid", type=int, default=1000)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_test:
        self_test()
    elif arguments.source_zip and arguments.source_sha256 and arguments.output:
        build(arguments)
    else:
        parser.error("Provide --source-zip, --source-sha256 and --output, or --self-test")
