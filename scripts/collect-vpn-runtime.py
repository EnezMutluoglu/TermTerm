#!/usr/bin/env python3
"""Collect verified Windows VPN dependency notices/source; never install or publish."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import shlex
import shutil
import stat
import subprocess as sp
import tarfile
import tempfile
import threading
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / ".tools/vpn-windows"
MSYS = TOOLS / "msys2-portable/msys64"


def digest(path, algorithm="sha256"):
    result = hashlib.new(algorithm)
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            result.update(block)
    return result.hexdigest()


def verify(path, expected, algorithm="sha256"):
    if path.is_symlink() or not path.is_file() or digest(path, algorithm) != expected:
        raise ValueError("Dependency integrity mismatch: " + path.name)


def archive_source(repository, destination):
    # Archive Git objects, never the local port tree containing ignored or untracked files.
    sp.run(["git", "-C", str(repository), "archive", "--output", str(destination), "HEAD"], check=True)


def fields(path):
    return {key: value.strip().splitlines() for key, value in re.findall(r"%([^%]+)%\n(.*?)(?=\n%|\Z)", path.read_text(), re.S)}


def download(url, path):
    if not path.exists():
        with urllib.request.urlopen(url, timeout=90) as response, path.open("xb") as output:
            shutil.copyfileobj(response, output)


def signed_source(package, binary, destination):
    with tarfile.open(binary) as archive:
        info = archive.extractfile(".BUILDINFO").read().decode()
    expected = re.search(r"^pkgbuild_sha256sum = ([0-9a-f]{64})$", info, re.M)
    if not expected:
        raise ValueError("Binary package lacks source recipe fingerprint")
    name = package["base"] + "-" + package["version"] + ".src.tar.zst"
    url = "https://repo.msys2.org/mingw/sources/" + name
    source = destination / name
    signature = destination / (name + ".sig")
    download(url, source)
    download(url + ".sig", signature)
    def posix(path):
        value = path.resolve().as_posix()
        return "/" + value[0].lower() + value[2:]
    # pacman-key requires admin even for reads; use its existing trusted keyring without changing it.
    checked = sp.run([str(MSYS / "usr/bin/bash.exe"), "--login", "-c", "gpg --status-fd 1 --homedir /etc/pacman.d/gnupg --no-auto-check-trustdb --lock-never --verify " + shlex.quote(posix(signature)) + " " + shlex.quote(posix(source))], check=True, capture_output=True, text=True)
    if "[GNUPG:] VALIDSIG " not in checked.stdout or not any("[GNUPG:] TRUST_" + trust in checked.stdout for trust in ["FULLY", "ULTIMATE"]):
        raise ValueError("Source signature has no trusted MSYS2 signer: " + name)
    with tarfile.open(source) as archive:
        recipes = [entry for entry in archive.getmembers() if entry.name.endswith("/PKGBUILD") and entry.isfile()]
        if len(recipes) != 1 or hashlib.sha256(archive.extractfile(recipes[0]).read()).hexdigest() != expected[1]:
            raise ValueError("Signed source differs from binary build recipe: " + name)
    signer = re.search(r"\[GNUPG:\] VALIDSIG ([0-9A-F]{40}) ", checked.stdout)[1]
    return {"file": name, "url": url, "sha256": digest(source), "signatureSha256": digest(signature), "recipeSha256": expected[1], "signatureVerified": True, "signer": signer}


def source_notices(source, destination):
    copied = []
    def is_notice(name):
        return re.fullmatch(r"(?i)(copying\d*|copyright|licen[cs]e|notice)([._-].*)?", Path(name).name)
    def write_notice(name, data):
        target = destination / name
        if not target.resolve().is_relative_to(destination.resolve()):
            raise ValueError("Unsafe source notice path")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        copied.append(target)
    def tar_output(arguments, limit):
        process = sp.Popen(arguments, stdout=sp.PIPE, stderr=sp.DEVNULL)
        timer = threading.Timer(30, process.kill)
        timer.start()
        try:
            data = process.stdout.read(limit + 1)
            if len(data) > limit:
                raise ValueError("Source archive notice/list exceeds its size limit")
            if process.wait() != 0:
                raise ValueError("Existing tar could not read the compressed source notices")
            return data
        finally:
            timer.cancel()
            if process.poll() is None:
                process.kill()
            process.wait()
            process.stdout.close()
    def copy_notices(archive):
        for notice in archive.getmembers():
            if notice.isfile() and notice.size <= 1024 * 1024 and is_notice(notice.name):
                write_notice(notice.name, archive.extractfile(notice).read())
    with tarfile.open(source) as package:
        copy_notices(package)
        for member in package.getmembers():
            if not member.isfile() or not re.search(r"(\.tar(?:\.(gz|xz|bz2|zst|lz))?|\.tgz)$", member.name):
                continue
            if member.name.endswith(".tar.lz"):
                # Python has no lzip reader; the existing Cygwin preparation tar reads bytes only.
                with tempfile.TemporaryDirectory(prefix="vpn-source-notice-") as directory:
                    compressed = Path(directory) / "source.tar.lz"
                    compressed.write_bytes(package.extractfile(member).read())
                    for name in tar_output(["tar", "-tf", str(compressed)], 16 * 1024 * 1024).decode().splitlines():
                        if is_notice(name):
                            write_notice(name, tar_output(["tar", "-xOf", str(compressed), "--", name], 1024 * 1024))
                continue
            with tarfile.open(fileobj=io.BytesIO(package.extractfile(member).read())) as upstream:
                copy_notices(upstream)
    return copied


def no_links(path):
    for member in [path, *path.parents]:
        try:
            metadata = member.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise ValueError("Package paths cannot contain links or reparse points")


def relative_path(value):
    if not isinstance(value, str) or not value or "\\" in value or ":" in value:
        raise ValueError("Invalid package member path")
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or path.as_posix() != value:
        raise ValueError("Invalid package member path")
    return value


def source_paths():
    folders = ["src", "src-tauri/src", "src-tauri/capabilities", "src-tauri/icons", "migrations", "scripts", "templates", "docs", "team-client/src", "team-service/src", "team-service/migrations", "team-service/deploy", "vpn-core/src", "vpn-engine", "vpn-wireguard", "vpn-windows-broker/src"]
    files = ["package.json", "pnpm-lock.yaml", "pnpm-workspace.yaml", "index.html", "app-icon.svg", "tsconfig.json", "vite.config.ts", "rust-toolchain.toml", ".env.vpn-dev", "README.md"]
    for directory in ["src-tauri", "team-client", "team-service", "vpn-core", "vpn-windows-broker"]:
        files += [directory + "/Cargo.toml", directory + "/Cargo.lock"]
    files += ["src-tauri/build.rs", "src-tauri/tests/vpn_webkit_fixture.rs", "team-service/compatibility.json"]
    files += ["src-tauri/tauri.conf.json", "src-tauri/tauri.windows.conf.json", "src-tauri/tauri.linux.conf.json", "src-tauri/tauri.macos.conf.json", "src-tauri/tauri.vpn-dev.conf.json", "src-tauri/tauri.vpn-e2e.conf.json", "src-tauri/tauri.vpn-linux-package.conf.json", "src-tauri/tauri.vpn-windows-package.conf.json"]
    # Only established public paths in HEAD are recursive; new paths require a reviewed explicit addition.
    baseline = sp.check_output(["git", "ls-tree", "-rz", "--name-only", "HEAD", "--", *folders, *files], cwd=ROOT).decode().split("\0")
    inventory = ROOT / "scripts/vpn-public-source-files.json"
    no_links(inventory)
    additions = json.loads(inventory.read_bytes())
    if additions.get("version") != 1 or not isinstance(additions.get("files"), list) or len(set(additions["files"])) != len(additions["files"]):
        raise ValueError("Invalid reviewed public source inventory")
    result = []
    for relative in sorted(set(filter(None, baseline)) | set(additions["files"])):
        relative_path(relative)
        if relative not in files and not any(relative.startswith(folder + "/") for folder in folders):
            raise ValueError("Source inventory path is outside the approved public roots")
        path = ROOT / relative
        no_links(path)
        if not path.is_file():
            if relative in additions["files"]:
                raise ValueError("Reviewed public source file is missing")
            continue
        if relative == ".env.vpn-dev" and path.read_text().strip() != "VITE_VPN_DEV=1":
            raise ValueError("VPN environment contains unexpected data")
        result.append(relative)
    return result


def source_receipt():
    return {relative: digest(ROOT / relative) for relative in source_paths()}


def application_source(destination):
    receipt = {}
    with zipfile.ZipFile(destination, "x", compression=zipfile.ZIP_DEFLATED) as output:
        for relative in source_paths():
            path = ROOT / relative
            data = path.read_bytes()
            output.writestr(relative, data)
            receipt[relative] = hashlib.sha256(data).hexdigest()
        output.writestr("SOURCE-RECEIPT.json", json.dumps(receipt, indent=2) + "\n")
    return {"file": destination.name, "sha256": digest(destination), "files": receipt}


def verified_files(directory, files, manifests=(), algorithm="sha256"):
    no_links(directory)
    if not isinstance(files, dict) or not files:
        raise ValueError("Exact package asset inventory is required")
    expected = {relative_path(name) for name in files}
    actual = set()
    for path in directory.rglob("*"):
        no_links(path)
        if path.is_file():
            actual.add(path.relative_to(directory).as_posix())
    if actual != expected | set(manifests):
        raise ValueError("Package asset directory contains an unexpected file set")
    for name, expected_hash in files.items():
        verify(directory / name, expected_hash, algorithm)
    return sorted(expected)


def mosh_sources(destination, notices):
    mosh = ROOT / "src-tauri/resources/mosh"
    no_links(mosh)
    file_manifest = json.loads((mosh / "runtime-files.json").read_bytes())
    verified_files(mosh, file_manifest["files"], ("runtime-files.json",))
    packages = json.loads((mosh / "packages.json").read_bytes())
    pins = {}
    for package in packages:
        url = package["source"]
        if not url.startswith("https://cygwin.com/pub/cygwin/"):
            raise ValueError("Mosh sources must match the official binary package index")
        name = relative_path(url.rsplit("/", 1)[1])
        pin = {"url": url, "bytes": package["sourceBytes"], "sha512": package["sourceSha512"]}
        if name in pins and pins[name] != pin:
            raise ValueError("Conflicting Mosh source pins")
        pins[name] = pin
    cache = ROOT / ".tools/cygwin/sources"
    no_links(cache)
    manifest = json.loads((cache / "SOURCES.json").read_bytes())
    if manifest.get("version") != 1 or manifest.get("sources") != pins:
        raise ValueError("Mosh sources do not match the binary package source pins")
    files = verified_files(cache, {name: pin["sha512"] for name, pin in pins.items()}, ("SOURCES.json",), "sha512")
    destination.mkdir()
    for name in files + ["SOURCES.json"]:
        if name != "SOURCES.json" and (cache / name).stat().st_size != pins[name]["bytes"]:
            raise ValueError("Mosh source archive size differs from the binary source pin")
        shutil.copyfile(cache / name, destination / name)
    copied_notices = {}
    for name in files:
        copied = source_notices(cache / name, notices / name)
        if not copied:
            raise ValueError("No license notice found in a matching Mosh source archive")
        copied_notices[name] = [path.relative_to(notices).as_posix() for path in copied]
    return {"packagesSha256": digest(mosh / "packages.json"), "sources": pins, "sourceBytes": sum(pin["bytes"] for pin in pins.values()), "notices": copied_notices}


def package_preflight(runtime, output=None):
    tools = ROOT / ".tools/vpn-windows"
    if output is not None:
        no_links(output)
        output = output.resolve()
        if not output.is_relative_to(tools.resolve()) or output.exists():
            raise ValueError("Use a new package output inside the worktree VPN tools directory")
    no_links(runtime)
    runtime = runtime.resolve()
    if not runtime.is_relative_to(tools.resolve()):
        raise ValueError("Use the local verified VPN dependency collection")
    manifest = json.loads((runtime / "RUNTIME.json").read_bytes())
    if not manifest.get("developmentOnly") or manifest.get("distributionReady") or len(manifest.get("runtime", {})) != 23 or manifest.get("missing"):
        raise ValueError("Expected the complete DLL closure for development")
    if any(not value.get("source", {}).get("signatureVerified") for name, value in manifest.get("packages", {}).items() if name.startswith("mingw-")):
        raise ValueError("Every MSYS2 runtime needs its signed matching source")
    runtime_files = verified_files(runtime, manifest.get("files"), ("RUNTIME.json",))
    before = source_receipt()
    if before != manifest.get("applicationSource", {}).get("files"):
        raise ValueError("Public source changed after collection; collect a fresh matching source set")
    no_links(tools / "engine")
    if {path.name for path in (tools / "engine").glob("*.dll")} != set(manifest["runtime"]):
        raise ValueError("Engine directory has an unexpected DLL file set")
    for name, value in manifest["runtime"].items():
        path = tools / "engine" / relative_path(name)
        no_links(path)
        verify(path, value["sha256"])
    assets = []
    for relative in runtime_files:
        if relative.startswith("notices/"):
            assets.append({"source": str(runtime / relative), "target": "vpn-runtime-notices/" + relative.removeprefix("notices/")})
    assets.append({"source": str(runtime / "RUNTIME.json"), "target": "VPN-RUNTIME.json"})
    license_files = {name.removeprefix("docs/licenses/"): value for name, value in before.items() if name.startswith("docs/licenses/")}
    for relative in verified_files(ROOT / "docs/licenses", license_files):
        assets.append({"source": str(ROOT / "docs/licenses" / relative), "target": "licenses/" + relative})
    for name in ["THIRD_PARTY.md", "VPN_WINDOWS_PREPARATION_TR.md", "VPN_TEAM_TR.md", "AUTHENTICATOR_TR.md", "VPN_DEPENDENCIES_TR.md"]:
        if "docs/" + name not in before:
            raise ValueError("Required public package guide is missing")
        assets.append({"source": str(ROOT / "docs" / name), "target": name})
    mosh = ROOT / "src-tauri/resources/mosh"
    no_links(mosh)
    if not (mosh / "runtime-files.json").is_file():
        raise ValueError("Mosh file receipt is missing; run node scripts/fetch-mosh.mjs before packaging")
    mosh_manifest = json.loads((mosh / "runtime-files.json").read_bytes())
    if mosh_manifest.get("version") != 1:
        raise ValueError("Unsupported Mosh runtime file receipt")
    no_links(mosh / "packages.json")
    if manifest.get("mosh", {}).get("packagesSha256") != digest(mosh / "packages.json"):
        raise ValueError("Collected Mosh sources do not match the packaged runtime")
    for relative in verified_files(mosh, mosh_manifest.get("files"), ("runtime-files.json",)):
        assets.append({"source": str(mosh / relative), "target": "mosh/" + relative})
    assets.append({"source": str(mosh / "runtime-files.json"), "target": "mosh/runtime-files.json"})
    return {"sourceReceipt": before, "assets": assets, "runtimeFiles": runtime_files + ["RUNTIME.json"]}


def collect(destination, fetch_sources):
    destination = destination.resolve()
    if not destination.is_relative_to((ROOT / ".tools").resolve()) or destination.exists():
        raise ValueError("Choose a new output inside this worktree's .tools")
    lock = json.loads((ROOT / "scripts/vpn-windows-msys2-lock.json").read_bytes())
    packages = {item["name"]: item for item in lock["packages"]}
    owners = {}
    for entry in (MSYS / "var/lib/pacman/local").iterdir():
        if not (entry / "files").is_file():
            continue
        info = fields(entry / "desc")
        for name in (entry / "files").read_text().splitlines():
            if name.startswith("mingw64/bin/") and name.endswith(".dll"):
                owners[Path(name).name] = info
    destination.mkdir(parents=True)
    notices = destination / "notices"
    sources = destination / "sources"
    notices.mkdir()
    sources.mkdir()
    result = {"version": 1, "developmentOnly": True, "distributionReady": False, "runtime": {}, "packages": {}, "missing": []}
    vcpkg = TOOLS / "core-build/vcpkg_installed/x64-windows"
    vcpkg_dlls = {"jsoncpp.dll": "jsoncpp", "lz4.dll": "lz4", "libssl-3-x64.dll": "openssl", "libcrypto-3-x64.dll": "openssl"}
    for path in sorted((TOOLS / "engine").glob("*.dll")):
        name = path.name
        if name == "libopenconnect-5.dll":
            verify(path, "a728bd5a1efee8a3a06e5efec07dd13126ddbeb43964cbb4d8c34ec6ba14665c")
            result["runtime"][name] = {"sha256": digest(path), "package": "termterm-openconnect-9.21"}
            continue
        if name in vcpkg_dlls:
            verify(vcpkg / "bin" / name, digest(path))
            result["runtime"][name] = {"sha256": digest(path), "package": "vcpkg/" + vcpkg_dlls[name]}
            continue
        if name not in owners:
            raise ValueError("Unattributed runtime DLL: " + name)
        info = owners[name]
        package = packages[info["NAME"][0]]
        if package["version"] != info["VERSION"][0]:
            raise ValueError("Installed package differs from pinned runtime lock")
        binary = MSYS / "var/cache/pacman/pkg" / package["file"]
        verify(binary, package["sha256"])
        with tarfile.open(binary) as archive:
            data = archive.extractfile("mingw64/bin/" + name).read()
            if hashlib.sha256(data).hexdigest() != digest(path):
                raise ValueError("Staged DLL differs from pinned package: " + name)
            package_name = package["name"]
            if package_name not in result["packages"]:
                copied = []
                for member in archive.getmembers():
                    if member.isfile() and (member.name.startswith("mingw64/share/licenses/") or (member.name.startswith("mingw64/share/doc/") and re.search(r"(?i)(copying|copyright|licen[cs]e|notice)", member.name))):
                        target = notices / package_name / member.name
                        if not target.resolve().is_relative_to(notices.resolve()):
                            raise ValueError("Unsafe notice archive path")
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(archive.extractfile(member).read())
                        copied.append(target.relative_to(destination).as_posix())
                record = {"base": info["BASE"][0], "version": package["version"], "binaryUrl": package["url"], "binarySha256": package["sha256"], "licenses": package["licenses"], "notices": copied}
                if fetch_sources:
                    record["source"] = signed_source(record, binary, sources)
                    if not copied:
                        copied = [path.relative_to(destination).as_posix() for path in source_notices(sources / record["source"]["file"], notices / package_name)]
                        record["notices"] = copied
                else:
                    result["missing"].append(package_name + ": signed matching source not collected")
                if not copied:
                    result["missing"].append(package_name + ": package contains no license notice")
                result["packages"][package_name] = record
        result["runtime"][name] = {"sha256": digest(path), "package": package_name}
    windows = json.loads((ROOT / "scripts/vpn-windows-dependencies.json").read_bytes())
    commit = sp.check_output(["git", "-C", str(TOOLS / "vcpkg"), "rev-parse", "HEAD"], text=True).strip()
    if commit != windows["openvpnCore"]["vcpkgCommit"]:
        raise ValueError("Wrong vcpkg source revision")
    for name in ["asio", "fmt", "jsoncpp", "lz4", "openssl", "xxhash"]:
        overlay = TOOLS / "openvpn3/deps/vcpkg-ports" / name
        repository = TOOLS / ("openvpn3" if overlay.is_dir() else "vcpkg")
        port = overlay if overlay.is_dir() else repository / "ports" / name
        expected_commit = windows["openvpnCore"]["commit"] if overlay.is_dir() else commit
        if sp.check_output(["git", "-C", str(repository), "rev-parse", "HEAD"], text=True).strip() != expected_commit:
            raise ValueError("Wrong source repository for build recipe")
        sp.run(["git", "-C", str(repository), "diff", "--exit-code", "HEAD", "--", port.relative_to(repository).as_posix()], check=True, capture_output=True)
        sbom = json.loads((vcpkg / "share" / name / "vcpkg.spdx.json").read_bytes())
        for file in sbom["files"]:
            relative = file["fileName"].removeprefix("./")
            path = port / relative
            if path.is_file():
                checksum = next(value["checksumValue"] for value in file["checksums"] if value["algorithm"] == "SHA256")
                verify(path, checksum)
        recipe = (port / "portfile.cmake").read_text()
        metadata = json.loads((port / "vcpkg.json").read_bytes())
        version = next(metadata[key] for key in ["version", "version-string", "version-semver"] if key in metadata)
        block = re.search(r"vcpkg_from_github\((.*?)\n\)", recipe, re.S)[1]
        repo = re.search(r"REPO\s+(\S+)", block)[1]
        ref = re.search(r'REF\s+"?([^\s"]+)', block)[1].replace("$" + "{VERSION}", version)
        if name == "asio":
            ref = "asio-" + version.replace(".", "-")
        expected = re.search(r"SHA512\s+([0-9a-f]{128})", block)[1]
        archive = TOOLS / "downloads" / (repo.replace("/", "-") + "-" + ref + ".tar.gz")
        verify(archive, expected, "sha512")
        shutil.copyfile(archive, sources / archive.name)
        notice = vcpkg / "share" / name / "copyright"
        target = notices / ("vcpkg-" + name + ".txt")
        shutil.copyfile(notice, target)
        result["packages"]["vcpkg/" + name] = {"version": version, "vcpkgCommit": commit, "source": {"file": archive.name, "url": "https://github.com/" + repo + "/archive/" + ref + ".tar.gz", "sha512": expected}, "notices": [target.relative_to(destination).as_posix()]}
        for download_block in re.findall(r"vcpkg_download_distfile\((.*?)\n\)", recipe, re.S):
            filename = re.search(r"FILENAME\s+(\S+)", download_block)[1]
            expected_patch = re.search(r"SHA512\s+([0-9a-f]{128})", download_block)[1]
            patch = TOOLS / "downloads" / filename
            verify(patch, expected_patch, "sha512")
            shutil.copyfile(patch, sources / filename)
    openconnect = windows["openconnect"]
    archive = TOOLS / "openconnect-9.21.tar.gz"
    verify(archive, openconnect["sourceSha256"])
    shutil.copyfile(archive, sources / archive.name)
    for key in ["patch", "compatibilityPatch", "ssoPatch"]:
        patch = ROOT / openconnect[key]
        verify(patch, openconnect[key + "Sha256"])
        shutil.copyfile(patch, sources / patch.name)
    notice = ROOT / "docs/licenses/vpn/OpenConnect-LGPL-2.1.txt"
    shutil.copyfile(notice, notices / notice.name)
    result["packages"]["termterm-openconnect-9.21"] = {"version": "9.21", "source": {"url": openconnect["sourceUrl"], "sha256": openconnect["sourceSha256"]}, "patches": {key: openconnect[key + "Sha256"] for key in ["patch", "compatibilityPatch", "ssoPatch"]}}
    core = json.loads((ROOT / "vpn-engine/dependencies.json").read_bytes())["openvpnCore"]
    core_archive = sources / "openvpn3-source.tar"
    archive_source(TOOLS / "openvpn3", core_archive)
    verify(core_archive, core["gitArchiveSha256"])
    result["packages"]["openvpnCore"] = {"commit": core["commit"], "source": {"file": core_archive.name, "sha256": core["gitArchiveSha256"]}}
    for name in ["MPL-2.0.txt", "OpenVPN-Core-NOTICE.md"]:
        shutil.copyfile(ROOT / "docs/licenses/vpn" / name, notices / name)
    vcpkg_archive = sources / "vcpkg-build-source.tar"
    archive_source(TOOLS / "vcpkg", vcpkg_archive)
    result["buildToolSource"] = {"vcpkgCommit": commit, "file": vcpkg_archive.name, "sha256": digest(vcpkg_archive)}
    wireguard = json.loads((ROOT / "vpn-wireguard/dependencies.json").read_bytes())
    go = json.loads(sp.check_output(["go", "env", "-json", "GOVERSION", "GOMODCACHE"], cwd=ROOT / "vpn-wireguard", text=True))
    if go["GOVERSION"] != wireguard["goVersion"]:
        raise ValueError("Use the existing pinned Go toolchain")
    for module in wireguard["modules"]:
        archive = Path(go["GOMODCACHE"]) / "cache/download" / module["module"] / "@v" / (module["version"] + ".zip")
        verify(archive, module["archiveSha256"])
        target = sources / (module["module"].replace("/", "-") + "-" + module["version"] + ".zip")
        shutil.copyfile(archive, target)
        notice = ROOT / "docs/licenses/vpn" / module["notice"]
        verify(notice, module["licenseSha256"])
        shutil.copyfile(notice, notices / notice.name)
        result["packages"][module["module"]] = {"version": module["version"], "source": {"file": target.name, "sha256": module["archiveSha256"], "sum": module["sum"]}}
    notice = ROOT / "docs/licenses/vpn/WireGuard-Go-runtime-LICENSE.txt"
    verify(notice, wireguard["goLicenseSha256"])
    shutil.copyfile(notice, notices / notice.name)
    result["goRuntime"] = {"version": wireguard["goVersion"], "noticeSha256": wireguard["goLicenseSha256"]}
    result["mosh"] = mosh_sources(sources / "cygwin", notices / "cygwin")
    result["applicationSource"] = application_source(sources / "termterm-vpn-source.zip")
    result["acceptanceRequired"] = ["Native installer install/upgrade/uninstall acceptance in a disposable environment", "Platform signing and runtime trust policy; no stable release approval is implied"]
    result["files"] = {path.relative_to(destination).as_posix(): digest(path) for path in sorted(destination.rglob("*")) if path.is_file()}
    (destination / "RUNTIME.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"runtimeDlls": len(result["runtime"]), "packages": len(result["packages"]), "missing": result["missing"], "distributionReady": False}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--fetch-sources", action="store_true")
    parser.add_argument("--source-receipt", action="store_true")
    parser.add_argument("--package-preflight", action="store_true")
    parser.add_argument("--runtime", type=Path)
    args = parser.parse_args()
    if args.source_receipt:
        print(json.dumps(source_receipt(), separators=(",", ":")))
    elif args.package_preflight:
        if not args.runtime:
            parser.error("--package-preflight needs --runtime")
        print(json.dumps(package_preflight(args.runtime, args.output), separators=(",", ":")))
    elif args.output:
        collect(args.output, args.fetch_sources)
    else:
        parser.error("Provide --output, --source-receipt or --package-preflight")
