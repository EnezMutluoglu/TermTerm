#!/usr/bin/env python3
"""Focused AppDir trust/copy/upgrade checks using inert fixtures; no real installation or service."""
from contextlib import contextmanager
import importlib.util
import io
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@unittest.skipIf(os.name == "nt", "Linux pwd/fcntl and symlink checks run in WSL")
class AppImage(unittest.TestCase):
    def setUp(self):
        self.app = load("install-vpn-appimage")
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def tree(self, root, value=b"inert ELF fixture"):
        (root / "usr/bin").mkdir(parents=True)
        (root / "usr/bin/termterm").write_bytes(b"\x7fELF" + value)
        (root / "usr/bin/termterm").chmod(0o755)
        (root / "AppRun").symlink_to("usr/bin/termterm")
        return root

    def test_service_shutdown_handles_sigterm_delivered_to_an_owner_thread(self):
        script = r'''
import importlib.util, os, signal, socket, sys, threading, time
from pathlib import Path
from types import SimpleNamespace
spec = importlib.util.spec_from_file_location("native", sys.argv[1])
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
native.SOCKET = sys.argv[2]
native.pin = lambda *args: None
native.trusted = lambda *args: None
native.os.chown = lambda *args: None
native.authenticate = lambda *args: os.open("/dev/null", os.O_RDONLY)
policy = SimpleNamespace(terminate=lambda: None, wait=lambda timeout: None)
native.NativeBroker = lambda config: SimpleNamespace(policy=policy)
class Owner:
    def __init__(self, connection, broker): self.connection = connection
    def run(self):
        time.sleep(.15)
        self.connection.sendall(b"owner-sigterm\n")
        signal.pthread_kill(threading.get_ident(), signal.SIGTERM)
        self.connection.recv(1)
native.NativeOwner = Owner
try:
    native.serve({"uid": 1000, "client": "unused", "clientSha256": "unused"})
except KeyboardInterrupt:
    pass
assert not Path(native.SOCKET).exists()
print("clean-shutdown", flush=True)
'''
        path = self.root / "broker.sock"
        process = subprocess.Popen([sys.executable, "-c", script, str(ROOT / "scripts/vpn-unix-broker.py"), str(path)],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 5
            while not path.is_socket() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(.02)
            self.assertTrue(path.is_socket())
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(5)
                connection.connect(str(path))
                self.assertEqual(connection.recv(64), b"owner-sigterm\n")
                stdout, stderr = process.communicate(timeout=3)
            self.assertEqual(process.returncode, 0, stderr)
            self.assertEqual(stdout.strip(), "clean-shutdown")
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=3)

    def test_inventory_rejects_escape_dangling_special_and_setid_members(self):
        tree = self.tree(self.root / "tree")
        good = self.app.inventory(tree)
        self.assertEqual(good["AppRun"], {"kind": "symlink", "target": "usr/bin/termterm"})
        for target in ["/etc/passwd", "../../outside", "missing"]:
            link = tree / "unsafe"
            link.symlink_to(target)
            with self.assertRaises(ValueError):
                self.app.inventory(tree)
            link.unlink()
        os.mkfifo(tree / "fifo")
        with self.assertRaises(ValueError):
            self.app.inventory(tree)
        (tree / "fifo").unlink()
        (tree / "usr/bin/termterm").chmod(0o4755)
        with self.assertRaises(ValueError):
            self.app.inventory(tree)

    def test_extract_verifies_bytes_and_always_drops_privileges(self):
        image = self.root / "image"
        image.write_bytes(b"inert image fixture")
        expected = self.app.digest(image)
        def extract(arguments, **options):
            self.assertIn("--reuid=1000", arguments)
            self.assertIn("--clear-groups", arguments)
            self.assertIn("--no-new-privs", arguments)
            self.assertEqual(arguments[-1], "--appimage-extract")
            self.assertEqual(self.app.digest(Path(arguments[-2])), expected)
            self.tree(Path(options["cwd"]) / "squashfs-root")
        with patch.object(self.app.pwd, "getpwuid", return_value=SimpleNamespace(pw_gid=1000)), patch.object(self.app.os, "chown"), \
             patch.object(self.app.subprocess, "run", side_effect=extract) as run:
            with self.app.extracted(image, expected, 1000) as tree:
                self.assertTrue((tree / self.app.CLIENT).is_file())
            self.assertEqual(run.call_count, 1)
            for uid, pin in [(0, expected), (True, expected), (1000, "0" * 64)]:
                with self.assertRaises(ValueError), self.app.extracted(image, pin, uid):
                    pass
            self.assertEqual(run.call_count, 1)

    def test_broker_accepts_only_fixed_root_appdir_with_explicit_installation_kind(self):
        native = load("vpn-unix-broker")
        config = {"uid": 1000, "client": "/opt/termterm-appimage/usr/bin/termterm", "clientSha256": "0" * 64,
                  "clientInstall": "appimage", "applicationIdentifier": "local.termterm.desktop", "candidateVersion": "0.4.4",
                  "engines": "/opt/termterm-vpn/bin", "journal": "/var/lib/termterm-vpn",
                  "pins": {name: "0" * 64 for name in ["vpn-policy-lab", "termterm-openvpn", "termterm-openconnect", "termterm-wireguard"]}}
        native.validate_configuration(config, "0.4.4")
        for update in [{"clientInstall": "transient"}, {"clientInstall": "deb"}, {"client": "/tmp/.mount_termterm/usr/bin/termterm"}, {"client": "/home/user/AppDir/usr/bin/termterm"}]:
            with self.assertRaises(ValueError):
                native.validate_configuration({**config, **update}, "0.4.4")
        with self.assertRaises(ValueError):
            native.validate_configuration(config)

    def test_upgrade_requires_owned_marker_matching_old_pin_and_unchanged_launchers(self):
        directory, launcher, entry = self.root / "installed", self.root / "launcher", self.root / "entry"
        with patch.multiple(self.app, APPDIR=directory, LAUNCHER=launcher, ENTRY=entry):
            self.app.replacement({}, None)
            launcher.write_bytes(b"foreign")
            with self.assertRaises(ValueError):
                self.app.replacement({}, None)
            launcher.unlink()
            self.tree(directory)
            pin = self.app.digest(directory / self.app.CLIENT)
            config = {"clientInstall": "appimage", "clientSha256": pin}
            launcher.write_bytes(self.app.LAUNCH)
            entry.write_bytes(self.app.DESKTOP)
            with self.assertRaises(ValueError):
                self.app.replacement(config, pin)
            marker = directory / self.app.MARKER
            for version in ["0.4.3", "0.4.4"]:
                marker.write_text(json.dumps({"candidateVersion": version, "clientSha256": pin}))
                self.app.replacement(config, pin)
                for expected in [None, "0" * 64]:
                    with self.assertRaises(ValueError):
                        self.app.replacement(config, expected)
            for version in ["0.4.2", "0.4.5"]:
                marker.write_text(json.dumps({"candidateVersion": version, "clientSha256": pin}))
                with self.assertRaises(ValueError):
                    self.app.replacement(config, pin)
            self.assertEqual(self.app.digest(directory / self.app.CLIENT), pin)

    def test_install_rejects_copy_tampering_and_retains_original_config(self):
        tree = self.tree(self.root / "source")
        manifest = {"version": 1, "applicationIdentifier": "local.termterm.desktop", "candidateVersion": "0.4.4", "imageSha256": "0" * 64,
                    "clientSha256": self.app.digest(tree / self.app.CLIENT), "files": self.app.inventory(tree)}
        path = self.root / "manifest.json"
        path.write_text(json.dumps(manifest))
        config_path = self.root / "broker.json"
        journal = self.root / "journal"
        journal.mkdir()
        processes = self.root / "processes"
        processes.mkdir()
        config = {"uid": 1000, "client": "/usr/bin/termterm", "pins": {}, "journal": str(journal), "engines": str(self.root)}
        original = json.dumps(config).encode()
        config_path.write_bytes(original)
        @contextmanager
        def extracted(*args):
            yield tree
        native = SimpleNamespace(trusted=lambda *args: None, validate_configuration=lambda *args: None,
                                 Journal=lambda *args: SimpleNamespace(entries={}, lock=os.open(self.root / "lock", os.O_CREAT | os.O_RDWR, 0o600)),
                                 verify_loader_paths=lambda *args: None)
        fake_spec = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda *args: None))
        def tamper(source, destination, **options):
            self.tree(destination, b"changed during copy")
        with patch.multiple(self.app, APPDIR=self.root / "installed", CONFIG=config_path, LAUNCHER=self.root / "launcher", ENTRY=self.root / "entry", PROCESSES=processes), \
             patch.object(self.app.importlib.util, "spec_from_file_location", return_value=fake_spec), patch.object(self.app.importlib.util, "module_from_spec", return_value=native), \
             patch.object(self.app.pwd, "getpwuid"), patch.object(self.app.subprocess, "run", return_value=SimpleNamespace(stdout="inactive\n")), \
             patch.object(self.app, "extracted", extracted), patch.object(self.app.shutil, "copytree", side_effect=tamper):
            with self.assertRaisesRegex(ValueError, "changed_during_install_copy"):
                self.app.install(self.root / "unused", path, self.app.digest(path), 1000)
            self.assertEqual(config_path.read_bytes(), original)
            self.assertFalse(self.app.APPDIR.exists())
            self.assertFalse(self.app.LAUNCHER.exists())

    def test_install_and_explicit_replacement_preserve_old_bytes_and_reject_bad_prerequisites(self):
        tree = self.tree(self.root / "source", b"first version")
        manifest_path, config_path = self.root / "manifest.json", self.root / "broker.json"
        journal = self.root / "journal"
        journal.mkdir()
        processes = self.root / "processes"
        processes.mkdir()
        config_path.write_text(json.dumps({"uid": 1000, "client": "/usr/bin/termterm", "pins": {}, "journal": str(journal), "engines": str(self.root)}))
        def manifest():
            value = {"version": 1, "applicationIdentifier": "local.termterm.desktop", "candidateVersion": "0.4.4", "imageSha256": "0" * 64,
                     "clientSha256": self.app.digest(tree / self.app.CLIENT), "files": self.app.inventory(tree)}
            manifest_path.write_text(json.dumps(value))
            return self.app.digest(manifest_path)
        @contextmanager
        def extracted(*args):
            yield tree
        state = {"service": "inactive", "pending": False, "missing": False}
        def run(arguments, **options):
            if arguments[0] == "/usr/bin/systemctl":
                return SimpleNamespace(stdout=state["service"] + "\n", returncode=3)
            self.assertIn("--reuid=1000", arguments)
            self.assertIn("--no-new-privs", arguments)
            self.assertIn("/usr/bin/ldd", arguments)
            return SimpleNamespace(stdout="libfixture => not found" if state["missing"] else "all fixture dependencies resolved", returncode=0)
        native = SimpleNamespace(trusted=lambda path, *args: Path(path), validate_configuration=lambda *args: None, pin=lambda *args: None,
                                 Journal=lambda *args: SimpleNamespace(entries={"fixture": {}} if state["pending"] else {}, lock=os.open(self.root / "lock", os.O_CREAT | os.O_RDWR, 0o600)),
                                 verify_loader_paths=lambda *args: None)
        spec = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda *args: None))
        with patch.multiple(self.app, APPDIR=self.root / "installed", CONFIG=config_path, LAUNCHER=self.root / "launcher", ENTRY=self.root / "entry", PROCESSES=processes), \
             patch.object(self.app.importlib.util, "spec_from_file_location", return_value=spec), patch.object(self.app.importlib.util, "module_from_spec", return_value=native), \
             patch.object(self.app.pwd, "getpwuid", return_value=SimpleNamespace(pw_gid=1000)), patch.object(self.app.subprocess, "run", side_effect=run), \
             patch.object(self.app, "extracted", extracted), patch("sys.stdout", new_callable=io.StringIO):
            pin = manifest()
            original = config_path.read_bytes()
            for uid in [0, 1001]:
                with self.assertRaisesRegex(ValueError, "configured_nonroot_uid"):
                    self.app.install(self.root / "unused", manifest_path, pin, uid)
            state["service"] = "active"
            with self.assertRaisesRegex(ValueError, "stop_broker"):
                self.app.install(self.root / "unused", manifest_path, pin, 1000)
            state.update(service="inactive", pending=True)
            with self.assertRaisesRegex(ValueError, "pending_owned_network"):
                self.app.install(self.root / "unused", manifest_path, pin, 1000)
            self.assertEqual(config_path.read_bytes(), original)
            state["pending"] = False
            checked = self.app.checked_file
            def changed_manifest(path, expected):
                value = checked(path, expected)
                value.write_bytes(b"changed after the first hash check")
                return value
            with patch.object(self.app, "checked_file", side_effect=changed_manifest), self.assertRaisesRegex(ValueError, "manifest_changed"):
                self.app.install(self.root / "unused", manifest_path, pin, 1000)
            self.assertEqual(config_path.read_bytes(), original)
            pin = manifest()
            self.app.install(self.root / "unused", manifest_path, pin, 1000)
            installed_marker = self.app.APPDIR / self.app.MARKER
            old_marker = json.loads(installed_marker.read_text())
            old_marker["candidateVersion"] = "0.4.3"
            installed_marker.write_text(json.dumps(old_marker))
            old = self.app.digest(self.app.APPDIR / self.app.CLIENT)
            installed = config_path.read_bytes()
            self.assertEqual(json.loads(installed)["clientInstall"], "appimage")
            self.assertEqual(self.app.LAUNCHER.read_bytes(), self.app.LAUNCH)
            self.assertEqual(self.app.ENTRY.read_bytes(), self.app.DESKTOP)
            (tree / self.app.CLIENT).write_bytes(b"\x7fELFsecond version")
            pin = manifest()
            running = processes / "123"
            running.mkdir()
            (running / "exe").symlink_to(self.app.APPDIR / self.app.CLIENT)
            with self.assertRaisesRegex(ValueError, "close_installed_appimage"):
                self.app.install(self.root / "unused", manifest_path, pin, 1000, old)
            self.assertEqual(config_path.read_bytes(), installed)
            (running / "exe").unlink()
            state["missing"] = True
            with self.assertRaisesRegex(ValueError, "runtime_dependencies_missing"):
                self.app.install(self.root / "unused", manifest_path, pin, 1000, old)
            self.assertEqual(config_path.read_bytes(), installed)
            self.assertEqual(self.app.digest(self.app.APPDIR / self.app.CLIENT), old)
            state["missing"] = False
            self.app.install(self.root / "unused", manifest_path, pin, 1000, old)
            self.assertNotEqual(json.loads(config_path.read_text())["clientSha256"], old)
            backups = list(self.root.glob("termterm-appimage.previous-*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(self.app.digest(backups[0] / self.app.CLIENT), old)
            self.assertTrue(any(path.read_bytes() == installed for path in self.root.glob("broker.before-appimage-*.json")))


if __name__ == "__main__":
    unittest.main()
