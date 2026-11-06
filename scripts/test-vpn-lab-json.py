#!/usr/bin/env python3
"""Socket-free checks for the disposable VPN lab's shared JSON boundary."""
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch

import vpn_lab_json as shared

ROOT = Path(__file__).resolve().parents[1]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def dns_failure(directory, tcp):
    dns = load("json_dns_check", "vpn-dns-fixture.py")
    name = "probe.fixture.termterm.test"
    wire = b"".join(bytes([len(label)]) + label.encode() for label in name.split(".")) + b"\0"
    packet = struct.pack("!6H", 123, 0x100, 1, 0, 0, 0) + wire + struct.pack("!HH", 1, 1)
    connection = MagicMock()
    connection.recv.side_effect = [struct.pack("!H", len(packet)), packet]
    servers = []

    class Server:
        def __init__(self, address, handler):
            self.handler = handler
            servers.append(self)
        def __enter__(self):
            return self
        def __exit__(self, *args):
            return False
        def serve_forever(self, **kwargs):
            pass
        def shutdown(self):
            pass
        def handle_request(self):
            handler = object.__new__(servers[int(tcp)].handler)
            handler.request = connection if tcp else (packet, connection)
            handler.client_address = ("127.0.0.1", 1)
            try:
                handler.handle()
            except Exception:
                self.handle_error(None, None)

    receipt = directory / "dns-receipt.json"
    original_replace, writes = shared.replace, 0

    def fail_runtime_write(temporary, path):
        nonlocal writes
        writes += 1
        if writes > 1:
            raise PermissionError(errno.EACCES, "held observer")
        original_replace(temporary, path)

    with patch.object(dns.socketserver, "UDPServer", Server), patch.object(dns.socketserver, "TCPServer", Server), patch.object(shared, "replace", fail_runtime_write), patch("time.sleep"):
        try:
            dns.serve("10.88.0.53", name, "10.88.0.53", receipt)
        except dns.ReceiptWriteFailure:
            assert shared.read_json(receipt)["udpAnswers"] == 0
            assert shared.read_json(receipt)["tcpAnswers"] == 0
            assert writes == 101 and not list(directory.glob(".*.tmp"))
            connection.sendto.assert_not_called()
            connection.sendall.assert_not_called()
            raise
        raise AssertionError("DNS fixture continued healthy after exhausted receipt failure")


class JsonChecks(unittest.TestCase):
    def test_read_retry_and_parse_boundary(self):
        valid = b'{"generation":7}'
        for failure in [PermissionError(errno.EACCES, "busy"), FileNotFoundError(errno.ENOENT, "busy")]:
            with patch.object(Path, "read_bytes", side_effect=[failure, valid]) as read, patch("time.sleep") as sleep:
                self.assertEqual(shared.read_json(Path("unused")), {"generation": 7})
                self.assertEqual((read.call_count, sleep.call_count), (2, 1))
            with patch.object(Path, "read_bytes", side_effect=failure) as read, patch("time.sleep") as sleep:
                with self.assertRaises(type(failure)):
                    shared.read_json(Path("unused"))
                self.assertEqual((read.call_count, sleep.call_count), (100, 99))
        with patch.object(Path, "read_bytes", return_value=b"{") as read, patch("time.sleep") as sleep:
            with self.assertRaises(json.JSONDecodeError):
                shared.read_json(Path("unused"))
            self.assertEqual(read.call_count, 1)
            sleep.assert_not_called()

    def test_unrelated_error_is_not_retried(self):
        for failure in [OSError(errno.EPERM, "denied"), OSError(errno.EIO, "disk")]:
            with patch("time.sleep") as sleep:
                operation = MagicMock(side_effect=failure)
                with self.assertRaises(OSError):
                    shared.retry_io(operation)
                self.assertEqual(operation.call_count, 1)
                sleep.assert_not_called()
        failure = PermissionError(errno.EACCES, "ACL denied")
        failure.winerror = 5
        with patch("time.sleep") as sleep, self.assertRaises(PermissionError):
            shared.retry_io(MagicMock(side_effect=failure))
        sleep.assert_not_called()

    def test_windows_sharing_errors_are_bounded(self):
        for code in [32, 33]:
            failure = OSError(errno.EACCES, "sharing")
            failure.winerror = code
            operation = MagicMock(side_effect=[failure, "complete"])
            with patch("time.sleep") as sleep:
                self.assertEqual(shared.retry_io(operation), "complete")
                self.assertEqual((operation.call_count, sleep.call_count), (2, 1))

    def test_replace_1175_retains_complete_destination_and_temporary(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "receipt.json"
            shared.write_json(path, {"generation": 0})
            before = path.read_bytes()
            original, attempts = shared.replace, 0

            def interrupted(temporary, destination):
                nonlocal attempts
                attempts += 1
                if attempts < 3:
                    self.assertEqual(destination.read_bytes(), before)
                    self.assertEqual(shared.read_json(temporary), {"generation": 1})
                    failure = OSError(errno.EACCES, "retained replacement names")
                    failure.winerror = 1175 if attempts == 1 else 32
                    raise failure
                return original(temporary, destination)

            with patch.object(shared, "replace", interrupted), patch("time.sleep") as sleep:
                shared.write_json(path, {"generation": 1})
                self.assertEqual((attempts, sleep.call_count), (3, 2))
            self.assertEqual(shared.read_json(path), {"generation": 1})
            self.assertFalse(list(path.parent.glob(".*.tmp")))

            before = path.read_bytes()
            failure = OSError(errno.EACCES, "persistent retained replacement")
            failure.winerror = 1175
            with patch.object(shared, "replace", side_effect=failure) as replace, patch("time.sleep") as sleep:
                with self.assertRaises(OSError):
                    shared.write_json(path, {"generation": 2})
                self.assertEqual((replace.call_count, sleep.call_count), (100, 99))
            self.assertEqual(path.read_bytes(), before)
            self.assertFalse(list(path.parent.glob(".*.tmp")))

    def test_replace_retry_does_not_adopt_partial_state_or_acl_failures(self):
        for code, options in [(1175, {}), (1175, {"reading": True}),
                              (5, {"replacing": True}), (1176, {"replacing": True}), (1177, {"replacing": True})]:
            failure = OSError(errno.EACCES, "not a safe replacement retry")
            failure.winerror = code
            operation = MagicMock(side_effect=failure)
            with patch("time.sleep") as sleep, self.assertRaises(OSError):
                shared.retry_io(operation, **options)
            self.assertEqual(operation.call_count, 1)
            sleep.assert_not_called()

    def test_complete_private_write_exclusive_and_failure_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "control.json"
            shared.write_json(path, {"generation": 0}, exclusive=True)
            before = path.read_bytes()
            with self.assertRaises(FileExistsError):
                shared.write_json(path, {"generation": 99}, exclusive=True)
            self.assertEqual(path.read_bytes(), before)
            original = shared.replace
            with patch.object(shared, "replace", side_effect=[PermissionError(errno.EACCES, "busy"), None]) as replace, patch("time.sleep") as sleep:
                shared.write_json(path, {"generation": 1})
                self.assertEqual((replace.call_count, sleep.call_count), (2, 1))
            with patch.object(shared, "replace", side_effect=PermissionError(errno.EACCES, "held")), patch("time.sleep") as sleep:
                with self.assertRaises(PermissionError):
                    shared.write_json(path, {"generation": 2})
                self.assertEqual(sleep.call_count, 99)
            self.assertEqual(path.read_bytes(), before)
            shared.write_json(path, {"generation": 3})
            self.assertEqual(shared.read_json(path), {"generation": 3})
            self.assertFalse(list(path.parent.glob(".*.tmp")))
            if os.name == "posix":
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_baseline_is_immutable_digest_bound_and_incarnation_bound(self):
        fixture = load("json_fixture_check", "test-wireguard-lab.py")
        context = {"bootId": "original", "pid1StartTicks": "17"}
        original_text, original_bytes = Path.read_text, Path.read_bytes

        def text(path, *args, **kwargs):
            return " ".join(["0"] * 21 + ["23"]) if path.as_posix() == "/proc/self/stat" else original_text(path, *args, **kwargs)

        def data(path):
            return b"synthetic fixture command" if path.as_posix() == "/proc/self/cmdline" else original_bytes(path)

        with tempfile.TemporaryDirectory() as temporary, patch.object(fixture, "linux_incarnation", return_value=context), patch.object(Path, "read_text", text), patch.object(Path, "read_bytes", data):
            directory = Path(temporary)
            digest = fixture.persist_baseline(directory, "fixture", {"dns": "original"})
            path = directory / "linux-baseline-before.json"
            before = path.read_bytes()
            self.assertTrue(fixture.compare_baseline(path, "fixture", digest, context, {"dns": "original"}))
            self.assertFalse(fixture.compare_baseline(path, "fixture", digest, context, {"dns": "changed"}))
            self.assertIsNone(fixture.compare_baseline(path, "fixture", digest, {**context, "pid1StartTicks": "18"}, {"dns": "original"}))
            with self.assertRaises(FileExistsError):
                fixture.persist_baseline(directory, "fixture", {"dns": "override"})
            self.assertEqual(path.read_bytes(), before)
            shared.write_json(path, {"fixtureId": "other"})
            with self.assertRaisesRegex(ValueError, "digest_changed"):
                fixture.compare_baseline(path, "fixture", digest, context, {})

    def test_dns_receipt_exhaustion_is_unhealthy_for_udp_and_tcp(self):
        for tcp in [False, True]:
            with tempfile.TemporaryDirectory() as temporary:
                with self.assertRaisesRegex(RuntimeError, "dns_receipt_write_failed"):
                    dns_failure(Path(temporary), tcp)

    def test_dns_unhealthy_child_exits(self):
        with tempfile.TemporaryDirectory() as temporary:
            process = subprocess.run([sys.executable, __file__, "--dns-failure-child", temporary], capture_output=True, text=True, timeout=5)
            self.assertNotEqual(process.returncode, 0)
            self.assertIn("dns_receipt_write_failed", process.stderr)
            self.assertEqual(shared.read_json(Path(temporary) / "dns-receipt.json")["tcpAnswers"], 0)


PS_WORKER = r"""param([string]$Mode,[string]$Directory,[string]$Root,[double]$Deadline)
$ErrorActionPreference='Stop'
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Security/Microsoft.PowerShell.Security.psd1')
foreach ($entry in @(@('validate-dns-v1.ps1','Read-SharedJson'),@('validate-wireguard-v1.ps1','Write-RelayLossFile'))) {
    $errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $Root ('vpn-windows-broker/scripts/'+$entry[0])),[ref]$null,[ref]$errors)
    if ($errors) {throw 'Production helper parse failed'}
    $definition=@($ast.EndBlock.Statements | Where-Object {$_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -ceq $entry[1]})
    if ($definition.Count -ne 1) {throw 'Expected one production helper'}
    . ([scriptblock]::Create($definition[0].Extent.Text))
}
function Before-Deadline {([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()/1000) -lt $Deadline}
function Check-Value($Value,[int]$Previous) {
    if ($Value.fixtureId -cne 'proof' -or $Value.generation -lt $Previous -or $Value.generation -gt 100 -or $Value.payload -cne ('x'*4096)) {throw 'Incomplete or stale JSON snapshot'}
}
function Get-AclProof([string]$Path) {
    $acl=Get-Acl -LiteralPath $Path
    $raw=[Security.AccessControl.RawSecurityDescriptor]::new($acl.GetSecurityDescriptorBinaryForm(),0)
    $raw.SetFlags($raw.ControlFlags -band (-bnot [Security.AccessControl.ControlFlags]::DiscretionaryAclAutoInherited))
    @{sddl=$acl.Sddl;semantic=$raw.GetSddlForm([Security.AccessControl.AccessControlSections]::All);protected=$acl.AreAccessRulesProtected}
}
$path=Join-Path $Directory 'data.json'
if ($Mode -ceq 'read-protected') {
    $acl=Get-Acl -LiteralPath $path;$acl.SetAccessRuleProtection($true,$true);Set-Acl -LiteralPath $path -AclObject $acl
}
$aclBeforeRead=if ($Mode -like 'read*') {Get-AclProof $path} else {$null}
$identity=[ordered]@{pid=$PID;bornUtc=[Diagnostics.Process]::GetCurrentProcess().StartTime.ToUniversalTime().ToString('o')}
Write-RelayLossFile (Join-Path $Directory ('windows-'+$Mode+'-ready.json')) ($identity | ConvertTo-Json -Compress)
while (-not (Test-Path -LiteralPath (Join-Path $Directory 'go'))) {if (-not (Before-Deadline)) {throw 'Owned worker deadline'};Start-Sleep -Milliseconds 10}
$reads=0;$last=0;$observedIoErrors=0;$aclPreserved=$null;$aclExactSddl=$null;$aclAutoInheritedBitOnly=$null;$aclProtected=$null
if ($Mode -ceq 'write') {
    $acl=Get-Acl -LiteralPath $path;$acl.SetAccessRuleProtection($true,$true);Set-Acl -LiteralPath $path -AclObject $acl;$before=(Get-Acl -LiteralPath $path).Sddl
    for ($generation=1;$generation -le 100;$generation++) {
        if (-not (Before-Deadline)) {throw 'Owned writer deadline'}
        Write-RelayLossFile $path (@{fixtureId='proof';generation=$generation;payload=('x'*4096)} | ConvertTo-Json -Compress)
    }
    $aclPreserved=(Get-Acl -LiteralPath $path).Sddl -ceq $before
    if (-not $aclPreserved) {throw 'Protected destination ACL changed'}
    [IO.File]::WriteAllText((Join-Path $Directory 'done'),'done')
} else {
    while (-not (Test-Path -LiteralPath (Join-Path $Directory 'done'))) {
        if (-not (Before-Deadline)) {throw 'Owned reader deadline'}
        try {
            $value=if ($Mode -ceq 'observe') {Get-Content -LiteralPath $path -Raw -ErrorAction Stop | ConvertFrom-Json} else {Read-SharedJson $path}
            Check-Value $value $last;$last=$value.generation;$reads++
        } catch [IO.IOException],[UnauthorizedAccessException],[Management.Automation.ItemNotFoundException] {
            if ($Mode -cne 'observe') {throw}
            $observedIoErrors++
        }
    }
    $value=Read-SharedJson $path;Check-Value $value $last
    if ($value.generation -ne 100) {throw 'Final generation missing'}
    if ($Mode -like 'read*') {
        $after=Get-AclProof $path
        $aclPreserved=$after.semantic -ceq $aclBeforeRead.semantic
        $aclExactSddl=$after.sddl -ceq $aclBeforeRead.sddl
        $aclAutoInheritedBitOnly=$aclPreserved -and -not $aclExactSddl
        $aclProtected=$after.protected
        if (-not $aclPreserved -or ($Mode -ceq 'read-protected' -and -not $aclExactSddl)) {throw 'Replacement changed the destination ACL beyond its auto-inherited control bit'}
    }
}
Write-RelayLossFile (Join-Path $Directory ('windows-'+$Mode+'-result.json')) (@{ok=$true;reads=$reads;observedIoErrors=$observedIoErrors;aclPreserved=$aclPreserved;aclExactSddl=$aclExactSddl;aclAutoInheritedBitOnly=$aclAutoInheritedBitOnly;aclProtected=$aclProtected} | ConvertTo-Json -Compress)
"""


def linux_worker(mode, directory, deadline):
    import time
    directory = Path(directory)
    identity = {"pid": os.getpid(), "startTicks": Path("/proc/self/stat").read_text().split()[21],
                "commandLineSha256": hashlib.sha256(Path("/proc/self/cmdline").read_bytes()).hexdigest()}
    shared.write_json(directory / "linux-ready.json", identity, exclusive=True)
    while not (directory / "go").exists():
        assert time.time() < deadline, "Owned Linux worker deadline"
        time.sleep(.01)
    errors, reads, last = {}, 0, 0
    original_read, original_replace = Path.read_bytes, shared.replace

    def observed(operation, *args):
        try:
            return operation(*args)
        except OSError as failure:
            key = f"{failure.errno}/{getattr(failure, 'winerror', None)}"
            errors[key] = errors.get(key, 0) + 1
            raise

    with patch.object(Path, "read_bytes", lambda path: observed(original_read, path)), patch.object(shared, "replace", lambda temporary, path: observed(original_replace, temporary, path)):
        if mode == "write":
            for generation in range(1, 101):
                assert time.time() < deadline, "Owned Linux writer deadline"
                shared.write_json(directory / "data.json", {"fixtureId": "proof", "generation": generation, "payload": "x" * 4096})
            (directory / "done").write_text("done")
        else:
            while not (directory / "done").exists():
                assert time.time() < deadline, "Owned Linux reader deadline"
                value = shared.read_json(directory / "data.json")
                assert value["fixtureId"] == "proof" and value["payload"] == "x" * 4096
                assert last <= value["generation"] <= 100
                last, reads = value["generation"], reads + 1
            assert shared.read_json(directory / "data.json")["generation"] == 100
    shared.write_json(directory / "linux-result.json", {"ok": True, "reads": reads, "retriedIoErrors": errors})


def integration(report_path):
    import time
    assert os.name == "nt", "Run the cross-OS proof on Windows with Ubuntu-24.04"
    assert not Path(report_path).exists(), "Preserve previous interop evidence"
    deadline = time.time() + 45
    worker_deadline = deadline - 5
    directory = Path(tempfile.mkdtemp(prefix="termterm-json-interop-")).resolve()
    assert directory.is_relative_to(Path(tempfile.gettempdir()).resolve())
    worker_script = directory / "worker.ps1"
    worker_script.write_text(PS_WORKER, encoding="utf-8")
    source = Path(__file__).resolve()

    def linux_path(path):
        value = path.resolve().as_posix()
        assert value[1:3] == ":/", "Expected this task's Windows drive"
        return "/mnt/" + value[0].lower() + value[2:]

    def probe_linux(identity, stop):
        code = """
import pathlib,hashlib,json,os,signal,time
pid=PID;expected_start=START;expected_hash=HASH
def matches():
 try:
  p=pathlib.Path('/proc')/str(pid)
  return p.joinpath('stat').read_text().split()[21]==expected_start and hashlib.sha256(p.joinpath('cmdline').read_bytes()).hexdigest()==expected_hash
 except (FileNotFoundError,ProcessLookupError):return False
signalled=False
if STOP and matches():
 os.kill(pid,signal.SIGTERM);signalled=True;until=time.monotonic()+2
 while matches() and time.monotonic()<until:time.sleep(.01)
 if matches():os.kill(pid,signal.SIGKILL)
until=time.monotonic()+2
while matches() and time.monotonic()<until:time.sleep(.01)
print(json.dumps({'originalProcessAbsent':not matches(),'exactIdentitySignalled':signalled,'pid':pid,'startTicks':expected_start}))
""".replace("PID", str(identity["pid"])).replace("START", repr(identity["startTicks"])).replace("HASH", repr(identity["commandLineSha256"])).replace("STOP", repr(stop))
        result = subprocess.run(["wsl.exe", "-d", "Ubuntu-24.04", "-u", "root", "--", "python3", "-c", code], capture_output=True, text=True, timeout=8)
        assert result.returncode == 0, result.stderr
        value = json.loads(result.stdout)
        assert value["originalProcessAbsent"], "Owned Linux writer/reader remains"
        return value

    report = {"ok": False, "deadlineSeconds": 45, "cases": [], "networkMutations": False, "vpn": False, "uac": False}
    all_children, linux_identities, linux_cases = [], [], []

    def cleanup_linux_case(mode, case):
        expected = [linux_path(source), "--worker", mode, linux_path(case), str(worker_deadline)]
        code = """
import pathlib,json,hashlib,os,signal,time
expected=EXPECTED
def find():
 result=[]
 for p in pathlib.Path('/proc').iterdir():
  if not p.name.isdigit():continue
  try:
   raw=p.joinpath('cmdline').read_bytes()
   if [a.decode() for a in raw.split(b'\\0')[1:-1]]==expected:result.append((int(p.name),p.joinpath('stat').read_text().split()[21],hashlib.sha256(raw).hexdigest()))
  except (FileNotFoundError,ProcessLookupError,PermissionError):pass
 return result
found=find()
for pid,start,digest in found:
 def same():return (pid,start,digest) in find()
 if same():os.kill(pid,signal.SIGTERM)
 until=time.monotonic()+2
 while same() and time.monotonic()<until:time.sleep(.01)
 if same():os.kill(pid,signal.SIGKILL)
until=time.monotonic()+2
while find() and time.monotonic()<until:time.sleep(.01)
print(json.dumps({'originalProcessAbsent':not find(),'exactIdentitySignalled':[p[0] for p in found]}))
""".replace("EXPECTED", repr(expected))
        result = subprocess.run(["wsl.exe", "-d", "Ubuntu-24.04", "-u", "root", "--", "python3", "-c", code], capture_output=True, text=True, timeout=8)
        assert result.returncode == 0, result.stderr
        value = json.loads(result.stdout)
        assert value["originalProcessAbsent"], "Exact disposable Linux worker remains"
        return value
    try:
        for case_name in ["write", "read", "python-write-inherited", "python-write-protected"]:
            python_writer = case_name.startswith("python-write-")
            linux_mode = "read" if python_writer else case_name
            case = directory / case_name
            case.mkdir(mode=0o700)
            linux_cases.append((linux_mode, case))
            shared.write_json(case / "data.json", {"fixtureId": "proof", "generation": 0, "payload": "x" * 4096})
            linux = subprocess.Popen(["wsl.exe", "-d", "Ubuntu-24.04", "-u", "root", "--", "python3", linux_path(source),
                                      "--worker", linux_mode, linux_path(case), str(worker_deadline)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            modes = (["read-protected" if case_name == "python-write-protected" else "read", "observe"]
                     if python_writer or linux_mode == "write" else ["write", "observe"])
            windows = [subprocess.Popen(["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(worker_script),
                                         mode, str(case), str(ROOT), str(worker_deadline)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for mode in modes]
            children = [linux, *windows]
            all_children.extend(children)
            ready = [case / "linux-ready.json", *(case / ("windows-" + mode + "-ready.json") for mode in modes)]
            while not all(path.exists() for path in ready):
                assert time.time() < deadline and all(child.poll() is None for child in children), "Owned worker failed before ready"
                time.sleep(.01)
            linux_identity = shared.read_json(ready[0])
            linux_identities.append(linux_identity)
            windows_identities = [shared.read_json(path) for path in ready[1:]]
            assert [value["pid"] for value in windows_identities] == [child.pid for child in windows]
            (case / "go").write_text("go")
            writer_proof = None
            if python_writer:
                import ctypes
                writer_proof = {"writes": 0, "injected1175": 0, "actualWinErrors": {}, "injectedFailureRetainedCompleteNames": False}
                original_replace = shared.replace

                def observed_replace(temporary, path):
                    if not writer_proof["injected1175"]:
                        old, new = shared.read_json(path), shared.read_json(temporary)
                        assert old["generation"] == 0 and new["generation"] == 1
                        assert old["payload"] == new["payload"] == "x" * 4096
                        writer_proof["injectedFailureRetainedCompleteNames"] = path.exists() and temporary.exists()
                        writer_proof["injected1175"] += 1
                        raise ctypes.WinError(1175)
                    try:
                        return original_replace(temporary, path)
                    except OSError as failure:
                        code = str(getattr(failure, "winerror", None))
                        writer_proof["actualWinErrors"][code] = writer_proof["actualWinErrors"].get(code, 0) + 1
                        raise

                with patch.object(shared, "replace", observed_replace):
                    for generation in range(1, 101):
                        for _ in range(5):
                            assert time.time() < worker_deadline, "Owned Python writer deadline"
                            shared.write_json(case / "data.json", {"fixtureId": "proof", "generation": generation, "payload": "x" * 4096})
                            writer_proof["writes"] += 1
                (case / "done").write_text("done")
            for child in children:
                stdout, stderr = child.communicate(timeout=max(.1, deadline - time.time()))
                assert child.returncode == 0, stderr or stdout
            assert shared.read_json(case / "data.json")["generation"] == 100
            result = {"direction": ("Windows-Python-to-WSL-and-Windows-" + case_name.removeprefix("python-write-") if python_writer
                                    else "WSL-to-Windows" if linux_mode == "write" else "Windows-to-WSL"),
                      "linuxIdentity": linux_identity, "windowsIdentities": windows_identities,
                      "linux": shared.read_json(case / "linux-result.json"),
                      "windows": [shared.read_json(case / ("windows-" + mode + "-result.json")) for mode in modes],
                      "cleanup": probe_linux(linux_identity, False), "generations": 100}
            if writer_proof:
                result["pythonWriter"] = writer_proof
            report["cases"].append(result)
        assert time.time() < deadline, "Global interop proof deadline"
        report["ok"] = True
    finally:
        for child in all_children:
            if child.poll() is None:
                child.kill()
            child.communicate(timeout=3)
        report["linuxFinalCleanup"] = [cleanup_linux_case(mode, case) for mode, case in linux_cases]
        report["windowsChildrenExited"] = all(child.poll() is not None for child in all_children)
        report["temporaryEvidenceDirectory"] = str(directory)
        Path(report_path).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--dns-failure-child":
        dns_failure(Path(sys.argv[2]), True)
    elif len(sys.argv) > 1 and sys.argv[1] == "--worker":
        linux_worker(sys.argv[2], sys.argv[3], float(sys.argv[4]))
    elif len(sys.argv) > 1 and sys.argv[1] == "--integration":
        integration(sys.argv[2])
    else:
        unittest.main()
