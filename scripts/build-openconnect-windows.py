#!/usr/bin/env python3
"""Build the pinned OpenConnect packet-pipe patch in a local portable MSYS2 root."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shlex
import tarfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("vpn_windows_prepare", ROOT / "scripts/prepare-vpn-windows.py")
prep = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prep)
LOCK = json.loads((ROOT / "scripts/vpn-windows-msys2-lock.json").read_bytes())
BUILD = prep.BUILD
MSYS = BUILD / "msys2-portable/msys64"


def bash(command):
    return prep.run(MSYS / "usr/bin/bash.exe", "--login", "-c", command, env={**os.environ, "MSYSTEM": "MINGW64"})


def posix(path):
    path = path.resolve()
    return "/" + path.drive[0].lower() + path.as_posix()[2:]


def prepare_toolchain():
    BUILD.mkdir(exist_ok=True)
    archive = BUILD / "msys2-base-x86_64-20260927.tar.xz"
    prep.download(LOCK["base"]["url"], archive, LOCK["base"]["sha256"])
    if not (MSYS / "usr/bin/bash.exe").exists():
        sevenzip = BUILD / "downloads/tools/7zip-26.03-windows/7z.exe"
        if not sevenzip.exists():
            raise RuntimeError("Run Windows Core preparation first to acquire pinned local 7-Zip")
        output = BUILD / "msys2-portable"
        prep.run(sevenzip, "x", archive, "-o" + str(output), "-y")
        prep.run(sevenzip, "x", output / "msys2-base-x86_64-20260927.tar", "-o" + str(output), "-y")
    cache = MSYS / "var/cache/pacman/pkg"
    cache.mkdir(parents=True, exist_ok=True)
    for package in LOCK["packages"]:
        prep.download(package["url"], cache / package["file"], package["sha256"])
        prep.download(package["url"] + ".sig", cache / (package["file"] + ".sig"), package["signatureSha256"])
    files = " ".join(shlex.quote(package["file"]) for package in LOCK["packages"])
    bash("cd /var/cache/pacman/pkg && pacman -U --needed --noconfirm -- " + files)


def build():
    oc = prep.LOCK["openconnect"]
    source = BUILD / "openconnect-source/openconnect-9.21"
    patches = [ROOT / oc[key] for key in ["patch", "compatibilityPatch", "ssoPatch"]]
    for key, patch in zip(["patch", "compatibilityPatch", "ssoPatch"], patches):
        prep.verify(patch, oc[key + "Sha256"])
    archive = BUILD / "openconnect-9.21.tar.gz"
    prep.download(oc["sourceUrl"], archive, oc["sourceSha256"])
    with tarfile.open(archive) as tar:
        tar.extractall(source.parent, filter="data")
    for patch in patches:
        command = "cd " + shlex.quote(posix(source)) + " && patch -p1 --fuzz=0 "
        bash(command + "--dry-run < " + shlex.quote(posix(patch)))
        bash(command + "< " + shlex.quote(posix(patch)))
    output = BUILD / "openconnect-build"
    output.mkdir(exist_ok=True)
    configure = shlex.quote(posix(source / "configure"))
    options = "--disable-nls --without-java --without-libproxy --without-stoken --without-lz4 --without-pskc --without-pcsclite --without-gnutls-version-check --disable-static --enable-shared --with-vpnc-script=/termterm/no-script"
    bash("cd " + shlex.quote(posix(output)) + " && " + configure + " " + options + " && make -j2 libopenconnect.la")
    dll = output / ".libs/libopenconnect-5.dll"
    print("Patched DLL SHA256:", hashlib.sha256(dll.read_bytes()).hexdigest())
    vs = prep.visual_studio()
    batch = BUILD / "test-packet-api.cmd"
    batch.write_bytes(('''@echo off
call "''' + str(vs / "VC/Auxiliary/Build/vcvars64.bat") + '''" >nul
if errorlevel 1 exit /b %errorlevel%
cl /nologo /EHsc "''' + str(ROOT / "scripts/test-openconnect-packet-handle.cpp") + '''" /I"''' + str(source) + '''" /Fe:test-packet-api.exe
if errorlevel 1 exit /b %errorlevel%
test-packet-api.exe "''' + str(dll) + '''" "''' + str(MSYS / "mingw64/bin") + '''"
''').replace("\n", "\r\n").encode())
    prep.run("cmd", "/c", batch.name, cwd=BUILD)


def self_test():
    assert LOCK["environment"] == "MINGW64"
    assert len({package["file"] for package in LOCK["packages"]}) == len(LOCK["packages"])
    for package in LOCK["packages"]:
        assert package["url"].startswith("https://repo.msys2.org/")
        assert package["file"] == package["url"].split("/")[-1]
        assert len(package["sha256"]) == len(package["signatureSha256"]) == 64
    for key in ["patch", "compatibilityPatch", "ssoPatch"]:
        prep.verify(ROOT / prep.LOCK["openconnect"][key], prep.LOCK["openconnect"][key + "Sha256"])
    print("MSYS2 package lock and OpenConnect patch hashes verified.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--prepare-only", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        if os.name != "nt":
            parser.error("This build uses portable native Windows MSYS2, not a global installation")
        prepare_toolchain()
        if not args.prepare_only:
            build()
