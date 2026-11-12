#!/usr/bin/env python3
"""Inert candidate identity, service pairing and source-boundary checks; never install or connect."""
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Candidate(unittest.TestCase):
    def test_packaged_helper_rpath_rejects_build_missing_and_extra_loader_paths(self):
        linux = load("package-vpn-linux")
        for tag in ["RPATH", "RUNPATH"]:
            linux.verify_packaged_helper_rpath(f" 0x000000000000001d ({tag}) Library runpath: [$ORIGIN/../lib]\n")
        for value in ["", "/opt/termterm-vpn-dev/lib", "$ORIGIN/../lib:/opt/termterm-vpn-dev/lib", "$ORIGIN/../lib:", "/tmp/lib"]:
            with self.assertRaises(ValueError):
                linux.verify_packaged_helper_rpath(f" 0x000000000000001d (RUNPATH) Library runpath: [{value}]\n")
        for value in ["No dynamic section", "(RUNPATH) [$ORIGIN/../lib]\n(RPATH) [$ORIGIN/../lib]", "(RUNPATH) [$ORIGIN/../lib]\n(RUNPATH) [/usr/lib]"]:
            with self.assertRaises(ValueError):
                linux.verify_packaged_helper_rpath(value)

    def test_release_overlay_retains_the_existing_signed_feed_without_publication(self):
        stable = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())
        candidate = json.loads((ROOT / "src-tauri/tauri.vpn-candidate.conf.json").read_text())
        release = json.loads((ROOT / "src-tauri/tauri.vpn-release-candidate.conf.json").read_text())
        self.assertEqual(release["version"], "0.4.4")
        team = (ROOT / "team-client/src/compatibility.rs").read_text()
        self.assertIn('#[cfg(feature = "vpn-candidate")]\npub const CLIENT_VERSION: &str = "' + release["version"] + '";', team)
        self.assertEqual(release["identifier"], candidate["identifier"])
        self.assertEqual(release["mainBinaryName"], candidate["mainBinaryName"])
        self.assertEqual(release["plugins"]["updater"], {"endpoints": stable["plugins"]["updater"]["endpoints"]})
        self.assertTrue(stable["plugins"]["updater"]["pubkey"])
        self.assertFalse(release["bundle"]["createUpdaterArtifacts"])
        self.assertEqual(candidate["plugins"]["updater"]["endpoints"], [])
        # Tauri merges these nested updater objects; the overlay must retain the original signature key.
        effective = {**stable["plugins"]["updater"], **candidate["plugins"]["updater"], **release["plugins"]["updater"]}
        self.assertEqual(effective["pubkey"], stable["plugins"]["updater"]["pubkey"])
        self.assertEqual(effective["endpoints"], stable["plugins"]["updater"]["endpoints"])
    def test_candidate_and_development_packaging_contracts_cannot_be_relabelled(self):
        windows = load("stage-vpn-windows-dev")
        linux = load("package-vpn-linux")
        self.assertEqual(windows.contract(), ("termterm-vpn-dev.exe", "vpn-dev-bundle.json", "local.termterm.desktop.vpndev"))
        self.assertEqual(windows.contract("0.4.4"), ("termterm.exe", "vpn-bundle.json", "local.termterm.desktop"))
        self.assertEqual(linux.package_contract("0.4.4")["service"], "termterm-vpn")
        self.assertEqual(linux.package_contract("0.4.4")["package"], "term-term")
        self.assertEqual(linux.package_contract()["name"], "termterm-vpn-dev")
        for module, function in [(windows, "contract"), (linux, "package_contract")]:
            for version in ["0.4.2", "0.4.5", "0.4.3-dev.4", ""]:
                with self.assertRaises(ValueError):
                    getattr(module, function)(version)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            selected = root / "artifacts/vpn-delivery-20261105/candidate-0.4.4/linux"
            self.assertEqual(linux.delivery_path(selected, root, "0.4.4"), selected)
            for path, candidate in [(selected, None), (root / "artifacts/vpn-delivery-20261105/candidate-0.4.5/linux", "0.4.4"),
                                    (root / "outside/vpn-delivery-20261105/candidate-0.4.4/linux", "0.4.4")]:
                with self.assertRaises(ValueError):
                    linux.delivery_path(path, root, candidate)

    def test_service_templates_use_distinct_fixed_production_paths_and_explicit_version(self):
        unix = load("prepare-vpn-unix")
        unit = unix.linux_unit("0.4.4")
        self.assertIn("ExecStart=/usr/bin/python3 -I /opt/termterm-vpn/bin/vpn-unix-broker.py --candidate 0.4.4\n", unit)
        self.assertIn("RuntimeDirectory=termterm-vpn\n", unit)
        self.assertIn("StateDirectory=termterm-vpn\n", unit)
        self.assertNotIn("termterm-vpn-dev", unit)
        self.assertEqual(unix.linux_unit(), unix.LINUX_UNIT)
        plist = plistlib.loads(unix.mac_plist("0.4.4").encode())
        self.assertEqual(plist["Label"], "local.termterm.desktop.broker")
        self.assertEqual(plist["ProgramArguments"][-2:], ["--candidate", "0.4.4"])
        self.assertNotIn("vpndev", json.dumps(plist))
        for function in [unix.linux_unit, unix.mac_plist]:
            with self.assertRaises(ValueError):
                function("0.4.5")

    def test_binary_mac_manifest_keeps_service_directory_when_copying_helpers(self):
        unix = load("prepare-vpn-unix")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            engines = root / "engines"
            engines.mkdir()
            for name in ["termterm-openconnect", "termterm-openvpn", "termterm-wireguard", "vpn-policy-lab", "termterm-vpn-macos"]:
                (engines / name).write_bytes(b"inert helper fixture")
            (root / "scripts").mkdir()
            for name in ["vpn-unix-broker.py", "vpn-lab-broker.py"]:
                (root / "scripts" / name).write_text("# inert fixture\n")
            client = root / "client"
            client.write_bytes(b"inert desktop fixture")
            output = root / ".tools/prepared"
            arguments = ["prepare", "--platform", "macos", "--candidate", "0.4.4", "--output", str(output), "--engines", str(engines),
                         "--client", str(client), "--uid", "501", "--client-requirement", "identifier fixture", "--python-runtime", str(client)]
            with patch.object(unix, "ROOT", root), patch.object(unix, "source_files", return_value=[]), \
                 patch.object(unix, "mac_python", return_value={"python": "/verified/python3", "pythonSha256": "0" * 64}), patch.object(sys, "argv", arguments):
                unix.main()
            manifest = json.loads((output / "PREPARATION.json").read_text())
            self.assertEqual(manifest["macosRuntime"]["directories"]["/private/var/run/termterm-vpn"], "root:0755")
            self.assertFalse(any("vpn-lab-broker.py" in path for path in manifest["macosRuntime"]["directories"]))
            self.assertTrue((output / "rootfs/private/var/run/termterm-vpn").is_dir())
            self.assertFalse(manifest["macosNativeTested"])

    @unittest.skipIf(os.name == "nt", "Unix imports require fcntl; run this case in WSL")
    def test_native_candidate_rejects_development_and_transient_appimage_pairing(self):
        native = load("vpn-unix-broker")
        config = {"uid": 1000, "client": "/usr/bin/termterm", "clientSha256": "0" * 64, "engines": "/opt/termterm-vpn/bin",
                  "pins": {name: "0" * 64 for name in ["vpn-policy-lab", "termterm-openvpn", "termterm-openconnect", "termterm-wireguard"]},
                  "journal": "/var/lib/termterm-vpn", "applicationIdentifier": "local.termterm.desktop", "candidateVersion": "0.4.4"}
        with patch.object(native.sys, "platform", "linux"):
            native.validate_configuration(config, "0.4.4")
            self.assertEqual(native.contract("0.4.4")["socket"], "/run/termterm-vpn/broker.sock")
            self.assertNotEqual(native.contract()["config"], native.contract("0.4.4")["config"])
            for key, value in [("uid", 0), ("uid", True), ("client", "/usr/bin/termterm-vpn-dev"), ("client", "/tmp/.mount_AppImage/usr/bin/termterm"),
                               ("engines", "/opt/termterm-vpn-dev/bin"), ("journal", "/var/lib/termterm-vpn-dev"),
                               ("applicationIdentifier", "local.termterm.desktop.vpndev"), ("candidateVersion", "0.4.5"), ("labSocket", "/tmp/lab.sock"), ("pins", {})]:
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    native.validate_configuration({**config, key: value}, "0.4.4")
            with self.assertRaises(ValueError):
                native.validate_configuration(config)
            broker = object.__new__(native.NativeBroker)
            broker.config = config
            self.assertEqual(broker.hello()["identity"], "local.termterm.desktop.broker")
            self.assertFalse(broker.hello()["labOnly"])
        with patch.object(native.sys, "platform", "darwin"):
            mac = {**config, "client": "/Applications/TermTerm.app/Contents/MacOS/termterm",
                   "pins": {**config["pins"], "termterm-vpn-macos": "0" * 64},
                   "engines": "/Library/PrivilegedHelperTools/local.termterm.desktop.broker", "journal": "/Library/Application Support/TermTerm VPN/journal"}
            native.validate_configuration(mac, "0.4.4")
            with self.assertRaises(ValueError):
                native.validate_configuration(config, "0.4.4")

    def test_public_source_includes_candidate_files_and_preserves_private_exclusion(self):
        runtime = load("collect-vpn-runtime")
        paths = runtime.source_paths()
        for path in [".env.vpn-candidate", "src-tauri/tauri.vpn-candidate.conf.json", "src-tauri/tauri.vpn-candidate-linux.conf.json", "src-tauri/tauri.vpn-release-candidate.conf.json", "vpn-windows-broker/src/bundle.rs", "scripts/install-vpn-appimage.py", "scripts/test-vpn-appimage.py"]:
            self.assertIn(path, paths)
        self.assertFalse(any(path.startswith((".tools/", ".lab/", "artifacts/")) for path in paths))
        linux = load("package-vpn-linux")
        self.assertEqual(str(linux.safe_name(".env.vpn-candidate")), ".env.vpn-candidate")
        with self.assertRaises(ValueError):
            linux.safe_name(".env.private")


if __name__ == "__main__":
    unittest.main()
