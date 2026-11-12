#!/usr/bin/env python3
"""Build local Linux development/candidate packages; never install, start services or publish."""
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


def package_contract(candidate=None):
    require(candidate in (None, "0.4.5"), "Unsupported candidate version")
    return {"identity": "local.termterm.desktop" if candidate else IDENTITY,
            "name": "termterm" if candidate else "termterm-vpn-dev",
            "service": "termterm-vpn" if candidate else "termterm-vpn-dev",
            "package": "term-term" if candidate else DESKTOP_PACKAGE,
            "config": "src-tauri/tauri.vpn-candidate.conf.json" if candidate else "src-tauri/tauri.vpn-dev.conf.json",
            "overlay": "src-tauri/tauri.vpn-candidate-linux.conf.json" if candidate else "src-tauri/tauri.vpn-linux-package.conf.json",
            "dist": "dist-vpn-candidate" if candidate else "dist-vpn-dev"}


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify_packaged_helper_rpath(dynamic):
    require(re.findall(r"\((?:RPATH|RUNPATH)\).*\[(.*?)\]", dynamic) == ["$ORIGIN/../lib"],
            "Packaged VPN helper requires exactly $ORIGIN/../lib, without build or development paths")


def copy_library(original, destination):
    require(destination.parent.resolve() == destination.parent and not destination.is_symlink(), "Library destination cannot contain links")
    if original.resolve() == destination.resolve():
        require(destination.is_file(), "Existing library must be a regular file")
        return False
    shutil.copyfile(original, destination)
    return True


def safe_name(name):
    path = PurePosixPath(name)
    require(name and "\\" not in name and ":" not in name and not path.is_absolute()
            and ".." not in path.parts and path.as_posix() == name, "Unsafe source member")
    require(not any(part in {".git", ".lab", ".tools", "node_modules", "target", "artifacts"} for part in path.parts), "Private/build source member")
    require(path.suffix.lower() not in {".pem", ".key", ".pfx", ".p12", ".dump", ".exe", ".dll"}
            and not any(part.startswith(".env") and part not in {".env.vpn-dev", ".env.vpn-candidate"} for part in path.parts)
            and not any(part.startswith(".") and part not in {".env.vpn-dev", ".env.vpn-candidate"} for part in path.parts)
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
        if ".env.vpn-candidate" in names:
            require(archive.read(".env.vpn-candidate").strip() == b"VITE_VPN_CANDIDATE=0.4.5", "Unexpected candidate environment")
        return receipt


def verify_source(source, receipt):
    for name, expected in receipt.items():
        path = source / name
        require(not path.is_symlink() and path.is_file() and digest(path) == expected, "Frozen source changed: " + name)


def delivery_path(path, root, candidate=None):
    path = path.absolute()
    require(path.resolve() == path and not any(parent.is_symlink() for parent in [path, *path.parents]), "Delivery paths cannot contain links")
    delivery = path.parent.parent if candidate else path.parent
    require(candidate in (None, "0.4.5") and (not candidate or path.parent.name == "candidate-0.4.5")
            and path.name == "linux" and re.fullmatch(r"vpn-delivery-[0-9]{8}", delivery.name)
            and delivery.parent == root / "artifacts" and not path.exists(), "Choose a new directory inside this worktree's artifact delivery root")
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
    selected = package_contract(args.candidate)
    require(not args.release_candidate or args.candidate == "0.4.5", "Release candidate requires the exact --candidate 0.4.5 contract")
    require(not args.candidate or args.uid is not None, "Candidate pairing requires an explicit nonroot --uid selected by the administrator")
    args.uid = args.uid if args.uid is not None else 1000
    require(re.fullmatch(r"[a-z0-9-]{1,40}", args.run) is not None and args.uid > 0, "A simple build name and nonroot UID are required")
    require(GUEST.resolve() == GUEST and (GUEST / ".termterm-vpn-build-root").read_text() == "isolated-vpndev-jammy\n", "Separate marked Jammy build guest required")
    require((GUEST / "usr/sbin/policy-rc.d").read_text() == "#!/bin/sh\nexit 101\n", "Guest service autostart must remain blocked")
    output = delivery_path(args.output, Path(__file__).resolve().parents[1], args.candidate)
    receipt = source_archive(args.source_zip, args.source_sha256)
    require(receipt.get("scripts/package-vpn-linux.py") == digest(Path(__file__)), "Packaging script differs from the frozen public source")
    work = GUEST / "work" / args.run
    target = GUEST / "opt/termterm-vpn-build" / ("desktop-target-" + args.run.removeprefix("vpndev-"))
    inputs = {"sourceSha256": args.source_sha256, "uid": args.uid, "run": args.run, "candidate": args.candidate, "releaseCandidate": args.release_candidate}
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
    dev = json.loads((source / selected["config"]).read_text())
    overlay = json.loads((source / selected["overlay"]).read_text())
    require(dev["identifier"] == selected["identity"] and overlay.get("identifier", dev["identifier"]) == selected["identity"]
            and overlay.get("mainBinaryName", dev.get("mainBinaryName")) == selected["name"]
            and not dev["plugins"]["updater"]["endpoints"] and not overlay["bundle"]["createUpdaterArtifacts"]
            and not overlay["bundle"]["linux"]["deb"]["conflicts"] and not overlay["bundle"]["linux"]["deb"]["replaces"], "Isolated identity and release protections required")
    version = dev["version"]
    if args.candidate:
        require(version == args.candidate and dev["build"]["features"] == ["vpn-candidate"], "Exact candidate version and compiled feature required")
    configs = ["--config", selected["config"], "--config", selected["overlay"]]
    if args.release_candidate:
        release_path = "src-tauri/tauri.vpn-release-candidate.conf.json"
        release = json.loads((source / release_path).read_text())
        stable = json.loads((source / "src-tauri/tauri.conf.json").read_text())
        require(release["version"] == args.candidate and release["identifier"] == selected["identity"] and release["mainBinaryName"] == selected["name"]
                and release["plugins"]["updater"] == {"endpoints": stable["plugins"]["updater"]["endpoints"]}
                and bool(stable["plugins"]["updater"]["pubkey"]) and not release["bundle"]["createUpdaterArtifacts"], "Existing signed stable updater contract must be preserved")
        configs += ["--config", release_path]
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
                      "XDG_CACHE_HOME=" + virtual(work / "cache"), "TERMTERM_RELEASE_CHANNEL=" + ("stable" if args.release_candidate else "candidate" if args.candidate else "development"), "CARGO_BUILD_JOBS=2",
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
             "-DCMAKE_BUILD_RPATH=/opt/termterm-vpn-dev/lib", "-DCMAKE_INSTALL_RPATH=$ORIGIN/../lib", "-DCMAKE_BUILD_WITH_INSTALL_RPATH=ON"], extra=("PKG_CONFIG_PATH=/opt/termterm-vpn-dev/lib/pkgconfig",))
    command(["cmake", "--build", virtual(work / "build-engines"), "--parallel", "2"])
    for name in ["termterm-openconnect", "termterm-openvpn"]:
        shutil.copyfile(work / "build-engines" / name, engine_bin / name)
    command(["go", "test", "-count=1", "./..."], cwd=source / "vpn-wireguard")
    wireguard = source / ".tools/vpn-linux/termterm-wireguard"
    command(["python3", "scripts/build-wireguard.py", "--target", "linux", "--arch", "amd64", "--output", virtual(wireguard)])
    shutil.copyfile(wireguard, engine_bin / "termterm-wireguard")
    print("Building the policy helper and isolated desktop", flush=True)
    command(["cargo", "build", "--locked", "--release", "--manifest-path", "vpn-core/Cargo.toml", "--bin", "vpn-policy-lab"])
    command(["pnpm", "exec", "tauri", "build", "--ci", "--no-bundle", *configs])
    client = target / "release" / selected["name"]
    require(client.is_file() and client.read_bytes()[:4] == b"\x7fELF", "Isolated Linux executable missing")
    for asset in (source / selected["dist"] / "assets").glob("*.js"):
        require(not re.search(r"__nativeEvents|__E2E__", asset.read_text()), "Frontend test hook found")
    rootfs = work / "broker-rootfs"
    require(not rootfs.exists(), "Broker stage already exists; preserve the prior stage")
    helper = rootfs / "opt" / selected["service"] / "bin"
    helper.mkdir(parents=True)
    pins = {}
    for name in ["termterm-openconnect", "termterm-openvpn", "termterm-wireguard", "vpn-policy-lab"]:
        original = target / "release" / name if name == "vpn-policy-lab" else engine_bin / name
        if name in {"termterm-openconnect", "termterm-openvpn"}:
            verify_packaged_helper_rpath(command(["readelf", "-d", virtual(original)], capture=True))
        shutil.copyfile(original, helper / name)
        (helper / name).chmod(0o755)
        pins[name] = digest(helper / name)
    for name in ["vpn-unix-broker.py", "vpn-lab-broker.py"]:
        shutil.copyfile(source / "scripts" / name, helper / name)
    if args.candidate:
        shutil.copyfile(source / "scripts/install-vpn-appimage.py", helper / "install-vpn-appimage.py")
    spec = importlib.util.spec_from_file_location("prepare_vpn_unix", source / "scripts/prepare-vpn-unix.py")
    unix = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(unix)
    unit = rootfs / "usr/lib/systemd/system" / (selected["service"] + ".service")
    unit.parent.mkdir(parents=True)
    unit.write_text(unix.linux_unit(args.candidate).replace("[Unit]\n", "[Unit]\nConditionPathExists=/etc/" + selected["service"] + "/broker.json\n").replace("[Service]\n", "[Service]\nEnvironment=LD_LIBRARY_PATH=/opt/" + selected["service"] + "/lib\n"))
    config = rootfs / "etc" / selected["service"] / "broker.json"
    config.parent.mkdir(parents=True)
    broker_config = {"uid": args.uid, "client": "/usr/bin/" + selected["name"], "clientSha256": digest(client),
                     "engines": "/opt/" + selected["service"] + "/bin", "pins": pins, "journal": "/var/lib/" + selected["service"]}
    if args.candidate:
        broker_config.update(applicationIdentifier=selected["identity"], candidateVersion=args.candidate)
    config.write_text(json.dumps(broker_config, indent=2) + "\n")
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
            if not copy_library(original, destination / name):
                continue
            if not path.startswith("/opt/termterm-vpn-dev/"):
                system_package(path)
        return listing
    library_path = rootfs / "opt" / selected["service"] / "lib"
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
    bundle_formats(client, work / "client-unbundled", ["pnpm", "exec", "tauri", "bundle", "--ci", "--no-sign", *configs, "--config", virtual(resource_config)], command)
    verify_source(source, receipt)
    output.mkdir(parents=True)
    desktop_deb, appimage_manifest = None, None
    for extension, directory in [("*.deb", "deb"), ("*.AppImage", "appimage")]:
        files = list((target / "release/bundle" / directory).glob(extension))
        require(len(files) == 1, "Expected one " + directory + " desktop package")
        shutil.copyfile(files[0], output / files[0].name)
        if directory == "deb":
            desktop_deb = files[0]
        elif args.candidate:
            prepared = work / "APPIMAGE-INSTALL.json"
            command(["python3", "scripts/install-vpn-appimage.py", "--prepare", "--appimage", virtual(files[0]), "--image-sha256", digest(files[0]),
                     "--manifest", virtual(prepared), "--uid", "65534"])
            appimage_manifest = json.loads(prepared.read_text())
            shutil.copyfile(prepared, output / prepared.name)
    metadata = sp.check_output(["dpkg-deb", "--field", str(desktop_deb)], text=True)
    require(re.search(r"^Package: " + re.escape(selected["package"]) + "$", metadata, re.M) and re.search(r"^Version: " + re.escape(version) + "$", metadata, re.M)
            and not re.search(r"^(Conflicts|Replaces):\s*\S", metadata, re.M), "Desktop DEB identity or compatibility boundary changed")
    with tarfile.open(fileobj=io.BytesIO(sp.check_output(["dpkg-deb", "--fsys-tarfile", str(desktop_deb)]))) as archive:
        members = [member for member in archive if member.name.removeprefix("./") == "usr/bin/" + selected["name"]]
        require(len(members) == 1 and members[0].isfile(), "Installed development client must be one regular file")
        member = members[0]
        installed_client_sha256 = hashlib.sha256(archive.extractfile(member).read()).hexdigest()
    installed_config = json.loads(config.read_text())
    installed_config["clientSha256"] = installed_client_sha256
    config.write_text(json.dumps(installed_config, indent=2) + "\n")
    documentation = rootfs / "usr/share/doc" / (selected["service"] + "-broker")
    shutil.copytree(source / "docs/licenses/vpn", documentation / "licenses")
    shutil.copytree(notices, documentation / "system-dependencies")
    shutil.copyfile(source / "docs/VPN_UNIX_KURULUM_TR.md", documentation / "VPN_UNIX_KURULUM_TR.md")
    control = rootfs / "DEBIAN"
    control.mkdir()
    desktop_dependency = "" if args.candidate else selected['package'] + " (= " + version + "), "
    (control / "control").write_text(f"Package: {selected['service']}-broker\nVersion: {version}\nArchitecture: amd64\nMaintainer: TermTerm Development\nDepends: {desktop_dependency}python3 (>= 3.10), iproute2, systemd-resolved, binutils, util-linux, ca-certificates, libc6 (>= 2.35), libstdc++6, libgcc-s1\nSection: net\nPriority: optional\nDescription: TermTerm VPN broker (inactive until manually configured for the matching DEB or fixed AppDir)\n")
    (control / "conffiles").write_text("/etc/" + selected["service"] + "/broker.json\n")
    broker_deb = output / f"{selected['service']}-broker_{version}_amd64.deb"
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
    (output / "README.txt").write_text(f"""TermTerm {'production candidate' if args.candidate else 'VPN Dev'} {version}, Linux x86_64. Local evaluation only; not an approved release.
Built in isolated Ubuntu 22.04 with pinned Rust/Node and OpenConnect 9.21.
Desktop executable: /usr/bin/{selected['name']}; {'optional updates retain the existing owner-controlled feed and signature verification; publication of these exact local bytes still requires separate approval' if args.release_candidate else 'updater disabled'}.
The broker DEB has no maintainer scripts and never enables/starts a service.
Its configuration pins the exact installed desktop bytes and administrator-selected nonroot UID {args.uid}. A root administrator must verify this UID, root ownership, DNS prerequisites and every runtime dependency before manually starting {selected['service']}.service.
{'For AppImage VPN, install the broker DEB, verify APPIMAGE-INSTALL.json against its reviewed SHA256, then use the packaged install-vpn-appimage.py as root with the explicit UID. It extracts as nonroot, verifies the complete inventory, installs /opt/termterm-appimage and /usr/bin/termterm-appimage, and pins the exact installed ELF. Start that fixed launcher as the normal user. Direct transient AppImage mounts are never authorized. Stop the broker and close the installed AppDir before an explicit administrator upgrade/repin; old files/config are retained. Root-owned AppDir updates are manual administrator operations.' if args.candidate else 'The development AppImage is an unpaired desktop preview; install the matching desktop DEB for VPN pairing.'}
{'This payload uses the production identity: evaluate only in a separate disposable machine; never install over the owner protected stable app.' if args.candidate else 'The separate development identity does not replace stable TermTerm.'}
Matching public application source, patched vendor source, package versions and original copyright/source-location notices are included.
No package installation, desktop launch, native VPN connection, broker startup or production authentication was performed by this build. Native acceptance remains required; distributionReady=false.
See VPN_UNIX_KURULUM_TR.md for DNS prerequisites and cleanup policy.
""")
    package_receipt = {"version": version, "identity": selected["identity"], "candidateVersion": args.candidate, "developmentOnly": not bool(args.candidate), "distributionReady": False,
                       "releaseCandidate": args.release_candidate, "updateChannel": "stable" if args.release_candidate else "candidate" if args.candidate else "development",
                       "appImageVpn": "root-owned-installation" if args.candidate else "preview-unpaired",
                       "appImageClientSha256": appimage_manifest["clientSha256"] if appimage_manifest else None,
                       "appImageManifestSha256": digest(output / "APPIMAGE-INSTALL.json") if appimage_manifest else None,
                       "desktopPackage": selected["package"], "brokerService": selected["service"] + ".service",
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
        source = Path(temporary) / "system-library"
        source.write_bytes(b"inert dependency")
        library = Path(temporary) / "lib/libfixture.so"
        library.parent.mkdir()
        assert copy_library(source, library)
        assert not copy_library(library.parent / "../lib/libfixture.so", library)
        assert library.read_bytes() == source.read_bytes()
        library.unlink()
        library.symlink_to(source)
        try:
            copy_library(source, library)
        except ValueError:
            pass
        else:
            raise AssertionError("Linked library destination accepted")
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
    parser.add_argument("--uid", type=int)
    parser.add_argument("--candidate", choices=["0.4.5"])
    parser.add_argument("--release-candidate", action="store_true", help="Retain the existing signed stable updater; never publish")
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_test:
        self_test()
    elif arguments.source_zip and arguments.source_sha256 and arguments.output:
        build(arguments)
    else:
        parser.error("Provide --source-zip, --source-sha256 and --output, or --self-test")
