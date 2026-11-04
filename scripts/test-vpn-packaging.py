#!/usr/bin/env python3
"""Inert packaging boundary checks; no installer, network, service or engine execution."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import plistlib
import shutil
import stat
import subprocess
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


runtime = load("collect-vpn-runtime")
unix = load("prepare-vpn-unix")
windows = load("stage-vpn-windows-dev")


def archive(path, entries):
    with tarfile.open(path, "w") as result:
        for name, value in entries.items():
            info = tarfile.TarInfo(name)
            info.size = len(value)
            result.addfile(info, io.BytesIO(value))


class Packaging(unittest.TestCase):
    def test_installer_and_portable_require_their_exact_tauri_bundle_markers(self):
        original = b"MZ inert fixture __TAURI_BUNDLE_TYPE_VAR_UNK suffix"
        installed = windows.nsis_bytes(original)
        self.assertEqual(installed, b"MZ inert fixture __TAURI_BUNDLE_TYPE_VAR_NSS suffix")
        self.assertNotEqual(hashlib.sha256(installed).digest(), hashlib.sha256(original).digest())
        for invalid in [b"missing marker", installed, original + original]:
            with self.assertRaisesRegex(ValueError, "single unbundled Tauri marker"):
                windows.nsis_bytes(invalid)
    def test_mac_python_rejects_untrusted_paths_before_probe_and_requires_supported_selected_runtime(self):
        with tempfile.TemporaryDirectory() as temporary:
            executable = (Path(temporary) / "python3").resolve()
            executable.write_bytes(b"inert interpreter fixture")
            for owner, mode in [(501, stat.S_IFREG | 0o555), (0, stat.S_IFREG | 0o775), (0, stat.S_IFLNK | 0o555)]:
                with patch.object(Path, "lstat", return_value=SimpleNamespace(st_uid=owner, st_mode=mode)), patch.object(unix.subprocess, "run") as probe:
                    with self.assertRaisesRegex(ValueError, "root owned"):
                        unix.mac_python(executable)
                    probe.assert_not_called()
            with patch.object(Path, "lstat", return_value=SimpleNamespace(st_uid=0, st_mode=stat.S_IFREG | 0o555)):
                for version, selected in [([3, 9], executable), ([3, 10], executable.parent / "different-python")]:
                    with patch.object(unix.subprocess, "run", return_value=SimpleNamespace(stdout=json.dumps({"version":version,"executable":str(selected)}))):
                        with self.assertRaisesRegex(ValueError, "selected Python 3.10"):
                            unix.mac_python(executable)
                with patch.object(unix.subprocess, "run", return_value=SimpleNamespace(stdout=json.dumps({"version":[3,10],"executable":str(executable)}))) as probe:
                    self.assertEqual(unix.mac_python(executable), {"python":str(executable),"pythonSha256":hashlib.sha256(executable.read_bytes()).hexdigest()})
                    self.assertEqual(probe.call_args.args[0][:3], [str(executable), "-I", "-c"])

    def test_mac_binary_preparation_requires_explicit_python_before_creating_output(self):
        with tempfile.TemporaryDirectory(dir=ROOT / ".tools", prefix="vpn-mac-python-check-") as temporary:
            output = Path(temporary) / "must-not-exist"
            command = ["python", str(ROOT / "scripts/prepare-vpn-unix.py"), "--platform", "macos", "--output", str(output), "--engines", temporary,
                       "--client", str(Path(temporary) / "unused-client"), "--uid", "501", "--client-requirement", "identifier fixture"]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("requires --python-runtime", result.stderr)
            self.assertFalse(output.exists())

    def test_installer_preflight_rejects_incomplete_or_unsigned_collection_before_build(self):
        powershell = shutil.which("pwsh.exe")
        if not powershell:
            self.skipTest("PowerShell 7 is unavailable")
        with tempfile.TemporaryDirectory(dir=ROOT / ".tools/vpn-windows", prefix="vpn-package-reject-") as temporary:
            directory = Path(temporary)
            collection = directory / "runtime"
            collection.mkdir()
            manifest = {"developmentOnly": True, "distributionReady": False, "runtime": {str(index): {} for index in range(23)}, "packages": {"mingw-fixture": {"source": {"signatureVerified": False}}}, "missing": []}
            for update, message in [({"developmentOnly": False}, "complete DLL closure"), ({"developmentOnly": True}, "signed matching source")]:
                manifest.update(update)
                (collection / "RUNTIME.json").write_text(json.dumps(manifest))
                output = directory / "must-not-exist"
                result = subprocess.run([powershell, "-NoProfile", "-NonInteractive", "-File", str(ROOT / "scripts/package-vpn-dev.ps1"), "-Runtime", str(collection), "-Output", str(output), "-WebViewRuntime", str(directory / "unused.exe"), "-WebViewSha256", "0" * 64], cwd=ROOT, capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)
                self.assertFalse(output.exists())

    def test_source_subset_preserves_patches_and_complete_notices(self):
        files = {Path(name).as_posix() for name in unix.source_files()}
        lock = json.loads((ROOT / "scripts/vpn-windows-dependencies.json").read_bytes())
        for key in ["patch", "compatibilityPatch", "ssoPatch"]:
            self.assertIn(lock["openconnect"][key], files)
        for name in ["docs/licenses/vpn/MPL-2.0.txt", "docs/licenses/vpn/OpenVPN-Core-NOTICE.md", "vpn-engine/windows/core-packet.hpp", "scripts/vpn-dns-fixture.py", "scripts/vpn_lab_json.py"]:
            self.assertIn(name, files)

    def test_tampered_local_dependency_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "runtime.dll"
            path.write_bytes(b"inert")
            runtime.verify(path, hashlib.sha256(b"inert").hexdigest())
            with self.assertRaisesRegex(ValueError, "integrity mismatch"):
                runtime.verify(path, "0" * 64)
            path.write_bytes(b"boundary fixture" * 10000)
            self.assertEqual(runtime.digest(path, "sha512"), hashlib.sha512(path.read_bytes()).hexdigest())

    def test_notices_are_taken_from_nested_source_without_extracting_the_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            upstream = directory / "upstream.tar.gz"
            archive(upstream, {"upstream/COPYING.LESSER": b"public license fixture", "upstream/COPYING3": b"public GPL fixture", "upstream/COPYING.RUNTIME": b"public runtime exception fixture", "upstream/private.data": b"not a notice"})
            source = directory / "source.tar"
            archive(source, {"source/upstream.tar.gz": upstream.read_bytes()})
            result = runtime.source_notices(source, directory / "notices")
            self.assertEqual([path.name for path in result], ["COPYING.LESSER", "COPYING3", "COPYING.RUNTIME"])
            self.assertFalse((directory / "notices/upstream/private.data").exists())
            archive(upstream, {"../../COPYING": b"invalid path"})
            archive(source, {"source/upstream.tar.gz": upstream.read_bytes()})
            with self.assertRaisesRegex(ValueError, "Unsafe source notice"):
                runtime.source_notices(source, directory / "notices")

    def test_application_source_uses_public_code_allowlist(self):
        with tempfile.TemporaryDirectory(dir=ROOT / ".tools", prefix="vpn-source-check-") as temporary:
            result = runtime.application_source(Path(temporary) / "source.zip")
            files = result["files"]
            for path in ["src-tauri/Cargo.toml", "vpn-core/Cargo.toml", "vpn-windows-broker/src/lib.rs", "vpn-wireguard/main.go", "scripts/build-openconnect-windows.py"]:
                self.assertIn(path, files)
            self.assertFalse(any(name.startswith((".lab/", ".tools/", ".git/")) for name in files))
            self.assertFalse(any(Path(name).suffix in (".ttvault", ".pem", ".key", ".exe", ".dll") for name in files))

    def test_tgz_and_lzip_notice_readers_preserve_limits(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            upstream = directory / "upstream.tgz"
            archive(upstream, {"upstream/COPYING": b"public fixture"})
            source = directory / "source.tar"
            archive(source, {"source/upstream.tgz": upstream.read_bytes()})
            self.assertEqual(len(runtime.source_notices(source, directory / "tgz-notices")), 1)
            archive(source, {"source/upstream.tar.lz": b"inert compressed fixture"})
            def process(data):
                return SimpleNamespace(stdout=io.BytesIO(data), wait=lambda: 0, poll=lambda: 0, kill=lambda: None)
            with patch.object(runtime.sp, "Popen", side_effect=[process(b"upstream/COPYING3\n"), process(b"public GPL fixture")]):
                self.assertEqual(len(runtime.source_notices(source, directory / "lzip-notices")), 1)
            with patch.object(runtime.sp, "Popen", side_effect=[process(b"upstream/COPYING3\n"), process(b"x" * (1024 * 1024 + 1))]):
                with self.assertRaisesRegex(ValueError, "size limit"):
                    runtime.source_notices(source, directory / "oversize")
            self.assertFalse((directory / "oversize/upstream/COPYING3").exists())

    def test_source_excludes_unapproved_untracked_ignored_and_staged_files_without_reading_them(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "docs").mkdir()
            (root / "scripts").mkdir()
            (root / "src-tauri").mkdir()
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / "docs/public.md").write_text("public baseline")
            (root / ".gitignore").write_text("docs/ignored-file.txt\n")
            subprocess.run(["git", "-C", str(root), "add", "."], check=True)
            subprocess.run(["git", "-C", str(root), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "Public fixture"], check=True)
            (root / "docs/added.md").write_text("reviewed public addition")
            (root / "scripts/vpn-public-source-files.json").write_text(json.dumps({"version": 1, "files": ["docs/added.md", "scripts/vpn-public-source-files.json"]}))
            private = [root / name for name in ["docs/private-passwords.json", "docs/private-signing-key.txt", "scripts/notes.py", "docs/ignored-file.txt", "docs/staged-private.json", "src-tauri/tauri.local-private.conf.json"]]
            for path in private:
                path.write_text("inert private fixture")
            subprocess.run(["git", "-C", str(root), "add", "docs/staged-private.json"], check=True)
            original_read = Path.read_bytes
            def read_public(path):
                self.assertNotIn(path, private, "Unapproved file contents were opened")
                return original_read(path)
            with patch.object(runtime, "ROOT", root), patch.object(Path, "read_bytes", read_public):
                result = runtime.application_source(root / "source.zip")
                self.assertEqual(set(result["files"]), {"docs/public.md", "docs/added.md", "scripts/vpn-public-source-files.json"})
                self.assertEqual(runtime.source_receipt(), result["files"])
            with zipfile.ZipFile(root / "source.zip") as archive:
                self.assertFalse(any(path.relative_to(root).as_posix() in archive.namelist() for path in private))

    def test_dependency_source_archive_excludes_private_local_port_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            for relative in ["ports/openssl/portfile.cmake", "deps/vcpkg-ports/asio/portfile.cmake"]:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("public build recipe")
            (root / ".gitignore").write_text("ignored.txt\n")
            subprocess.run(["git", "-C", str(root), "add", "."], check=True)
            subprocess.run(["git", "-C", str(root), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "Public fixture"], check=True)
            for relative in ["ports/openssl/ordinary-name.json", "deps/vcpkg-ports/asio/ordinary-name.json", "ports/openssl/ignored.txt"]:
                (root / relative).write_text("inert private fixture")
            subprocess.run(["git", "-C", str(root), "diff", "--exit-code", "HEAD"], check=True, capture_output=True)
            destination = root / "public-source.tar"
            runtime.archive_source(root, destination)
            with tarfile.open(destination) as archive_file:
                names = archive_file.getnames()
                self.assertIn("ports/openssl/portfile.cmake", names)
                self.assertIn("deps/vcpkg-ports/asio/portfile.cmake", names)
                self.assertFalse(any(name.endswith(("ordinary-name.json", "ignored.txt")) for name in names))

    def test_unix_subset_uses_reviewed_paths_without_admitting_private_siblings(self):
        selected = unix.source_files()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in selected:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("public fixture")
            shutil.copyfile(ROOT / "scripts/collect-vpn-runtime.py", root / "scripts/collect-vpn-runtime.py")
            (root / "scripts/vpn-public-source-files.json").write_text(json.dumps({"version": 1, "files": []}))
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "-C", str(root), "add", "."], check=True)
            subprocess.run(["git", "-C", str(root), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "Public Unix fixture"], check=True)
            private = [root / "docs/licenses/vpn/private-signing.key", root / "vpn-engine/private-passwords.cpp"]
            for path in private:
                path.write_text("inert private fixture")
            original_read = Path.read_bytes
            def read_public(path):
                self.assertNotIn(path, private, "Unapproved Unix preparation input was opened")
                return original_read(path)
            with patch.object(unix, "ROOT", root), patch.object(Path, "read_bytes", read_public):
                self.assertEqual(unix.source_files(), selected)

    def test_asset_inventory_rejects_extra_files_and_changed_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            public = directory / "COPYING.txt"
            public.write_bytes(b"public fixture")
            files = {public.name: runtime.digest(public)}
            self.assertEqual(runtime.verified_files(directory, files), [public.name])
            private = directory / "ordinary-name.json"
            private.write_bytes(b"inert private fixture")
            with patch.object(runtime, "verify", side_effect=AssertionError("Extra asset must be rejected before opening files")):
                with self.assertRaisesRegex(ValueError, "unexpected file set"):
                    runtime.verified_files(directory, files)
            private.unlink()
            public.write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "integrity mismatch"):
                runtime.verified_files(directory, files)

    def test_preflight_copies_only_verified_assets_and_requires_mosh_before_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tools = root / ".tools/vpn-windows"
            collection = tools / "collection"
            collection.mkdir(parents=True)
            engine = tools / "engine"
            engine.mkdir()
            dlls = {}
            for index in range(23):
                path = engine / (str(index) + ".dll")
                path.write_bytes(b"inert engine fixture")
                dlls[path.name] = {"sha256": runtime.digest(path)}
            (collection / "notice.txt").write_bytes(b"inert dependency fixture")
            public = {}
            for name in ["licenses/COPYING.txt", "THIRD_PARTY.md", "VPN_WINDOWS_PREPARATION_TR.md", "VPN_TEAM_TR.md", "AUTHENTICATOR_TR.md", "VPN_DEPENDENCIES_TR.md"]:
                path = root / "docs" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("public fixture")
                public["docs/" + name] = runtime.digest(path)
            manifest = {"developmentOnly": True, "distributionReady": False, "runtime": dlls, "packages": {}, "missing": [], "files": {"notice.txt": runtime.digest(collection / "notice.txt")}, "applicationSource": {"files": public}}
            (collection / "RUNTIME.json").write_text(json.dumps(manifest))
            with patch.object(runtime, "ROOT", root), patch.object(runtime, "source_receipt", return_value=public):
                with self.assertRaisesRegex(ValueError, "fetch-mosh"):
                    runtime.package_preflight(collection, tools / "new-output")
                mosh = root / "src-tauri/resources/mosh"
                mosh.mkdir(parents=True)
                (mosh / "mosh-client.exe").write_bytes(b"inert mosh fixture")
                (mosh / "packages.json").write_text("[]")
                manifest["mosh"] = {"packagesSha256": runtime.digest(mosh / "packages.json")}
                (collection / "RUNTIME.json").write_text(json.dumps(manifest))
                (mosh / "runtime-files.json").write_text(json.dumps({"version": 1, "files": {"mosh-client.exe": runtime.digest(mosh / "mosh-client.exe"), "packages.json": runtime.digest(mosh / "packages.json")}}))
                result = runtime.package_preflight(collection, tools / "new-output")
                self.assertIn("mosh/mosh-client.exe", [asset["target"] for asset in result["assets"]])
                self.assertIn("licenses/COPYING.txt", [asset["target"] for asset in result["assets"]])
                for extra in [root / "docs/licenses/ordinary-name.json", mosh / "ordinary-name.txt", collection / "ordinary-name.key"]:
                    extra.write_text("inert private fixture")
                    with self.assertRaisesRegex(ValueError, "unexpected file set"):
                        runtime.package_preflight(collection, tools / "new-output")
                    extra.unlink()
                with self.assertRaisesRegex(ValueError, "new package output"):
                    runtime.package_preflight(collection, tools / ".." / ".." / "outside-output")
                self.assertFalse((tools / "new-output").exists())

    def test_output_and_asset_junction_ancestors_are_rejected_before_writes(self):
        powershell = shutil.which("pwsh.exe")
        if not powershell:
            self.skipTest("Windows junction creation is unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tools = root / ".tools/vpn-windows"
            tools.mkdir(parents=True)
            outside = root / "unrelated-data"
            outside.mkdir()
            junction = tools / "linked"
            quoted_junction = "'" + str(junction).replace("'", "''") + "'"
            quoted_outside = "'" + str(outside).replace("'", "''") + "'"
            subprocess.run([powershell, "-NoProfile", "-NonInteractive", "-Command", "New-Item -ItemType Junction -Path " + quoted_junction + " -Target " + quoted_outside + " | Out-Null"], check=True, capture_output=True)
            try:
                with patch.object(runtime, "ROOT", root):
                    with self.assertRaisesRegex(ValueError, "links or reparse"):
                        runtime.package_preflight(tools / "unused-runtime", junction / "must-not-exist")
                with self.assertRaisesRegex(ValueError, "links or reparse"):
                    runtime.verified_files(junction, {"COPYING.txt": "0" * 64})
                self.assertFalse((outside / "must-not-exist").exists())
                self.assertEqual(list(outside.iterdir()), [])
            finally:
                junction.rmdir()

    def test_mosh_receipt_comes_only_from_verified_archive_members(self):
        node = shutil.which("node")
        if not node or not shutil.which("tar"):
            self.skipTest("Node and tar are unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            shutil.copyfile(ROOT / "scripts/fetch-mosh.mjs", root / "scripts/fetch-mosh.mjs")
            payloads = root / "payloads"
            payloads.mkdir()
            packages = ["mosh", "cygwin", "libgcc1", "libstdc++6", "libncursesw10", "libprotobuf32", "libssl3", "zlib0", "libiconv2", "libintl8", "libzstd1", "terminfo"]
            sections = []
            for name in packages:
                path = payloads / (name + ".tar.gz")
                entries = {"usr/share/doc/" + name + "/COPYING": b"public license fixture"}
                entries["usr/bin/mosh-client.exe" if name == "mosh" else "usr/bin/" + name + ".dll"] = b"inert vendor fixture"
                archive(path, entries)
                source_name = {"libstdc++6": "libgcc1", "terminfo": "libncursesw10"}.get(name, name)
                source = payloads / (source_name + ".src.tar.gz")
                archive(source, {"source/DoNotUnpack.txt": b"inert source fixture"})
                sections.append("\n@ " + name + "\nversion: 1\ninstall: " + path.name + " " + str(path.stat().st_size) + " " + runtime.digest(path, "sha512") + "\nsource: " + source.name + " " + str(source.stat().st_size) + " " + runtime.digest(source, "sha512") + "\n")
            (payloads / "setup.ini").write_text("".join(sections), newline="\n")
            runtime_dir = root / "src-tauri/resources/mosh"
            (runtime_dir / "usr/bin").mkdir(parents=True)
            (runtime_dir / "usr/bin/ordinary-private.dll").write_bytes(b"inert private fixture")
            (runtime_dir / "notes.json").write_bytes(b"inert private fixture")
            bootstrap = root / "fixture.mjs"
            bootstrap.write_text('import fs from "node:fs/promises";\nimport {pathToFileURL} from "node:url";\n'
                + "const payloads=" + json.dumps(str(payloads)) + ";\n"
                + 'globalThis.fetch=async url=>{const name=new URL(url).pathname.split("/").pop();const bytes=await fs.readFile(payloads+"/"+name);return {ok:true,text:async()=>bytes.toString(),arrayBuffer:async()=>bytes};};\n'
                + "await import(pathToFileURL(" + json.dumps(str(root / "scripts/fetch-mosh.mjs")) + "));\n")
            completed = subprocess.run([node, str(bootstrap)], capture_output=True, text=True)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            manifest = json.loads((runtime_dir / "runtime-files.json").read_bytes())
            self.assertIn("bin/mosh-client.exe", manifest["files"])
            self.assertNotIn("notes.json", manifest["files"])
            self.assertNotIn("usr/bin/ordinary-private.dll", manifest["files"])
            self.assertFalse((runtime_dir / "bin/ordinary-private.dll").exists())
            self.assertEqual((runtime_dir / "notes.json").read_bytes(), b"inert private fixture")
            for relative, expected in manifest["files"].items():
                self.assertEqual(runtime.digest(runtime_dir / relative), expected)
            source_cache = root / ".tools/cygwin/sources"
            source_manifest = json.loads((source_cache / "SOURCES.json").read_bytes())
            self.assertEqual(len(source_manifest["sources"]), 10)
            self.assertFalse((source_cache / "source/DoNotUnpack.txt").exists())
            for name, pin in source_manifest["sources"].items():
                self.assertEqual(runtime.digest(source_cache / name, "sha512"), pin["sha512"])
                self.assertEqual((source_cache / name).stat().st_size, pin["bytes"])
            with self.assertRaisesRegex(ValueError, "unexpected file set"):
                runtime.verified_files(runtime_dir, manifest["files"], ("runtime-files.json",))

    def test_mosh_source_collection_requires_binary_bound_pins_and_exact_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            mosh = root / "src-tauri/resources/mosh"
            mosh.mkdir(parents=True)
            cache = root / ".tools/cygwin/sources"
            cache.mkdir(parents=True)
            source = cache / "fixture.src.tar.xz"
            archive(source, {"source/COPYING3": b"public license fixture"})
            pin = {"url": "https://cygwin.com/pub/cygwin/fixture.src.tar.xz", "sha512": runtime.digest(source, "sha512"), "bytes": source.stat().st_size}
            (mosh / "packages.json").write_text(json.dumps([{"source": pin["url"], "sourceSha512": pin["sha512"], "sourceBytes": pin["bytes"]}]))
            (mosh / "runtime-files.json").write_text(json.dumps({"version": 1, "files": {"packages.json": runtime.digest(mosh / "packages.json")}}))
            (cache / "SOURCES.json").write_text(json.dumps({"version": 1, "sources": {source.name: pin}}))
            with patch.object(runtime, "ROOT", root):
                result = runtime.mosh_sources(root / "collected", root / "notices")
                self.assertEqual(result["sourceBytes"], pin["bytes"])
                self.assertEqual((root / "collected" / source.name).read_bytes(), source.read_bytes())
                extra = cache / "ordinary-name.json"
                extra.write_bytes(b"inert private fixture")
                with self.assertRaisesRegex(ValueError, "unexpected file set"):
                    runtime.mosh_sources(root / "must-not-exist", root / "must-not-exist-notices")
                self.assertFalse((root / "must-not-exist").exists())
                extra.unlink()
                source.write_bytes(b"tampered")
                with self.assertRaisesRegex(ValueError, "integrity mismatch"):
                    runtime.mosh_sources(root / "must-not-exist", root / "must-not-exist-notices")

    def test_matching_signed_recipe_is_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            recipe = b"public fixture source recipe"
            binary = directory / "binary.tar"
            archive(binary, {".BUILDINFO": ("pkgbuild_sha256sum = " + hashlib.sha256(recipe).hexdigest() + "\n").encode()})
            source = directory / "fixture-1.src.tar.zst"
            archive(source, {"fixture/PKGBUILD": recipe})
            (directory / (source.name + ".sig")).write_bytes(b"inert detached signature")
            trusted = "[GNUPG:] VALIDSIG " + "A" * 40 + " 2026-10-30\n[GNUPG:] TRUST_FULLY 0 pgp\n"
            with patch.object(runtime.sp, "run", return_value=SimpleNamespace(stdout=trusted)):
                result = runtime.signed_source({"base": "fixture", "version": "1"}, binary, directory)
                self.assertTrue(result["signatureVerified"])
                archive(source, {"fixture/PKGBUILD": b"different recipe"})
                with self.assertRaisesRegex(ValueError, "differs from binary"):
                    runtime.signed_source({"base": "fixture", "version": "1"}, binary, directory)
            with patch.object(runtime.sp, "run", return_value=SimpleNamespace(stdout=trusted.replace("TRUST_FULLY", "TRUST_UNDEFINED"))):
                with self.assertRaisesRegex(ValueError, "no trusted"):
                    runtime.signed_source({"base": "fixture", "version": "1"}, binary, directory)

    def test_source_only_outputs_remain_inert_and_refuse_replacement(self):
        with tempfile.TemporaryDirectory(dir=ROOT / ".tools", prefix="vpn-preparation-check-") as temporary:
            for platform in ["linux", "macos"]:
                output = Path(temporary) / platform
                command = ["python", str(ROOT / "scripts/prepare-vpn-unix.py"), "--platform", platform, "--source-only", "--output", str(output)]
                subprocess.run(command, check=True, capture_output=True)
                manifest = json.loads((output / "PREPARATION.json").read_bytes())
                self.assertFalse(manifest["distributionReady"])
                self.assertFalse(manifest["installed"])
                self.assertFalse(manifest["serviceStarted"])
                self.assertFalse(manifest["macosNativeTested"])
                if platform == "macos":
                    self.assertFalse(manifest["macosRuntime"]["pythonConfigured"])
                    plist = plistlib.loads((output / "rootfs/Library/LaunchDaemons" / (unix.IDENTITY + ".plist")).read_bytes())
                    self.assertEqual(plist["ProgramArguments"][:2], ["/REQUIRES_VERIFIED_PYTHON_3_10_OR_NEWER/python3", "-I"])
                    for directory in manifest["macosRuntime"]["directories"]:
                        self.assertTrue((output / "rootfs" / directory.lstrip("/")).is_dir())
                for relative, expected in manifest["sources"].items():
                    self.assertEqual(hashlib.sha256((output / "source" / relative).read_bytes()).hexdigest(), expected)
                self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)


if __name__ == "__main__":
    unittest.main()
