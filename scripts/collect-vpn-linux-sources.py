#!/usr/bin/env python3
"""Collect exact Ubuntu sources for the frozen Linux payload; never install or execute it."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from email.parser import Parser
import hashlib
import json
import lzma
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
REPOSITORIES = {
    "jammy": "https://archive.ubuntu.com/ubuntu",
    "jammy-updates": "https://archive.ubuntu.com/ubuntu",
    "jammy-security": "https://security.ubuntu.com/ubuntu",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def control(text):
    if text.startswith("-----BEGIN PGP SIGNED MESSAGE-----"):
        text = text.split("\n\n", 1)[1].split("\n-----BEGIN PGP SIGNATURE-----", 1)[0]
        text = "\n".join(line[2:] if line.startswith("- ") else line for line in text.splitlines())
    result = Parser().parsestr(text + "\n\n")
    require(not result.defects and len(result.keys()) == len(set(key.lower() for key in result.keys())), "Malformed or duplicate Debian control fields")
    return result


def checksums(value):
    result = {}
    for line in value.splitlines():
        if not line.strip():
            continue
        parts = line.split()
        require(len(parts) == 3 and re.fullmatch(r"[0-9a-f]{64}", parts[0]) and parts[1].isdigit(), "Invalid SHA256 source record")
        name = PurePosixPath(parts[2])
        require(not name.is_absolute() and ".." not in name.parts and "\\" not in parts[2] and ":" not in parts[2], "Unsafe archive path")
        require(parts[2] not in result, "Duplicate archive checksum")
        result[parts[2]] = {"sha256": parts[0], "bytes": int(parts[1])}
    require(result, "Empty SHA256 source record")
    return result


def verify(path, expected):
    require(not path.is_symlink() and path.is_file() and path.stat().st_size == expected["bytes"] and digest(path) == expected["sha256"], "Source archive integrity mismatch: " + path.name)


def download(job):
    path, record = job
    if path.exists():
        verify(path, record)
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    partial = path.with_name(path.name + ".part")
    require(not partial.is_symlink(), "Partial download cannot be a link")
    request = urllib.request.Request(record["url"], headers={"User-Agent": "TermTerm-source-verification/1"})
    with urllib.request.urlopen(request, timeout=90) as response, partial.open("wb") as stream:
        require(response.geturl().startswith("https://"), "Source redirect downgraded HTTPS")
        shutil.copyfileobj(response, stream)
    verify(partial, record)
    partial.rename(path)


def collect(delivery, metadata, output):
    delivery = delivery.absolute()
    require(delivery.resolve() == delivery and delivery.name == "linux" and re.fullmatch(r"vpn-delivery-[0-9]{8}", delivery.parent.name)
            and delivery.parent.parent == ROOT / "artifacts", "Use this worktree's frozen Linux delivery")
    output = output.absolute()
    require(output.resolve() == output and output.parent == delivery and output.name == "source-closure-20261103", "Use the separate Linux source-closure directory")
    original = {path.name: {"sha256": digest(path), "bytes": path.stat().st_size} for path in delivery.iterdir() if path.is_file()}
    app = json.loads((delivery / "APPIMAGE-PROVENANCE.json").read_text())
    helper = json.loads((delivery / "PACKAGE-RECEIPT.json").read_text())
    wanted = {(entry["sourcePackage"], entry["sourceVersion"]) for entry in [*app["systemPackages"].values(), *helper["systemDependencyPackages"].values()]}
    require(len(app["elfFiles"]) == 179 and len(app["systemPackages"]) == 116 and len(wanted) == 94, "Frozen Linux source inventory changed")
    output.mkdir(exist_ok=True)
    before = output / "ORIGINAL-ARTIFACTS.json"
    if before.exists():
        require(json.loads(before.read_text()) == original, "Original delivery changed during source collection")
    else:
        before.write_text(json.dumps(original, indent=2) + "\n")
    evidence = output / "metadata"
    evidence.mkdir(exist_ok=True)
    keyring = Path("/usr/share/keyrings/ubuntu-archive-keyring.gpg")
    shutil.copyfile(keyring, evidence / keyring.name)
    index_receipts, found = [], {}
    for suite, base in REPOSITORIES.items():
        release = metadata / (suite + "-InRelease")
        checked = subprocess.run(["gpgv", "--status-fd", "1", "--keyring", str(keyring), str(release)], check=True, capture_output=True, text=True)
        require("[GNUPG:] VALIDSIG " in checked.stdout, "Ubuntu archive signature missing")
        release_fields = control(release.read_text())
        expected = checksums(release_fields["SHA256"])
        shutil.copyfile(release, evidence / release.name)
        for component in ["main", "universe"]:
            relative = component + "/source/Sources.xz"
            archive = metadata / (suite + "-" + component + "-Sources.xz")
            verify(archive, expected[relative])
            shutil.copyfile(archive, evidence / archive.name)
            index_receipts.append({"file": archive.name, **expected[relative], "url": base + "/dists/" + suite + "/" + relative,
                                   "inRelease": release.name, "inReleaseSha256": digest(release), "gpgStatus": checked.stdout.splitlines()})
            for paragraph in lzma.decompress(archive.read_bytes()).decode().split("\n\n"):
                fields = control(paragraph)
                pair = (fields.get("Package"), fields.get("Version"))
                if pair not in wanted:
                    continue
                directory = fields["Directory"]
                require(directory.startswith("pool/") and ".." not in PurePosixPath(directory).parts, "Unsafe Ubuntu source directory")
                files = checksums(fields["Checksums-Sha256"])
                require(all("/" not in name for name in files), "Source archive name must be a basename")
                record = {"package": pair[0], "version": pair[1], "directory": directory, "index": archive.name, "files": files, "base": base}
                if pair in found:
                    require(found[pair]["files"] == files and found[pair]["directory"] == directory, "Signed indexes disagree about the exact source version")
                else:
                    found[pair] = record
    require(set(found) == wanted, "Exact source versions absent from signed indexes: " + repr(sorted(wanted - set(found))))
    assets = {}
    for pair, record in sorted(found.items()):
        for name, value in record["files"].items():
            relative = "ubuntu/" + pair[0] + "/" + name
            asset = {**value, "url": record["base"] + "/" + record["directory"] + "/" + name}
            if relative in assets:
                require(assets[relative]["sha256"] == asset["sha256"], "Conflicting shared source archive")
            else:
                assets[relative] = asset
    total = sum(value["bytes"] for value in assets.values())
    require(shutil.disk_usage(output).free > total * 2 + 64 * 1024 * 1024, "Insufficient source download space")
    manifest = {"sourceVersionPairs": [found[pair] for pair in sorted(found)], "indexes": index_receipts, "files": assets,
                "ubuntuKeyringSha256": digest(keyring), "authentication": "Ubuntu archive InRelease signature -> Sources SHA256 -> .dsc and source archive SHA256",
                "individualDscMaintainerSignaturesVerified": False, "nativeTestsPerformed": False, "originalPackagesModified": False}
    (output / "DOWNLOAD-INPUTS.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Downloading/verifying {len(assets)} files ({total:,} bytes) for {len(found)} exact source/version pairs", flush=True)
    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(download, [(output / name, value) for name, value in assets.items()]))
    for pair, record in found.items():
        descriptors = [name for name in record["files"] if name.endswith(".dsc")]
        require(len(descriptors) == 1, "Expected exactly one source descriptor")
        descriptor = control((output / "ubuntu" / pair[0] / descriptors[0]).read_text())
        require(descriptor["Source"] == pair[0] and descriptor["Version"] == pair[1], "Source descriptor identity mismatch")
        listed = checksums(descriptor["Checksums-Sha256"])
        require(set(listed) == set(record["files"]) - set(descriptors), "Source descriptor has a different archive set")
        require(all(value == record["files"][name] for name, value in listed.items()), "Source descriptor disagrees with signed Sources hashes")
    require({path.name: {"sha256": digest(path), "bytes": path.stat().st_size} for path in delivery.iterdir() if path.is_file()} == original, "Original delivery changed")
    manifest["allUbuntuSourcesVerified"] = True
    manifest["files"] = {name: {**value, "verified": True} for name, value in assets.items()}
    (output / "UBUNTU-SOURCES.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print("All 94 Ubuntu source/version pairs verified; original delivery unchanged", flush=True)


def verify_closure(output, oci_layer=None):
    output = output.absolute()
    require(output.resolve() == output, "Closure path cannot contain links")
    manifest = json.loads((output / "SOURCE-CLOSURE.json").read_text())
    require(manifest["version"] == 1, "Unsupported source closure manifest")
    require(not any(path.is_symlink() for path in output.rglob("*")), "Source closure cannot contain links")
    actual = {path.relative_to(output).as_posix() for path in output.rglob("*") if path.is_file()}
    require(actual == set(manifest["files"]) | {"SOURCE-CLOSURE.json", "SHA256SUMS.txt"}, "Unexpected source closure file set")
    for name, record in manifest["files"].items():
        path = PurePosixPath(name)
        require(not path.is_absolute() and ".." not in path.parts and "\\" not in name and ":" not in name, "Unsafe closure file path")
        verify(output / name, record)
    for name, record in manifest["originalArtifacts"].items():
        require(PurePosixPath(name).name == name and "\\" not in name and ":" not in name, "Invalid original artifact name")
        verify(output.parent / name, record)
    ubuntu = json.loads((output / "UBUNTU-SOURCES.json").read_text())
    require(ubuntu["allUbuntuSourcesVerified"] and len(ubuntu["sourceVersionPairs"]) == 94 and len(ubuntu["files"]) == 301, "Incomplete Ubuntu closure")
    for index in ubuntu["indexes"]:
        subprocess.run(["gpgv", "--keyring", str(output / "metadata/ubuntu-archive-keyring.gpg"), str(output / "metadata" / index["inRelease"])], check=True, capture_output=True)
        expected = checksums(control((output / "metadata" / index["inRelease"]).read_text())["SHA256"])
        relative = index["file"].split("-Sources.xz")[0].rsplit("-", 1)[1] + "/source/Sources.xz"
        verify(output / "metadata" / index["file"], expected[relative])
    runtime = manifest["runtime"]
    published = json.loads((output / runtime["officialReleaseMetadata"]).read_text())
    asset = next(entry for entry in published["assets"] if entry["name"] == "runtime-x86_64")
    require(published["target_commitish"] == runtime["commit"] and asset["digest"] == "sha256:" + runtime["officialRuntimeSha256"], "Runtime source/release identity changed")
    for signature in runtime["alpineSignatures"]:
        subprocess.run(["openssl", "dgst", "-sha1", "-verify", str(output / signature["key"]), "-signature", str(output / signature["signature"]), str(output / signature["signed"])], check=True, capture_output=True)
    for port in runtime["alpinePorts"]:
        directory = output / port["directory"]
        for entry in json.loads((directory / "TREE.json").read_text()):
            require(entry["type"] == "file" and PurePosixPath(entry["name"]).name == entry["name"], "Unexpected aports tree member")
            data = (directory / entry["name"]).read_bytes()
            require(hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest() == entry["sha"], "Aports Git blob changed")
        recipe = (directory / "APKBUILD").read_text()
        version = re.search(r"^pkgver=(.+)$", recipe, re.M)[1]
        revision = re.search(r"^pkgrel=(.+)$", recipe, re.M)[1]
        require(version + "-r" + revision == port["version"], "Aports source revision changed")
        for expected, name in re.findall(r"^([0-9a-f]{128})  (.+)$", recipe, re.M):
            require(PurePosixPath(name).name == name and "\\" not in name and ":" not in name, "Invalid aports source filename")
            path = directory / name if (directory / name).exists() else output / "runtime/upstream" / name
            require(hashlib.sha512(path.read_bytes()).hexdigest() == expected, "Aports source or patch checksum mismatch")
    image = output.parent / runtime["appImage"]
    apprun = subprocess.check_output(["unsquashfs", "-o", str(runtime["prefixBytes"]), "-cat", str(image), "AppRun.wrapped"])
    require(hashlib.sha256(apprun).hexdigest() == runtime["appRunSha256"], "AppRun payload differs from the proven upstream asset")
    prefix = image.read_bytes()[:runtime["prefixBytes"]]
    offset, size = runtime["digestMd5Section"]
    require(size == 16 and offset + size <= len(prefix), "Invalid runtime metadata range")
    sections = subprocess.check_output(["readelf", "--wide", "--sections", str(image)], text=True)
    section = re.findall(r"\]\s+\.digest_md5\s+\S+\s+\S+\s+([0-9a-f]+)\s+([0-9a-f]+)", sections)
    require([(int(start, 16), int(length, 16)) for start, length in section] == [(offset, size)], "Normalization must cover only the actual ELF digest metadata section")
    normalized = prefix[:offset] + bytes(size) + prefix[offset + size:]
    require(hashlib.sha256(normalized).hexdigest() == runtime["officialRuntimeSha256"], "Embedded runtime code differs from official release")
    if oci_layer:
        require(digest(oci_layer) == runtime["ociLayerSha256"], "CI-pinned OCI layer changed")
        with tarfile.open(oci_layer) as archive:
            data = archive.extractfile("lib/apk/db/installed").read()
        require(data == (output / "runtime/evidence/official-base-installed.txt").read_bytes(), "OCI musl origin evidence changed")
    print("Source closure files, signed Ubuntu/Alpine metadata, exact runtime body and original artifacts verified; no application execution")


def replay(lock_path, output):
    output = output.absolute()
    lock = json.loads(lock_path.read_text())
    require(lock["version"] == 1 and output.resolve() == output and output.name == "source-closure-20261103"
            and output.parent.name == "linux" and output.parent.parent.parent == ROOT / "artifacts", "Invalid source replay destination")
    for name, record in lock["files"].items():
        path = PurePosixPath(name)
        require(not path.is_absolute() and ".." not in path.parts and "\\" not in name and ":" not in name, "Unsafe replay file path")
        destination = output / name
        require(destination.resolve() == destination, "Replay path cannot contain links")
        if "url" in record:
            download((destination, record))
        else:
            verify(destination, record)
    print("Exact source lock replayed; no latest-version substitutions")


def self_test():
    value = control("Source: fixture\nVersion: 1:2-3\nChecksums-Sha256:\n " + "a" * 64 + " 3 fixture.tar.gz\n")
    assert value["Version"] == "1:2-3" and checksums(value["Checksums-Sha256"])["fixture.tar.gz"]["bytes"] == 3
    for text in ["Source: one\nSource: two\n", "broken header\n"]:
        try:
            control(text)
        except ValueError:
            pass
        else:
            raise AssertionError("Malformed control fields accepted")
    for name in ["../secret", "/absolute", "C:secret", "dir\\secret"]:
        try:
            checksums("a" * 64 + " 1 " + name)
        except ValueError:
            pass
        else:
            raise AssertionError("Unsafe source archive path accepted")
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "archive"
        path.write_bytes(b"changed")
        try:
            verify(path, {"sha256": "0" * 64, "bytes": 7})
        except ValueError:
            pass
        else:
            raise AssertionError("Changed source archive accepted")
        lock = Path(directory) / "lock.json"
        lock.write_text(json.dumps({"version": 1, "files": {"../secret": {"sha256": "0" * 64, "bytes": 1}}}))
        try:
            replay(lock, ROOT / "artifacts/vpn-delivery-20261103/linux/source-closure-20261103")
        except ValueError as error:
            assert "Unsafe replay file path" in str(error)
        else:
            raise AssertionError("Replay escaped its source directory")
    print("Inert Debian header, path and archive-integrity checks passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--delivery", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--verify-closure", type=Path)
    parser.add_argument("--oci-layer", type=Path, help="Optional cached CI-pinned base layer for independent musl origin verification")
    parser.add_argument("--replay-lock", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    elif args.verify_closure:
        verify_closure(args.verify_closure, args.oci_layer)
    elif args.replay_lock and args.output:
        replay(args.replay_lock, args.output)
    elif args.delivery and args.metadata and args.output:
        collect(args.delivery, args.metadata, args.output)
    else:
        parser.error("Provide --delivery, --metadata and --output, or --self-test")
