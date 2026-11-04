#!/usr/bin/env python3
"""Prepare pinned Windows VPN dependencies locally; never install a driver or connect."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess as sp
import tarfile
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
LOCK = json.loads((ROOT / "scripts/vpn-windows-dependencies.json").read_bytes())
BUILD = ROOT / ".tools/vpn-windows"


def run(*args, **kwargs):
    return sp.run(list(map(str, args)), check=True, **kwargs)


def verify(path, digest):
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise RuntimeError(f"Dependency hash mismatch: {path.name}")


def download(url, path, digest):
    if not path.exists():
        with urllib.request.urlopen(url, timeout=90) as source:
            path.write_bytes(source.read())
    verify(path, digest)


def unzip(path, destination):
    with zipfile.ZipFile(path) as archive:
        for name in archive.namelist():
            if not (destination / name).resolve().is_relative_to(destination.resolve()):
                raise RuntimeError("Archive entry escapes dependency directory")
        archive.extractall(destination)


def source(path, url, commit):
    if not path.exists():
        run("git", "init", path)
        run("git", "-C", path, "config", "core.autocrlf", "false")
        run("git", "-C", path, "remote", "add", "origin", url)
        run("git", "-C", path, "fetch", "--depth", "1", "origin", commit)
        run("git", "-C", path, "checkout", "--detach", "FETCH_HEAD")
    actual = run("git", "-C", path, "rev-parse", "HEAD", capture_output=True, text=True).stdout.strip()
    changed = sp.run(["git", "-C", str(path), "diff", "--quiet", "HEAD", "--"]).returncode
    untracked = run("git", "-C", path, "ls-files", "--others", "--exclude-standard", capture_output=True, text=True).stdout
    if actual != commit or changed or untracked:
        raise RuntimeError(f"Expected clean pinned dependency: {path.name}")


def visual_studio():
    locator = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"
    instances = json.loads(run(locator, "-utf8", "-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-format", "json", capture_output=True).stdout)
    if not instances:
        raise RuntimeError("Existing Visual Studio 2022 C++ Build Tools required; this script does not install them")
    return Path(instances[0]["installationPath"])


def prepare():
    BUILD.mkdir(parents=True, exist_ok=True)
    oc, wintun = LOCK["openconnect"], LOCK["wintun"]
    package = BUILD / "openconnect-9.21-official-artifacts.zip"
    download(oc["url"], package, oc["archiveSha256"])
    unzip(package, BUILD / "openconnect-official")
    verify(BUILD / "openconnect-official" / oc["installer"], oc["installerSha256"])
    src = BUILD / "openconnect-9.21.tar.gz"
    download(oc["sourceUrl"], src, oc["sourceSha256"])
    with tarfile.open(src) as archive:
        for name in ["openconnect.h", "COPYING.LGPL"]:
            (BUILD / name).write_bytes(archive.extractfile("openconnect-9.21/" + name).read())
    verify(BUILD / "COPYING.LGPL", oc["licenseSha256"])
    driver = BUILD / "wintun-0.14.1.zip"
    download(wintun["url"], driver, wintun["archiveSha256"])
    unzip(driver, BUILD / "wintun")
    verify(BUILD / "wintun/wintun/bin/amd64/wintun.dll", wintun["dllSha256"])
    verify(BUILD / "wintun/wintun/LICENSE.txt", wintun["licenseSha256"])
    dll = str(BUILD / "wintun/wintun/bin/amd64/wintun.dll").replace("'", "''")
    command = "Get-AuthenticodeSignature -LiteralPath '" + dll + "' | Select-Object @{n='status';e={$_.Status.ToString()}},@{n='signer';e={$_.SignerCertificate.Subject}} | ConvertTo-Json -Compress"
    # A PowerShell 7 parent's module path cannot be reused by Windows PowerShell 5.
    signature = json.loads(run("powershell.exe", "-NoProfile", "-NonInteractive", "-EncodedCommand", base64.b64encode(command.encode("utf-16le")).decode(), capture_output=True, env={key: value for key, value in os.environ.items() if key.upper() != "PSMODULEPATH"}).stdout)
    if signature["status"] != "Valid" or "WireGuard LLC" not in signature["signer"]:
        raise RuntimeError("Wintun publisher signature is not valid")
    print("Pinned official archives and source licenses verified; no installer executed.")


def core_build():
    spec = LOCK["openvpnCore"]
    source(BUILD / "openvpn3", spec["source"], spec["commit"])
    source(BUILD / "vcpkg", "https://github.com/microsoft/vcpkg.git", spec["vcpkgCommit"])
    core_lock = json.loads((ROOT / "vpn-engine/dependencies.json").read_bytes())["openvpnCore"]
    with sp.Popen(["git", "-C", str(BUILD / "openvpn3"), "archive", "HEAD"], stdout=sp.PIPE) as process:
        digest = hashlib.file_digest(process.stdout, "sha256").hexdigest()
        if process.wait() or digest != core_lock["gitArchiveSha256"]:
            raise RuntimeError("OpenVPN Core archive differs from approved source")
    license_bytes = run("git", "-C", BUILD / "openvpn3", "show", "HEAD:LICENSE.md", capture_output=True).stdout
    if hashlib.sha256(license_bytes).hexdigest() != core_lock["licenseSha256"]:
        raise RuntimeError("OpenVPN Core license differs from approved source")
    vs = visual_studio()
    cmake = vs / "Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe"
    run("cmd", "/c", "bootstrap-vcpkg.bat", "-disableMetrics", cwd=BUILD / "vcpkg")
    (BUILD / "binary-cache").mkdir(exist_ok=True)
    env = {**os.environ, "VCPKG_ROOT": str(BUILD / "vcpkg"), "VCPKG_DOWNLOADS": str(BUILD / "downloads"), "VCPKG_MAX_CONCURRENCY": "2", "VCPKG_BINARY_SOURCES": "clear;files," + str(BUILD / "binary-cache") + ",readwrite", "VCPKG_DISABLE_METRICS": "1"}
    run(cmake, "--preset", spec["preset"], "-S", BUILD / "openvpn3", "-B", BUILD / "core-build", "-DCLI_OVPNDCOWIN=OFF", "-DCLI_NULLTUN=ON", "-DBUILD_TESTING=OFF", env=env)
    run(cmake, "--build", BUILD / "core-build", "--config", "Release", "--target", "ovpnclinull", "--parallel", "2", env=env)
    print("Core null-TUN executable built. No VPN connection tested.")


def probe_openconnect():
    sevenzip = BUILD / "downloads/tools/7zip-26.03-windows/7z.exe"
    if not sevenzip.is_file():
        raise RuntimeError("Pinned vcpkg 7-Zip extractor not found; do not execute the OpenConnect installer")
    run(sevenzip, "x", BUILD / "openconnect-official" / LOCK["openconnect"]["installer"], "-o" + str(BUILD / "openconnect-9.21-x64"), "-y")
    verify(BUILD / "openconnect-9.21-x64/libopenconnect-5.dll", LOCK["openconnect"]["dllSha256"])
    probe = BUILD / "openconnect-abi-probe.cpp"
    probe.write_bytes(b'''#include <windows.h>
#include <cstdio>
// MSVC does not implement the header's GNU printf-format diagnostic annotation.
#define __attribute__(x)
#include "openconnect.h"
#undef __attribute__
int wmain(int argc, wchar_t** argv) {
    if (argc != 2) return 2;
    HMODULE library = LoadLibraryExW(argv[1], nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    if (!library) { std::printf("load failed: %lu\\n", GetLastError()); return 3; }
    auto version = reinterpret_cast<decltype(&openconnect_get_version)>(GetProcAddress(library, "openconnect_get_version"));
    if (!version) return 4;
    std::printf("MSVC header + Windows DLL C ABI: %s\\n", version());
    FreeLibrary(library);
    return 0;
}
''')
    vs = visual_studio()
    batch = BUILD / "openconnect-abi-probe.cmd"
    batch.write_bytes(('''@echo off
call "''' + str(vs / "VC/Auxiliary/Build/vcvars64.bat") + '''" >nul
if errorlevel 1 exit /b %errorlevel%
cl /nologo /EHsc openconnect-abi-probe.cpp /Fe:openconnect-abi-probe.exe
if errorlevel 1 exit /b %errorlevel%
openconnect-abi-probe.exe "%CD%\\openconnect-9.21-x64\\libopenconnect-5.dll"
''').replace("\n", "\r\n").encode())
    run("cmd", "/c", batch.name, cwd=BUILD)
    print("Official OpenConnect DLL loaded from MSVC; no adapter created or VPN connection tested.")


def self_test():
    assert LOCK["distributionReady"] is False
    assert LOCK["openvpnCore"]["commit"] == json.loads((ROOT / "vpn-engine/dependencies.json").read_bytes())["openvpnCore"]["commit"]
    for spec in [LOCK["openconnect"], LOCK["wintun"]]:
        assert spec["url"].startswith("https://")
        for key, value in spec.items():
            if key.endswith("Sha256"):
                assert len(value) == 64 and all(c in "0123456789abcdef" for c in value)
    assert LOCK["openconnect"]["job"] == 14877945032
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        archive = directory / "unsafe.zip"
        with zipfile.ZipFile(archive, "w") as output:
            output.writestr("../escape", "must not extract")
        try:
            unzip(archive, directory / "output")
        except RuntimeError:
            pass
        else:
            raise AssertionError("Archive traversal was accepted")
        assert not (directory / "escape").exists()
        try:
            verify(archive, "0" * 64)
        except RuntimeError:
            pass
        else:
            raise AssertionError("Wrong dependency hash was accepted")
    print("Windows dependency lock self-check passed.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--build-core", action="store_true", help="Build upstream null-TUN Core probe with existing VS2022; never run it against a server")
    parser.add_argument("--probe-openconnect", action="store_true", help="Extract official installer with vcpkg's local 7-Zip and run version-only MSVC C ABI probe")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        if os.name != "nt":
            parser.error("Run this preparation on Windows; it does not install global toolchains")
        prepare()
        if args.build_core:
            core_build()
        if args.probe_openconnect:
            probe_openconnect()
