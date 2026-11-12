#!/usr/bin/env python3
"""Prepare or install the fixed, root-owned 0.4.5 AppDir; never start services or publish."""
import argparse
from contextlib import contextmanager
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import pwd
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import uuid

APPDIR = Path("/opt/termterm-appimage")
CLIENT = "usr/bin/termterm"
MARKER = "TERMTERM-INSTALLATION.json"
CONFIG = Path("/etc/termterm-vpn/broker.json")
LAUNCHER = Path("/usr/bin/termterm-appimage")
ENTRY = Path("/usr/share/applications/termterm-appimage.desktop")
PROCESSES = Path("/proc")
LAUNCH = b'#!/bin/sh\nunset APPIMAGE APPIMAGE_EXTRACT_AND_RUN\nexport APPDIR=/opt/termterm-appimage\nexec /opt/termterm-appimage/AppRun "$@"\n'
DESKTOP = b"[Desktop Entry]\nType=Application\nName=TermTerm\nExec=/usr/bin/termterm-appimage\nIcon=/opt/termterm-appimage/.DirIcon\nCategories=Development;\nTerminal=false\n"


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            value.update(block)
    return value.hexdigest()


def checked_file(path, expected):
    path = Path(path).absolute()
    require(path == path.resolve() and path.is_file() and not path.is_symlink(), "regular_canonical_input_required")
    require(re.fullmatch(r"[0-9a-f]{64}", expected or "") and digest(path) == expected, "reviewed_input_sha256_mismatch")
    return path


def inventory(root):
    root = root.resolve()
    result, total = {}, 0
    for directory, directories, files in os.walk(root, followlinks=False):
        for name in sorted(directories + files):
            path = Path(directory) / name
            relative = path.relative_to(root).as_posix()
            require(len(relative) <= 1024 and not any(c in relative for c in "\\\r\n\0") and relative != MARKER, "unsafe_appdir_member")
            metadata = path.lstat()
            require(not metadata.st_mode & 0o6000 and len(result) < 16384, "unsafe_appdir_mode_or_inventory_size")
            if stat.S_ISLNK(metadata.st_mode):
                target = os.readlink(path)
                require(not Path(target).is_absolute() and not any(c in target for c in "\\\r\n\0")
                        and path.resolve().is_relative_to(root) and path.resolve().exists(), "appdir_link_escapes_or_is_dangling")
                result[relative] = {"kind": "symlink", "target": target}
            elif stat.S_ISDIR(metadata.st_mode):
                result[relative] = {"kind": "directory"}
            else:
                require(stat.S_ISREG(metadata.st_mode) and metadata.st_size <= 256 * 1024 * 1024, "regular_bounded_appdir_file_required")
                total += metadata.st_size
                require(total <= 2 * 1024 * 1024 * 1024, "appdir_size_exceeded")
                result[relative] = {"kind": "file", "sha256": digest(path), "executable": bool(metadata.st_mode & 0o111)}
    require(CLIENT in result and result[CLIENT]["kind"] == "file" and result[CLIENT]["executable"] and "AppRun" in result, "appdir_client_or_launcher_missing")
    with (root / CLIENT).open("rb") as stream:
        require(stream.read(4) == b"\x7fELF", "appdir_client_must_be_elf")
    return result


@contextmanager
def extracted(image, expected, uid):
    require(type(uid) is int and uid > 0, "nonroot_extraction_uid_required")
    account = pwd.getpwuid(uid)
    image = checked_file(image, expected)
    with tempfile.TemporaryDirectory(prefix="termterm-appimage-") as temporary:
        work = Path(temporary)
        work.chmod(0o755)
        immutable = work / "verified.AppImage"
        shutil.copyfile(image, immutable)
        immutable.chmod(0o555)
        require(digest(immutable) == expected, "appimage_changed_during_copy")
        output = work / "extract"
        output.mkdir(mode=0o700)
        os.chown(output, uid, account.pw_gid)
        # The reviewed AppImage runtime is never executed with root privileges.
        subprocess.run(["/usr/bin/setpriv", "--reuid=" + str(uid), "--regid=" + str(account.pw_gid), "--clear-groups", "--no-new-privs",
                        str(immutable), "--appimage-extract"], cwd=output, env={"PATH": "/usr/bin:/bin", "HOME": str(output), "LC_ALL": "C"},
                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True, timeout=120)
        tree = output / "squashfs-root"
        require(tree.is_dir() and not tree.is_symlink(), "appimage_extraction_missing")
        yield tree


def prepare(image, expected, uid, destination):
    require(not destination.exists() and not destination.is_symlink(), "fresh_manifest_destination_required")
    with extracted(image, expected, uid) as tree:
        files = inventory(tree)
    manifest = {"version": 1, "applicationIdentifier": "local.termterm.desktop", "candidateVersion": "0.4.5", "imageSha256": expected,
                "clientSha256": files[CLIENT]["sha256"], "files": files}
    with destination.open("x", encoding="utf-8") as stream:
        json.dump(manifest, stream, indent=2)
        stream.write("\n")
    return manifest


def replacement(config, expected_old):
    if APPDIR.exists() or APPDIR.is_symlink():
        require(expected_old is not None and not APPDIR.is_symlink(), "existing_appdir_requires_expected_client_sha256")
        marker = APPDIR / MARKER
        require(marker.is_file() and not marker.is_symlink() and marker.stat().st_size <= 16384, "unowned_existing_appdir_rejected")
        installed = json.loads(marker.read_text())
        require(installed.get("candidateVersion") in ("0.4.3", "0.4.4", "0.4.5") and installed.get("clientSha256") == expected_old
                and config.get("clientInstall") == "appimage" and config["clientSha256"] == expected_old
                and digest(APPDIR / CLIENT) == expected_old, "existing_appdir_pin_or_version_changed")
        require(LAUNCHER.read_bytes() == LAUNCH and ENTRY.read_bytes() == DESKTOP, "installed_launcher_changed")
    else:
        require(expected_old is None and not LAUNCHER.exists() and not LAUNCHER.is_symlink() and not ENTRY.exists() and not ENTRY.is_symlink(), "foreign_launcher_or_upgrade_state_rejected")


def install(image, manifest_path, manifest_sha256, uid, expected_old=None):
    spec = importlib.util.spec_from_file_location("native", Path(__file__).with_name("vpn-unix-broker.py"))
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    native.trusted(Path(__file__).resolve())
    native.trusted(CONFIG)
    config = json.loads(CONFIG.read_text())
    native.validate_configuration(config, "0.4.5")
    require(type(uid) is int and uid > 0 and config["uid"] == uid, "configured_nonroot_uid_required")
    pwd.getpwuid(uid)
    require(subprocess.run(["/usr/bin/systemctl", "is-active", "termterm-vpn.service"], capture_output=True, text=True).stdout.strip() == "inactive", "stop_broker_before_appimage_install")
    journal_path = Path(config["journal"])
    if not journal_path.exists():
        native.trusted(journal_path.parent, True)
        journal_path.mkdir(mode=0o700)
    journal = native.Journal(journal_path)
    try:
        require(not journal.entries, "pending_owned_network_cleanup_requires_review")
        if APPDIR.exists() or APPDIR.is_symlink():
            native.trusted(APPDIR, True)
            native.trusted(APPDIR / MARKER)
            native.trusted(LAUNCHER)
            native.trusted(ENTRY)
        replacement(config, expected_old)
        if APPDIR.exists():
            native.pin(APPDIR / CLIENT, expected_old)
            for process in PROCESSES.iterdir():
                if process.name.isdigit():
                    try:
                        require((process / "exe").resolve() != APPDIR / CLIENT, "close_installed_appimage_before_upgrade")
                    except FileNotFoundError:
                        pass
        manifest_path = checked_file(manifest_path, manifest_sha256)
        with os.fdopen(os.open(manifest_path, os.O_RDONLY | os.O_NOFOLLOW), "rb") as stream:
            manifest_bytes = stream.read(4 * 1024 * 1024 + 1)
        require(len(manifest_bytes) <= 4 * 1024 * 1024 and hashlib.sha256(manifest_bytes).hexdigest() == manifest_sha256, "appimage_manifest_changed_or_too_large")
        manifest = json.loads(manifest_bytes)
        require(set(manifest) == {"version", "applicationIdentifier", "candidateVersion", "imageSha256", "clientSha256", "files"}
                and type(manifest["version"]) is int and manifest["version"] == 1 and manifest["applicationIdentifier"] == "local.termterm.desktop" and manifest["candidateVersion"] == "0.4.5", "unsupported_appimage_manifest")
        for name, pin in config["pins"].items():
            native.pin(Path(config["engines"]) / name, pin)
        native.trusted(APPDIR.parent, True)
        native.trusted(LAUNCHER.parent, True)
        native.trusted(ENTRY.parent, True)
        run = uuid.uuid4().hex
        private = APPDIR.with_name(".termterm-appimage-stage-" + run)
        private.mkdir(mode=0o700)
        stage = private / "appdir"
        with extracted(image, manifest["imageSha256"], uid) as tree:
            require(inventory(tree) == manifest["files"] and manifest["files"][CLIENT]["sha256"] == manifest["clientSha256"], "appimage_inventory_changed")
            shutil.copytree(tree, stage, symlinks=True)
        require(inventory(stage) == manifest["files"], "appimage_changed_during_install_copy")
        for directory, directories, files in os.walk(stage, followlinks=False):
            for name in directories + files:
                path = Path(directory) / name
                if not path.is_symlink():
                    path.chmod(0o755 if path.is_dir() or manifest["files"][path.relative_to(stage).as_posix()].get("executable") else 0o644)
        stage.chmod(0o755)
        native.verify_loader_paths(stage / CLIENT)
        private.chmod(0o755)
        libraries = [stage / "usr/lib", stage / "usr/lib/x86_64-linux-gnu", stage / "lib"]
        libraries = [native.trusted(path.resolve(), True) for path in libraries if path.is_dir()]
        account = pwd.getpwuid(uid)
        runtime = subprocess.run(["/usr/bin/setpriv", "--reuid=" + str(uid), "--regid=" + str(account.pw_gid), "--clear-groups", "--no-new-privs", "/usr/bin/ldd", str(stage / CLIENT)],
                                 env={"PATH": "/usr/bin:/bin", "LC_ALL": "C", "LD_LIBRARY_PATH": ":".join(map(str, libraries))},
                                 stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=15)
        require(runtime.returncode == 0 and "not found" not in runtime.stdout, "appimage_runtime_dependencies_missing")
        (stage / MARKER).write_text(json.dumps({"candidateVersion": "0.4.5", "clientSha256": manifest["clientSha256"], "manifestSha256": manifest_sha256}) + "\n")
        (stage / MARKER).chmod(0o644)
        backup = CONFIG.with_name("broker.before-appimage-" + run + ".json")
        with backup.open("xb") as stream:
            os.fchmod(stream.fileno(), 0o600)
            stream.write(CONFIG.read_bytes())
            stream.flush()
            os.fsync(stream.fileno())
        if APPDIR.exists():
            APPDIR.rename(APPDIR.with_name("termterm-appimage.previous-" + run))
        stage.rename(APPDIR)
        config.update(client=str(APPDIR / CLIENT), clientSha256=manifest["clientSha256"], clientInstall="appimage")
        pending = CONFIG.with_name(".broker-appimage-" + run)
        with pending.open("x", encoding="utf-8") as stream:
            os.fchmod(stream.fileno(), 0o600)
            json.dump(config, stream, indent=2)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(pending, CONFIG)
        LAUNCHER.write_bytes(LAUNCH)
        LAUNCHER.chmod(0o755)
        ENTRY.write_bytes(DESKTOP)
        ENTRY.chmod(0o644)
        print(json.dumps({"installed": True, "clientSha256": config["clientSha256"], "serviceStarted": False, "configurationBackup": str(backup)}))
    finally:
        os.close(journal.lock)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare", action="store_true", help="Produce the reviewed extraction inventory; do not install")
    parser.add_argument("--appimage", type=Path, required=True)
    parser.add_argument("--image-sha256", help="Preparation: exact built AppImage SHA256")
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--manifest-sha256", help="Installation: independently reviewed manifest SHA256")
    parser.add_argument("--uid", type=int, required=True)
    parser.add_argument("--expected-client-sha256", help="Replacement: exact current installed client pin; old files are retained")
    args = parser.parse_args()
    require(sys.platform == "linux" and os.geteuid() == 0, "linux_root_administrator_required")
    if args.prepare:
        prepare(args.appimage, args.image_sha256, args.uid, args.manifest)
    else:
        install(args.appimage, args.manifest, args.manifest_sha256, args.uid, args.expected_client_sha256)
