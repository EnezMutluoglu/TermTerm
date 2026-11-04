#!/usr/bin/env python3
"""Authenticated Unix service checks; invoked only by the isolated VPN lab fixture."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import socket
import subprocess as sp
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
from unittest.mock import patch
import uuid


def load(source):
    spec = importlib.util.spec_from_file_location("native_broker", source / "scripts/vpn-unix-broker.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def credential_checks(source, policy):
    native = load(source)
    broker = native.lab.Broker(Path("/unused-engines"), policy)
    requests = []
    policy_call = broker.policy_call
    def checked_call(request):
        requests.append(request)
        return policy_call(request)
    broker.policy_call = checked_call
    normal = {"fields": [{"name": "user", "purpose": "text", "label": "Username:"},
                          {"name": "passwd", "purpose": "secret", "secret": True, "label": "Password:"}],
              "message": "Enter login credentials"}
    explicit = {"fields": [{"name": "username", "purpose": "username"},
                            {"name": "password", "purpose": "password", "secret": True}]}
    def run(forms, remember=True, password="Disposable saved password", broken_policy=False):
        messages, writes = [], []
        owner = SimpleNamespace(vault_id="fixture", send=messages.append)
        profile = {"family": "globalProtect", "server": "https://vpn.example", "realm": "",
                   "remember": remember, "username": "fixture-user", "password": password}
        session = native.lab.Session(broker, owner, str(uuid.uuid4()), 1, str(uuid.uuid4()), profile)
        events = [{"kind": "challenge", "detail": {**form, "id": i + 1}} for i, form in enumerate(forms)]
        events.append({"kind": "disconnected"})
        process = SimpleNamespace(stdout=io.BytesIO(b"".join((json.dumps(e) + "\n").encode() for e in events)),
                                  poll=lambda: 0, wait=lambda timeout: None)
        session.launch = lambda engine: process
        session.write = writes.append
        if broken_policy:
            with patch.object(broker, "policy_call", side_effect=ValueError("network_policy_unavailable")):
                session.run()
        else:
            session.run()
        return [w for w in writes if w.get("action") == "answer"], messages
    try:
        for form, names in [(normal, ("user", "passwd")), (explicit, ("username", "password"))]:
            answers, messages = run([form, form])
            assert len(answers) == 1 and answers[0]["values"] == {names[0]: "fixture-user", names[1]: "Disposable saved password"}
            assert sum(m["session"]["state"] == "challenge" for m in messages) == 1, "Rejected login must prompt, never retry"
        for marker in ["OTP", "one-time password", "Verification token", "Challenge:"]:
            for field in ["message", "error", "label"]:
                form = json.loads(json.dumps(normal))
                if field == "label":
                    form["fields"][1]["label"] = marker
                else:
                    form[field] = marker
                assert run([form, normal])[0] == [], "OTP or rejected first secret must consume autofill"
        for remember, password in [(False, "Disposable saved password"), (True, "")]:
            answers, messages = run([normal], remember, password)
            assert answers == []
            challenge = next(m["session"]["challenge"] for m in messages if m["session"]["state"] == "challenge")
            assert challenge["fields"][0]["value"] == "fixture-user" and "value" not in challenge["fields"][1]
        choice = {"fields": [{"name": "gateway", "purpose": "choice", "choices": []}]}
        assert len(run([choice, normal])[0]) == 1
        answers, messages = run([normal], broken_policy=True)
        assert not answers and messages[-1]["session"]["state"] == "failed"
        form = {**normal, "private": {"cookie": "fixture-private-cookie"},
                "fields": [{**f, "value": "fixture-private-value"} for f in normal["fields"]]}
        assert len(run([form])[0]) == 1
        assert all(set(r["challenge"]) == {"fields", "message", "error"} and
                   all(set(f) == {"name", "purpose", "label"} for f in r["challenge"]["fields"]) for r in requests)
        assert all(secret not in json.dumps(requests) for secret in ["Disposable saved password", "fixture-user", "fixture-private-cookie", "fixture-private-value"])
        pipe_in, pipe_out = broker.policy.stdin, broker.policy.stdout
        def locked_write(value):
            assert broker.lock._is_owned(), "Policy request write must stay serialized"
            return pipe_in.write(value)
        def locked_read(size):
            assert broker.lock._is_owned(), "Policy response read must stay serialized"
            return pipe_out.readline(size)
        errors = []
        def concurrent(request, expected):
            try:
                for _ in range(25):
                    assert policy_call(request)["result"] == expected
            except Exception as error:
                errors.append(error)
        with patch.object(broker.policy, "stdin", SimpleNamespace(write=locked_write, flush=pipe_in.flush)), \
                patch.object(broker.policy, "stdout", SimpleNamespace(readline=locked_read)):
            # A WireGuard validation request must not consume a credential policy reply.
            wireguard = {"action": "wireGuardConfig", "profile": {"version": 1, "label": "Fixture", "family": "wireGuard",
                         "source": "manual", "server": "", "groupId": None, "username": "", "password": "", "realm": "", "remember": False,
                         "config": "[Interface]\nPrivateKey = AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=\nAddress = 10.77.0.2/32\n[Peer]\nPublicKey = AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=\nEndpoint = 198.18.0.1:51820\nAllowedIPs = 10.77.0.0/24\n"}}
            wireguard_expected = policy_call(wireguard)["result"]
            threads = [threading.Thread(target=concurrent, args=(request, expected)) for request, expected in [
                ({"action": "passwordChallengeFields", "challenge": normal}, ["user", "passwd"]), (wireguard, wireguard_expected)]]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join(5)
                assert not thread.is_alive(), "Concurrent policy requests stalled"
            assert not errors, errors
    finally:
        broker.policy.stdin.close()
        broker.policy.wait(timeout=3)


def policy_checks(source):
    native = load(source)
    # Runtime pins also work on Python 3.10, which has no hashlib.file_digest.
    with patch.object(native.hashlib, "file_digest", None, create=True):
        for value in [b"", b"a" * (1024 * 1024 + 1)]:
            assert native.sha256(io.BytesIO(value)) == hashlib.sha256(value).hexdigest()
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "inert-helper"
            executable.write_bytes(b"not executable")
            with patch.object(native, "trusted", return_value=executable):
                assert native.pin(executable, hashlib.sha256(executable.read_bytes()).hexdigest()) == executable
                try:
                    native.pin(executable, "0" * 64)
                except ValueError as error:
                    assert str(error) == "executable_pin_mismatch"
                else:
                    raise AssertionError("Wrong executable pin accepted")
    detail = {"addresses": [{"cidr": "10.72.0.2/32"}], "routes": ["0.0.0.0/0"], "excludedRoutes": ["10.50.0.0/16"], "remote": "198.18.8.1", "dns": ["10.72.0.53"], "dnsDomains": ["office.example"], "mtu": 1420}
    network = native.plan(detail)
    import ipaddress
    routes = [ipaddress.ip_network(value) for value in network["routes"]]
    assert network["fullTunnel"] is True
    assert not native.plan({**detail, "routes": ["0.0.0.0/1", "128.0.0.0/1"]})["fullTunnel"]
    for excluded in ["198.18.8.1", "10.50.1.1", "127.0.0.1", "169.254.1.1", "224.0.0.1", "255.255.255.255"]:
        assert not any(ipaddress.ip_address(excluded) in route for route in routes)
    assert any(ipaddress.ip_address("10.72.0.53") in route for route in routes)
    assert network["domains"] == ["office.example"] and network["searchDomains"] == []
    mixed = native.plan({**detail, "dnsDomains": ["OFFICE.example.", "route.example"], "searchDomain": "Search.EXAMPLE. office.example SEARCH.example"})
    assert mixed["domains"] == ["office.example", "route.example", "search.example"]
    assert mixed["searchDomains"] == ["search.example", "office.example"]
    assert native.plan({**detail, "dnsDomains": [], "searchDomain": " "})["domains"] == []
    for update in [{"address6": "fd00::2/64"}, {"dns": ["127.0.0.1"]}, {"dnsDomains": ["bad;command"]}, {"searchDomain": "bad;command"}, {"dns": [], "searchDomain": "search.example"}, {"dnsDomains": ["office.example"] * 64, "searchDomain": "search.example"}, {"proxyPac": "https://example.test"}, {"mtu": True}, {"routes": ["fd00::/64"]}, {"remote": "example.test"}, {"command": "no"}, {"fullTunnel": True}, {"dnsOptions": {"unsupported": True}}]:
        try:
            native.plan({**detail, **update})
        except (ValueError, TypeError, KeyError):
            pass
        else:
            raise AssertionError("Unsafe network plan accepted")
    for raw in [b'{"action":"hello","action":"close"}\n', b'{}', b'[]\n', b'x' * (native.lab.LIMIT + 1)]:
        try:
            native.frame(io.BytesIO(raw))
        except ValueError:
            pass
        else:
            raise AssertionError("Malformed protocol frame accepted")

    linux = native.LinuxNetwork.__new__(native.LinuxNetwork)
    linux.ip, linux.resolver = "ip", "resolvectl"
    full = native.plan({**detail, "dns": [], "dnsDomains": []})
    physical_route = {"dst": "10.0.2.0/24", "dev": "eth0", "protocol": "kernel", "scope": "link", "prefsrc": "10.0.2.15"}
    physical_link = {"ifname": "eth0", "link_type": "ether", "flags": ["UP", "LOWER_UP"]}
    physical_address = {"ifname": "eth0", "addr_info": [{"family": "inet", "local": "10.0.2.15", "prefixlen": 24, "scope": "global"}]}
    def preflight(candidate, route=None, link=None, address=None, transport=None):
        def output(_, *args):
            if args == ("status",):
                return b""
            if args == ("-j", "-4", "route", "show", "table", "main"):
                return json.dumps([physical_route if route is None else route]).encode()
            if args == ("-j", "-4", "route", "get", full["remote"]):
                return json.dumps([{"dev": "eth0"} if transport is None else transport]).encode()
            if args == ("-j", "-d", "link", "show", "dev", "eth0"):
                return json.dumps([physical_link if link is None else link]).encode()
            if args == ("-j", "-4", "addr", "show", "dev", "eth0"):
                return json.dumps([physical_address if address is None else address]).encode()
            raise AssertionError("Unexpected preflight command: " + repr(args))
        with patch.object(native, "command", side_effect=output):
            linux.preflight(candidate, [])
    preflight(full)
    preflight(full, link={**physical_link, "linkinfo": {"info_kind": "veth"}})
    for candidate, route, link, address, transport in [
            ({key: value for key, value in full.items() if key != "fullTunnel"}, None, None, None, None),
            ({**full, "fullTunnel": False}, None, None, None, None),
            (native.plan({**detail, "routes": ["10.0.0.0/8"], "dns": [], "dnsDomains": []}), None, None, None, None),
            (native.plan({**detail, "routes": ["0.0.0.0/1", "128.0.0.0/1"], "dns": [], "dnsDomains": []}), None, None, None, None),
            ({**full, "addresses": ["10.0.2.100/24"]}, None, None, None, None),
            *[(full, {**physical_route, **change}, None, None, None) for change in [
                {"protocol": "boot"}, {"scope": "global"}, {"gateway": "10.0.2.2"}, {"type": "blackhole"},
                {"dev": "other-vpn"}, {"prefsrc": "10.0.2.99"}, {"dst": "10.0.2.0/25"}, {"dst": "10.0.0.0/8"}]],
            *[(full, None, {**physical_link, "linkinfo": {"info_kind": kind}}, None, None) for kind in ["tun", "wireguard", "dummy", "bridge", "bond", "vlan"]],
            (full, None, {**physical_link, "link_type": "none"}, None, None),
            (full, None, {**physical_link, "flags": []}, None, None),
            (full, None, None, {**physical_address, "addr_info": []}, None),
            (full, None, None, None, {"dev": "eth0", "type": "blackhole"})]:
        try:
            preflight(candidate, route, link, address, transport)
        except ValueError as error:
            assert str(error) == "existing_route_conflict", error
        else:
            raise AssertionError("Unverified full-tunnel route exception accepted")
    with patch.object(native, "command", return_value=b'[{"dev":"eth0"}]'):
        assert linux.full_tunnel_underlay(full, {"eth0"}) == (None, {}), "Active VPN must not become a physical underlay"
    full_dns = native.plan({**detail, "dns": ["10.0.2.53"], "dnsDomains": []})
    with patch.object(native.Path, "resolve", return_value=Path("/run/systemd/resolve/stub-resolv.conf")), patch.object(native, "trusted"):
        try:
            preflight(full_dns)
        except ValueError as error:
            assert str(error) == "dns_conflicts_with_physical_underlay", error
        else:
            raise AssertionError("VPN DNS escaped through a preserved physical connected prefix")
    mac = native.DarwinNetwork.__new__(native.DarwinNetwork)
    mac.helper, mac.boot = Path("/fixture-helper"), "current-boot"
    split = native.plan({**detail, "routes": ["10.72.0.0/24"]})
    for candidate, rows in [(full, b"10.0.2/24 link#4 UCS en0\n"), (split, b"10.72/16 192.0.2.1 UGS en9\n")]:
        with patch.object(native, "command", return_value=rows):
            try:
                mac.preflight(candidate, [])
            except ValueError as error:
                assert str(error) == "existing_route_conflict", error
            else:
                raise AssertionError("Unverified Darwin LAN or foreign route exception accepted")
    with patch.object(native, "command", return_value=b"10.0.2/24 link#4 UCS en0\n"):
        mac.preflight(split, [])
    for domains, searches, expected in [
            (mixed["domains"], mixed["searchDomains"], ["search.example", "office.example", "~route.example"]),
            (["office.example"], None, ["~office.example"]),
            ([], [], ["~."])]:
        network = {"addresses": [], "routes": [], "mtu": 1420, "dns": ["10.72.0.53"], "domains": domains}
        if searches is not None:
            network["searchDomains"] = searches
        entry = {"interface": "ttv000000000000", "marker": "fixture", "id": str(uuid.uuid4()), "boot": "old-boot", "plan": network}
        with patch.object(native, "command") as command:
            linux.apply(entry)
            assert command.call_args_list[-2].args == ("resolvectl", "domain", entry["interface"], *expected)
            assert command.call_args_list[-1].args == ("resolvectl", "default-route", entry["interface"], "no" if domains else "yes")
        with patch.object(native, "command") as command:
            mac.apply(entry)
            mac.cleanup(entry)
            for call, operation in zip(command.call_args_list[-2:], ("dns-set", "dns-remove")):
                assert call.args == (str(mac.helper), operation, entry["interface"], entry["id"])
                assert json.loads(call.kwargs["input"]) == {"interface": entry["interface"], "dns": network["dns"], "domains": domains, "searchDomains": searches or []}


def journal_checks(source):
    native = load(source)
    for deleting in (False, True):
        with tempfile.TemporaryDirectory(prefix="termterm-vpn-journal-", dir="/run") as directory:
            Path(directory).chmod(0o700)
            journal = native.Journal(directory)
            paused, resume, scheduled, finish = [threading.Event() for _ in range(4)]
            applied = {name: threading.Event() for name in ("first", "second")}

            class Lock:
                def __init__(self):
                    self.mutex = threading.RLock()

                def __enter__(self):
                    if not self.mutex.acquire(blocking=False):
                        scheduled.set()
                        self.mutex.acquire()

                def __exit__(self, *_):
                    self.mutex.release()

            class Process:
                def __init__(self, message):
                    self.message = message
                    self.stdin, self.stdout = io.BytesIO(), self
                    self.returncode = None

                def readline(self, _):
                    if self.message:
                        result, self.message = self.message, None
                        return (json.dumps(result) + "\n").encode()
                    assert finish.wait(5), "Journal fixture cleanup was not released"
                    return b'{"kind":"disconnected"}\n'

                def poll(self):
                    return self.returncode

                def terminate(self):
                    self.returncode = 0

                def wait(self, **_):
                    return self.returncode

            def apply(entry):
                assert entry["id"] in json.loads(journal.path.read_text()), "Network applied before durable intent"
                name = "first" if entry["id"] == first.public["id"] else "second"
                applied[name].set()
                if name == "second":
                    scheduled.set()

            broker = SimpleNamespace(lock=Lock(), journal=journal, active={}, failed=False, policy_call=lambda _: None,
                                     network=SimpleNamespace(boot="fixture", identity=lambda _: 1, apply=apply, cleanup=lambda _: None))
            owner = SimpleNamespace(vault_id=str(uuid.uuid4()), dead=lambda: True, send=lambda _: None)
            detail = {"addresses": [{"cidr": "10.72.0.2/32"}], "routes": ["10.72.0.0/24"], "remote": "198.18.8.1"}

            class Session(native.NativeSession):
                def initial(self):
                    return "fixture", {}

                def launch(self, _):
                    return Process({"kind": "disconnected"} if deleting and self is second else
                                   {"kind": "tunnel-ready", "detail": {**detail, "interface": self.interface}})

                def cleanup(self):
                    super().cleanup()
                    if self is second:
                        scheduled.set()

            first, second = [Session(broker, owner, str(uuid.uuid4()), 1, str(uuid.uuid4()), {"family": "openVpn"}) for _ in range(2)]
            for session in (first, second):
                session.reserved = True
                session.network_plan = native.plan(detail)
                broker.active[session.public["id"]] = session
            if deleting:
                second.entry = {"id": second.public["id"], "interface": second.interface, "plan": second.network_plan}
                journal.entries[second.public["id"]] = second.entry
                journal.save()
            replace = native.os.replace

            def delayed_replace(*args):
                if threading.current_thread() is first.thread and not paused.is_set():
                    paused.set()
                    assert resume.wait(5), "Serialized journal snapshot was not released"
                return replace(*args)

            try:
                with patch.object(native.os, "replace", delayed_replace):
                    first.thread.start()
                    assert paused.wait(5), "First journal snapshot was not serialized"
                    second.thread.start()
                    assert scheduled.wait(5), "Competing journal transaction was not scheduled"
                    resume.set()
                    assert applied["first"].wait(5), "First network intent did not apply"
                    if deleting:
                        second.thread.join(5)
                        assert not second.thread.is_alive(), "Competing cleanup did not finish"
                    else:
                        assert applied["second"].wait(5), "Second network intent did not apply"
                    expected = {first.public["id"]} | (set() if deleting else {second.public["id"]})
                    assert set(json.loads(journal.path.read_text())) == set(journal.entries) == expected, "Stale journal snapshot overwrote a concurrent transaction"
            finally:
                resume.set()
                finish.set()
                try:
                    for session in (first, second):
                        if session.thread.ident is not None:
                            session.thread.join(5)
                            assert not session.thread.is_alive(), "Journal fixture thread survived cleanup"
                    assert not journal.entries and json.loads(journal.path.read_text()) == {} and not broker.failed
                    if deleting:
                        def fail_cleanup(_):
                            raise ValueError("fixture_cleanup_failed")
                        failed = Session(broker, owner, str(uuid.uuid4()), 1, str(uuid.uuid4()), {"family": "openVpn"})
                        failed.entry = {"id": failed.public["id"]}
                        journal.entries[failed.public["id"]] = failed.entry
                        journal.save()
                        broker.network.cleanup = fail_cleanup
                        failed.launch = lambda _: Process({"kind": "disconnected"})
                        failed.run()
                        assert broker.failed and set(json.loads(journal.path.read_text())) == {failed.public["id"]}, "Failed cleanup discarded durable recovery intent"
                finally:
                    os.close(journal.lock)


def resolved_namespace(source, stage):
    native = load(source)
    assert native.lab.isolated() and os.stat("/proc/self/ns/mnt").st_ino != os.stat("/proc/1/ns/mnt").st_ino
    def command(*args):
        return sp.run(list(map(str, args)), check=True, capture_output=True, timeout=5)
    command("mount", "--make-rprivate", "/")
    original, private = stage / "etc-original", stage / "etc-private"
    original.mkdir()
    private.mkdir()
    command("mount", "--bind", "/etc", original)
    command("mount", "-o", "remount,bind,ro", original)
    for path in Path("/etc").iterdir():
        (private / path.name).symlink_to(original / path.name)
    (private / "resolv.conf").unlink()
    (private / "resolv.conf").symlink_to("/run/systemd/resolve/stub-resolv.conf")
    (private / "systemd").unlink()
    (private / "systemd").mkdir()
    (private / "systemd/resolved.conf").write_text("[Resolve]\nDNS=\nFallbackDNS=\nLLMNR=no\nMulticastDNS=no\nDNSSEC=no\nDNSOverTLS=no\nCache=no\nReadEtcHosts=no\n")
    command("mount", "--bind", private, "/etc")
    for directory in ("/run/systemd", "/run/dbus"):
        command("mount", "-t", "tmpfs", "-o", "mode=755,nosuid,nodev", "tmpfs", directory)
    Path("/run/systemd/resolve").mkdir()
    shutil.chown("/run/systemd/resolve", "systemd-resolve", "systemd-resolve")
    (stage / "dbus.conf").write_text('<busconfig><type>system</type><listen>unix:path=/run/dbus/system_bus_socket</listen><policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
    processes = []
    def stop(*_):
        raise KeyboardInterrupt()
    signal.signal(signal.SIGTERM, stop)
    try:
        for name, arguments in [("dbus", ["dbus-daemon", "--nofork", "--nopidfile", "--nosyslog", "--config-file=" + str(stage / "dbus.conf")]),
                                ("resolved", ["/usr/lib/systemd/systemd-resolved"])]:
            with (stage / (name + ".log")).open("wb") as log:
                processes.append(sp.Popen(arguments, stdout=log, stderr=log))
        deadline = time.monotonic() + 8
        while True:
            assert all(process.poll() is None for process in processes), "Private DNS daemon exited during startup"
            status = sp.run(["resolvectl", "status"], capture_output=True, timeout=2)
            if status.returncode == 0 and Path("/run/systemd/resolve/stub-resolv.conf").exists():
                break
            assert time.monotonic() < deadline, "Private resolved startup timed out"
            time.sleep(.05)
        assert Path("/etc/resolv.conf").resolve() == Path("/run/systemd/resolve/stub-resolv.conf")
        (stage / "resolved-ready.json").write_text(json.dumps({"mountNamespace": os.stat("/proc/self/ns/mnt").st_ino,
                "networkNamespace": os.stat("/proc/self/ns/net").st_ino, "version": command("resolvectl", "--version").stdout.decode().splitlines()[0]}))
        while True:
            assert all(process.poll() is None for process in processes), "Private DNS daemon exited"
            time.sleep(.1)
    except KeyboardInterrupt:
        pass
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(5)
                except sp.TimeoutExpired:
                    process.kill()
                    process.wait(5)


CLIENT = r'''
#include <sys/socket.h>
#include <sys/un.h>
#include <poll.h>
#include <unistd.h>
#include <string.h>
#include <stdlib.h>
int main(int argc,char **argv) {
 if(argc!=2)return 2;int s=socket(AF_UNIX,SOCK_STREAM,0);struct sockaddr_un a={.sun_family=AF_UNIX};
 if(strlen(argv[1])>=sizeof(a.sun_path))return 2;strcpy(a.sun_path,argv[1]);if(connect(s,(void*)&a,sizeof(a)))return 3;
 for(;;){struct pollfd f[2]={{0,POLLIN,0},{s,POLLIN,0}};if(poll(f,2,-1)<0)return 4;
  char b[8192];if(f[0].revents){ssize_t n=read(0,b,sizeof(b));if(n<=0)return 0;for(ssize_t p=0;p<n;){ssize_t w=write(s,b+p,n-p);if(w<=0)return 4;p+=w;}}
  if(f[1].revents){ssize_t n=read(s,b,sizeof(b));if(n<=0)return 0;for(ssize_t p=0;p<n;){ssize_t w=write(1,b+p,n-p);if(w<=0)return 4;p+=w;}}
 }
}'''


def exercise(source, args, root, client_ns, server_ns, profile, ns, results):
    credential_checks(source, args.policy)
    results.append({"test": "native-unix-remembered-credentials-otp-retry-and-concurrent-policy-boundaries", "status": "passed"})
    policy_checks(source)
    results.append({"test": "native-unix-bounded-protocol-and-network-trust-boundary", "status": "passed"})
    journal_checks(source)
    results.append({"test": "native-unix-journal-serializes-concurrent-intents-and-cleanup", "status": "passed"})
    diagnostic = {"test":"native-unix-sanitized-protocol-milestones","status":"diagnostic","events":[],"lastMilestone":{}}
    results.append(diagnostic)
    with tempfile.TemporaryDirectory(prefix="termterm-vpn-native-", dir="/run") as stage_name:
        stage = Path(stage_name)
        stage.chmod(0o755)
        engines = stage / "engines"
        engines.mkdir(mode=0o755)
        pins = {}
        for name, original in [("termterm-openconnect", args.engines / "termterm-openconnect"), ("termterm-openvpn", args.engines / "termterm-openvpn"),
                               ("vpn-policy-lab", args.policy), ("termterm-wireguard", source / ".tools/vpn-wireguard/termterm-wireguard-linux")]:
            shutil.copyfile(original, engines / name)
            (engines / name).chmod(0o755)
            pins[name] = hashlib.sha256((engines / name).read_bytes()).hexdigest()
        (stage / "client.c").write_text(CLIENT)
        sp.run(["cc", "-O2", "-Wall", "-Wextra", stage / "client.c", "-o", stage / "client"], check=True, capture_output=True)
        (stage / "client").chmod(0o755)
        shutil.copyfile(root / "ca.crt", stage / "ca.crt")
        (stage / "ca.crt").chmod(0o600)
        journal = stage / "journal"
        journal.mkdir(mode=0o700)
        config = {"uid": 65534, "client": str(stage / "client"), "clientSha256": hashlib.sha256((stage / "client").read_bytes()).hexdigest(),
                  "engines": str(engines), "pins": pins, "journal": str(journal), "caFile": str(stage / "ca.crt")}
        (stage / "config.json").write_text(json.dumps(config))
        (stage / "config.json").chmod(0o600)
        broker = sp.Popen(["ip", "netns", "exec", client_ns, "python3", __file__, "--serve", str(source), str(stage)], stdout=sp.PIPE, stderr=sp.PIPE)
        client = None
        dns_processes, dns_link, dns_underlay = [], False, False
        host_dns_before = None
        def host_dns_state():
            return (Path("/etc/resolv.conf").resolve(), Path("/etc/resolv.conf").read_bytes(),
                    sp.check_output(["systemctl", "show", "systemd-resolved", "-p", "MainPID", "-p", "NRestarts", "-p", "ActiveState"]),
                    sp.check_output(["resolvectl", "dns"]), sp.check_output(["resolvectl", "domain"]))
        ns(client_ns,"ip","link","add","ttvforeign0","type","dummy")
        ns(client_ns,"ip","link","set","ttvforeign0","up")
        ns(client_ns,"ip","route","add","203.0.113.0/24","dev","ttvforeign0")
        try:
            path = stage / "broker.sock"
            deadline = time.monotonic() + 8
            while not path.exists():
                assert broker.poll() is None, "Native broker startup failed: " + broker.stderr.read().decode()[-1000:]
                assert time.monotonic() < deadline, "Native broker socket timeout"
                time.sleep(.02)
            # Same UID is insufficient: the Python process is not the pinned fixture executable.
            denied_uid = sp.run(["ip","netns","exec",client_ns,"python3","-c","import socket,sys;s=socket.socket(socket.AF_UNIX);s.settimeout(3);s.connect(sys.argv[1]);assert s.recv(1)==b''",str(path)],capture_output=True,timeout=5)
            assert denied_uid.returncode == 0,"Unexpected UID accepted"
            denied = sp.run(["ip", "netns", "exec", client_ns, "setpriv", "--reuid", "65534", "--regid", "65534", "--clear-groups", "python3", "-c",
                             "import socket,sys;s=socket.socket(socket.AF_UNIX);s.settimeout(3);s.connect(sys.argv[1]);assert s.recv(1)==b''", str(path)], capture_output=True, timeout=5)
            assert denied.returncode == 0, "Unpinned same-user peer was not rejected"
            results.append({"test": "native-unix-rejects-wrong-uid-and-unpinned-same-uid-executable", "status": "passed"})
            client = sp.Popen(["ip", "netns", "exec", client_ns, "setpriv", "--reuid", "65534", "--regid", "65534", "--clear-groups", str(stage / "client"), str(path)], stdin=sp.PIPE, stdout=sp.PIPE, stderr=sp.PIPE, bufsize=0)
            counter, pending, states, receipts, dns_policies = 0, bytearray(), {}, {}, {}
            hold_ack = set()
            quiesce = set()
            def send(action, **fields):
                nonlocal counter
                counter += 1
                client.stdin.write((json.dumps({"action": action, "requestId": counter, **fields}) + "\n").encode())
                return counter
            def read(timeout=30):
                deadline = time.monotonic() + timeout
                while b"\n" not in pending:
                    assert select.select([client.stdout], [], [], max(0, deadline-time.monotonic()))[0], "Native broker event timeout"
                    part = os.read(client.stdout.fileno(), 8192)
                    assert part, "Native broker fixture closed unexpectedly"
                    pending.extend(part)
                end = pending.index(10)
                message = json.loads(pending[:end])
                del pending[:end+1]
                event = {"kind":message["kind"]}
                if message["kind"] == "session":
                    states[message["session"]["id"]] = message["session"]
                    event.update(id=message["session"]["id"],state=message["session"]["state"])
                    if message["session"].get("error"):
                        error=message["session"]["error"]
                        event["error"]=error if isinstance(error,str) and re.fullmatch(r"[a-z0-9_]{1,80}",error) else "bounded_error_redacted"
                if message["kind"] == "fixtureMilestone":
                    event.update(id=message["id"],milestone=message["milestone"])
                    diagnostic["lastMilestone"][message["id"]]=message["milestone"]
                if message["kind"] == "fixtureDnsPolicy":
                    dns_policies[message["id"]] = message["policy"]
                    event.update(policy=message["policy"])
                if len(diagnostic["events"])<200: diagnostic["events"].append(event)
                if message["kind"] == "networkLease":
                    receipts[message["id"]] = message["lease"]
                if message["kind"] == "quiesceRequired":
                    quiesce.add(message["id"])
                    if message["id"] not in hold_ack:
                        send("quiesced", id=message["id"], generation=message["generation"])
                return message
            def request(action, **fields):
                identity = send(action, **fields)
                while True:
                    message = read()
                    if message["kind"] == "reply" and message["requestId"] == identity:
                        return message
            def wait(identity, desired, timeout=30):
                deadline = time.monotonic()+timeout
                while time.monotonic() < deadline:
                    state = states.get(identity, {})
                    if state.get("state") in desired:
                        return state
                    if state.get("state") == "challenge":
                        challenge = state["challenge"]
                        assert request("answer", id=identity, generation=1, challengeId=challenge["id"], values={field["name"]: "labuser" if not field["secret"] else fixture_password for field in challenge["fields"]})["ok"]
                    else:
                        read(max(.01,deadline-time.monotonic()))
                raise AssertionError("Native service tunnel timed out")
            fixture_password = json.loads((root / "native-auth.json").read_text())["password"]
            hello = request("hello", vaultId=str(uuid.uuid4()))
            assert hello["ok"] and hello["result"]["version"] == 2 and hello["result"]["networkLease"] and not hello["result"]["labOnly"]
            baseline = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            identities = [str(uuid.uuid4()) for _ in range(3)]
            for index, identity in enumerate(identities):
                assert request("connect", id=identity, generation=1, profileId=str(uuid.uuid4()), profile={**profile(index), "remember": True, "password": fixture_password})["ok"]
            for index, identity in enumerate(identities):
                state = wait(identity, {"connected", "failed"})
                assert state["state"] == "connected", state.get("error", "Native tunnel failed")
                lease = receipts[identity]
                assert lease["id"] == identity and lease["generation"] == 1 and lease["interfaceIndex"] > 0 and lease["addressFamilies"] == ["ipv4"]
                assert not any(link.get("addr_info") for link in json.loads(ns(client_ns,"ip","-j","-6","addr","show","dev",lease["interfaceName"]).stdout)), "IPv4-only owned TUN has IPv6 addresses"
                ns(client_ns, "ping", "-c", "1", "-W", "2", f"10.88.0.{index+1}")
            assert set(json.loads((journal / "owned.json").read_text())) == set(identities)
            results.append({"test": "native-unix-authenticated-service-three-concurrent-real-openconnect-openvpn-tunnels-and-intents", "status": "passed"})
            before = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            assert not request("connect", id=str(uuid.uuid4()), generation=1, profileId=str(uuid.uuid4()), profile=profile(1))["ok"]
            assert not request("disconnect", id=identities[0], generation=2)["ok"]
            assert ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout == before
            middle = identities[1]
            hold_ack.add(middle)
            assert request("disconnect", id=middle, generation=1)["ok"]
            while middle not in quiesce:
                read()
            assert ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout == before, "Route changed before dependent-socket ACK"
            assert request("quiesced", id=middle, generation=1)["ok"]
            assert wait(middle, {"disconnected", "failed"})["state"] == "disconnected"
            for target in (1, 3):
                ns(client_ns, "ping", "-c", "1", "-W", "2", f"10.88.0.{target}")
            results.append({"test": "native-unix-quiesce-before-middle-cleanup-and-stale-generation", "status": "passed"})
            conflict = str(uuid.uuid4())
            assert request("connect", id=conflict, generation=1, profileId=str(uuid.uuid4()), profile=profile(2))["ok"]
            assert wait(conflict, {"failed"})["state"] == "failed"
            results.append({"test": "native-unix-route-conflict-refused-before-mutation", "status": "passed"})
            def keypair():
                private=sp.run(["wg","genkey"],check=True,capture_output=True).stdout.strip()
                public=sp.run(["wg","pubkey"],input=private+b"\n",check=True,capture_output=True).stdout.strip()
                return private,public
            server_private,server_public=keypair()
            client_private,client_public=keypair()
            ns(server_ns,"ip","link","add","ttnativewg","type","wireguard")
            server_config=b"[Interface]\nPrivateKey = "+server_private+b"\nListenPort = 31820\n[Peer]\nPublicKey = "+client_public+b"\nAllowedIPs = 10.97.0.2/32\n"
            sp.run(["ip","netns","exec",server_ns,"wg","setconf","ttnativewg","/dev/stdin"],input=server_config,check=True,capture_output=True)
            ns(server_ns,"ip","addr","add","10.97.0.1/32","dev","ttnativewg")
            ns(server_ns,"ip","link","set","ttnativewg","up")
            ns(server_ns,"ip","route","add","10.97.0.2/32","dev","ttnativewg")
            wg_profile={**profile(1),"family":"wireGuard","label":"Disposable native WireGuard","config":"[Interface]\nPrivateKey = "+client_private.decode()+"\nAddress = 10.97.0.2/32\n[Peer]\nPublicKey = "+server_public.decode()+"\nEndpoint = 198.18.8.1:31820\nAllowedIPs = 10.97.0.0/24\nPersistentKeepalive = 1\n"}
            wg_id=str(uuid.uuid4())
            assert request("connect",id=wg_id,generation=1,profileId=str(uuid.uuid4()),profile=wg_profile)["ok"]
            assert wait(wg_id,{"connected","failed"},timeout=36)["state"]=="connected", "Native WireGuard terminal failure; see sanitized milestones"
            assert receipts[wg_id]["addressFamilies"]==["ipv4"]
            ns(client_ns,"ping","-c","2","-W","2","10.97.0.1")
            ns(client_ns,"ping","-c","1","-W","2","10.88.0.1")
            ns(client_ns,"ping","-c","1","-W","2","10.88.0.3")
            results.append({"test":"native-unix-real-wireguard-kernel-peer-through-owned-tun-pump-parallel-with-openconnect-openvpn","status":"passed"})
            assert request("disconnect",id=wg_id,generation=1)["ok"]
            assert wait(wg_id,{"disconnected","failed"})["state"]=="disconnected"
            client.kill()
            client.wait(5)
            deadline = time.monotonic()+12
            while ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout != baseline or json.loads((journal / "owned.json").read_text()) != {}:
                assert time.monotonic() < deadline, "Owned native routes survived actual client process death"
                time.sleep(.05)
            assert json.loads((journal / "owned.json").read_text()) == {}
            results.append({"test": "native-unix-client-process-death-owned-journal-cleanup", "status": "passed"})
            client = sp.Popen(["ip", "netns", "exec", client_ns, "setpriv", "--reuid", "65534", "--regid", "65534", "--clear-groups", str(stage / "client"), str(path)], stdin=sp.PIPE, stdout=sp.PIPE, stderr=sp.PIPE, bufsize=0)
            pending.clear()
            assert request("hello", vaultId=str(uuid.uuid4()))["ok"]
            crashes = [str(uuid.uuid4()) for _ in range(3)]
            for index, crash in enumerate(crashes):
                assert request("connect", id=crash, generation=1, profileId=str(uuid.uuid4()), profile={**profile(index), "remember": True, "password": fixture_password})["ok"]
            for crash in crashes:
                assert wait(crash, {"connected", "failed"})["state"] == "connected"
            assert set(json.loads((journal / "owned.json").read_text())) == set(crashes)
            broker.kill()
            broker.wait(5)
            client.wait(5)
            assert set(json.loads((journal / "owned.json").read_text())) == set(crashes)
            deadline = time.monotonic()+5
            while ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout != baseline:
                assert time.monotonic() < deadline, "Broker death left native TUN routes"
                time.sleep(.05)
            broker = sp.Popen(["ip", "netns", "exec", client_ns, "python3", __file__, "--serve", str(source), str(stage)], stdout=sp.PIPE, stderr=sp.PIPE)
            deadline = time.monotonic()+8
            while json.loads((journal / "owned.json").read_text()) != {}:
                assert broker.poll() is None and time.monotonic() < deadline, "Owned crash journal recovery failed"
                time.sleep(.05)
            assert ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout == baseline
            results.append({"test": "native-unix-broker-death-capability-and-owned-journal-restart", "status": "passed"})
            ns(client_ns,"ip","link","show","ttvforeign0")
            results.append({"test":"native-unix-cleanup-preserves-foreign-vpn-like-interface-and-route","status":"passed"})
            full_before = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            full_restore, full_servers = [], []
            try:
                for operation, undo in [
                        ((client_ns, "ip", "route", "del", "203.0.113.0/24", "dev", "ttvforeign0"), (client_ns, "ip", "route", "add", "203.0.113.0/24", "dev", "ttvforeign0")),
                        ((client_ns, "ip", "addr", "add", "10.0.2.15/24", "dev", "ttvc"), (client_ns, "ip", "addr", "del", "10.0.2.15/24", "dev", "ttvc")),
                        ((client_ns, "ip", "route", "add", "default", "via", "198.18.8.1", "dev", "ttvc"), (client_ns, "ip", "route", "del", "default", "via", "198.18.8.1", "dev", "ttvc")),
                        ((server_ns, "ip", "addr", "add", "198.18.9.1/32", "dev", "lo"), (server_ns, "ip", "addr", "del", "198.18.9.1/32", "dev", "lo"))]:
                    ns(*operation); full_restore.append(undo)
                (stage / "full.ext").write_text("basicConstraints=critical,CA:false\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:198.18.9.1\n")
                from datetime import datetime, timedelta, timezone
                now = datetime.now(timezone.utc)
                sp.run(["openssl", "ca", "-batch", "-notext", "-config", root / "ca.conf", "-in", root / "server.csr", "-startdate", (now-timedelta(minutes=5)).strftime("%Y%m%d%H%M%SZ"), "-enddate", (now+timedelta(days=1)).strftime("%Y%m%d%H%M%SZ"), "-extfile", stage / "full.ext", "-out", stage / "full.crt"], check=True, capture_output=True)
                full_oc = (root / "ocserv.conf").read_text().replace("198.18.8.1", "198.18.9.1").replace("4443", "4448").replace("10.89.0.0", "10.99.0.0").replace("ocserv.sock", "full-ocserv.sock").replace("device = ocvpn", "device = fullvpn").replace(str(root / "server.crt"), str(stage / "full.crt")).replace("route = 10.88.0.1/255.255.255.255", "route = default")
                full_ov = (root / "openvpn.conf").read_text().replace("198.18.8.1", "198.18.9.1").replace("1194", "1197").replace("10.90.0.0", "10.98.0.0").replace(str(root / "server.crt"), str(stage / "full.crt")).replace('push "route 10.88.0.1 255.255.255.255"', 'push "redirect-gateway def1"')
                for name, content, arguments in [
                        ("full-oc", full_oc, [str(args.ocserv_root / "usr/sbin/ocserv"), "-f", "-c"]),
                        ("full-ov", full_ov, ["openvpn", "--config"])]:
                    configuration = stage / (name + ".conf"); configuration.write_text(content)
                    with (stage / (name + ".log")).open("wb") as log:
                        process = sp.Popen(["ip", "netns", "exec", server_ns, *arguments, str(configuration)], stdout=log, stderr=log)
                        full_servers.append(process); dns_processes.append(process)
                deadline = time.monotonic()+10
                while not all(port in ns(server_ns, "ss", "-ltn").stdout for port in (b":4448", b":1197")):
                    assert all(p.poll() is None for p in full_servers) and time.monotonic() < deadline, "Full-tunnel servers failed"
                    time.sleep(.05)
                full_baseline = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
                for protocol, target, full_profile in [
                        ("OpenConnect", "10.88.0.1", {**profile(0), "server": profile(0)["server"].replace("198.18.8.1", "198.18.9.1").replace("4443", "4448")}),
                        ("OpenVPN", "10.88.0.1", {**profile(1), "config": profile(1)["config"].replace("198.18.8.1", "198.18.9.1").replace("1194", "1197")}),
                        ("WireGuard", "10.97.0.1", {**wg_profile, "config": wg_profile["config"].replace("198.18.8.1", "198.18.9.1").replace("AllowedIPs = 10.97.0.0/24", "AllowedIPs = 0.0.0.0/0")})]:
                    for mode in ("disconnect", "client-death", "broker-death"):
                        client = sp.Popen(["ip", "netns", "exec", client_ns, "setpriv", "--reuid", "65534", "--regid", "65534", "--clear-groups", str(stage / "client"), str(path)], stdin=sp.PIPE, stdout=sp.PIPE, stderr=sp.PIPE, bufsize=0)
                        pending.clear(); assert request("hello", vaultId=str(uuid.uuid4()))["ok"]
                        identity = str(uuid.uuid4())
                        assert request("connect", id=identity, generation=1, profileId=str(uuid.uuid4()), profile={**full_profile, "remember": True, "password": fixture_password})["ok"]
                        assert wait(identity, {"connected", "failed"}, timeout=36)["state"] == "connected", "Full-tunnel failed; see sanitized milestones"
                        interface = receipts[identity]["interfaceName"]
                        owned = json.loads((journal / "owned.json").read_text())[identity]
                        assert owned["plan"]["fullTunnel"] is True
                        for destination, device in [("8.8.8.8", interface), ("198.18.9.1", "ttvc"), ("10.0.2.99", "ttvc")]:
                            assert json.loads(ns(client_ns, "ip", "-j", "route", "get", destination).stdout)[0]["dev"] == device
                        ns(client_ns, "ping", "-c", "1", "-W", "2", target)
                        connected_routes = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
                        competing = str(uuid.uuid4())
                        assert request("connect", id=competing, generation=1, profileId=str(uuid.uuid4()), profile=full_profile)["ok"]
                        assert wait(competing, {"failed"}, timeout=36)["state"] == "failed"
                        assert ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout == connected_routes, "Competing full tunnel changed network"
                        if mode == "disconnect":
                            assert request("disconnect", id=identity, generation=1)["ok"]
                            assert wait(identity, {"disconnected", "failed"})["state"] == "disconnected"
                            client.kill(); client.wait(5)
                        elif mode == "client-death":
                            client.kill(); client.wait(5)
                        else:
                            broker.kill(); broker.wait(5); client.wait(5)
                        deadline = time.monotonic()+12
                        while ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout != full_baseline:
                            assert time.monotonic() < deadline, "Full-tunnel cleanup left routes"
                            time.sleep(.05)
                        if mode == "broker-death":
                            broker = sp.Popen(["ip", "netns", "exec", client_ns, "python3", __file__, "--serve", str(source), str(stage)], stdout=sp.PIPE, stderr=sp.PIPE)
                        deadline = time.monotonic()+8
                        while json.loads((journal / "owned.json").read_text()) != {}:
                            assert broker.poll() is None and time.monotonic() < deadline, "Full-tunnel recovery journal remained"
                            time.sleep(.05)
                        results.append({"test": "native-unix-real-full-tunnel-" + protocol.lower() + "-" + mode, "status": "passed", "kernelConnectedUnderlayPreserved": True, "transportEndpointPreserved": True, "competingFullTunnelRejected": True})
            finally:
                for process in full_servers:
                    if process.poll() is None:
                        process.terminate(); process.wait(5)
                for undo in reversed(full_restore):
                    ns(*undo)
            assert ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout == full_before, "Full-tunnel fixture network not restored"
            broker.terminate()
            broker.wait(5)
            host_dns_before = host_dns_state()
            resolved = sp.Popen(["ip", "netns", "exec", client_ns, "unshare", "--mount", "--propagation", "private", "python3", __file__, "--resolved", str(source), str(stage)], stdout=sp.PIPE, stderr=sp.PIPE)
            dns_processes.append(resolved)
            deadline = time.monotonic() + 12
            while not (stage / "resolved-ready.json").exists():
                assert resolved.poll() is None, "Private resolved fixture failed: " + resolved.stderr.read().decode()[-1000:]
                assert time.monotonic() < deadline, "Private resolved fixture startup timed out"
                time.sleep(.05)
            isolation = json.loads((stage / "resolved-ready.json").read_text())
            assert isolation["mountNamespace"] != os.stat("/proc/self/ns/mnt").st_ino and isolation["networkNamespace"] != os.stat("/proc/self/ns/net").st_ino
            def private_run(*arguments, check=True):
                return sp.run(["nsenter", "--target", str(resolved.pid), "--mount", "--net", "--", *map(str, arguments)], check=check, capture_output=True, timeout=6)
            def start_dns_broker():
                process = sp.Popen(["nsenter", "--target", str(resolved.pid), "--mount", "--net", "--", "python3", __file__, "--serve", str(source), str(stage)], stdout=sp.PIPE, stderr=sp.PIPE)
                dns_processes.append(process)
                deadline = time.monotonic() + 8
                while True:
                    assert process.poll() is None, "Private DNS broker startup failed: " + process.stderr.read().decode()[-1000:]
                    if (journal / "owned.json").exists() and json.loads((journal / "owned.json").read_text()) == {} and path.exists():
                        # A successful authenticated handshake below confirms socket readiness.
                        with socket.socket(socket.AF_UNIX) as probe:
                            if probe.connect_ex(str(path)) == 0:
                                return process
                    assert time.monotonic() < deadline, "Private DNS broker startup timed out"
                    time.sleep(.05)
            ns(client_ns, "ip", "link", "add", "ttdnsbase", "type", "dummy")
            dns_link = True
            ns(client_ns, "ip", "link", "set", "ttdnsbase", "up")
            private_run("resolvectl", "dns", "ttdnsbase", "192.0.2.53")
            private_run("resolvectl", "domain", "ttdnsbase", "~baseline.invalid")
            private_run("resolvectl", "default-route", "ttdnsbase", "no")
            def dns_state():
                return tuple(private_run("resolvectl", action).stdout for action in ("dns", "domain", "default-route"))
            dns_before = dns_state()
            ns(client_ns, "ip", "addr", "add", "10.0.2.15/24", "dev", "ttvc")
            dns_underlay = True
            dns_routes = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            broker = start_dns_broker()
            for protocol, address, port in (("OpenConnect/AnyConnect", "10.88.0.1", "4447"), ("OpenVPN", "10.88.0.2", "1196")):
                domain = uuid.uuid4().hex[:16] + ".termterm.test"
                fqdn = "probe." + domain
                receipt = stage / ("dns-" + port + "-receipt.json")
                server_config = stage / ("dns-" + port + ".conf")
                if protocol == "OpenVPN":
                    server_config.write_text((root / "ov2.conf").read_text().replace("1194", port).replace("10.90.0.0", "10.92.0.0") + '\npush "dhcp-option DNS ' + address + '"\npush "dhcp-option ADAPTER_DOMAIN_SUFFIX ' + domain + '"\n')
                    server_arguments = ["openvpn", "--config", str(server_config)]
                    tunnel_profile = {**profile(1), "config": profile(1)["config"].replace("1194", port)}
                else:
                    server_config.write_text((root / "ocserv.conf").read_text().replace("4443", port).replace("10.89.0.0", "10.95.0.0").replace("ocserv.sock", "dns-ocserv.sock").replace("device = ocvpn", "device = dnsvpn").replace("route = 10.88.0.1/255.255.255.255", "route = default\nno-route = 198.18.8.0/255.255.255.252\nno-route = 203.0.113.0/255.255.255.0") + '\ndns = ' + address + '\ndefault-domain = ' + domain + '\nsplit-dns = ' + domain + '\n')
                    server_arguments = [str(args.ocserv_root / "usr/sbin/ocserv"), "-f", "-c", str(server_config)]
                    tunnel_profile = {**profile(0), "server": profile(0)["server"].replace("4443", port)}
                servers = []
                for name, arguments in [("server", server_arguments),
                                        ("authority", ["python3", "-c", "import runpy,sys;from pathlib import Path;sys.path.insert(0,str(Path(sys.argv[1]).parent));runpy.run_path(sys.argv[1])['serve'](sys.argv[2],sys.argv[3],sys.argv[2],Path(sys.argv[4]))", str(source / "scripts/vpn-dns-fixture.py"), address, fqdn, str(receipt)])]:
                    with (stage / ("dns-" + port + "-" + name + ".log")).open("wb") as log:
                        process = sp.Popen(["ip", "netns", "exec", server_ns, *arguments], stdout=log, stderr=log)
                        servers.append(process)
                        dns_processes.append(process)
                deadline = time.monotonic() + 8
                while not receipt.exists() or (":" + port).encode() not in ns(server_ns, "ss", "-ltn").stdout:
                    assert all(process.poll() is None for process in servers) and time.monotonic() < deadline, "Private DNS servers did not start"
                    time.sleep(.05)
                app_query = ["setpriv", "--reuid", "65534", "--regid", "65534", "--clear-groups", "python3", "-c",
                             "import socket,sys;assert {row[4][0] for row in socket.getaddrinfo(sys.argv[1],None,socket.AF_INET)}=={sys.argv[2]}", fqdn, address]
                no_dns_query = [*app_query[:-3], "import socket,sys\ntry: socket.getaddrinfo(sys.argv[1],None,socket.AF_INET)\nexcept socket.gaierror: pass\nelse: raise AssertionError('Tunnel-only DNS resolved after configuration removal')", fqdn]
                for mode in ("disconnect", "client-death", "broker-death"):
                    for name in (fqdn, "probe"):
                        private_run(*no_dns_query[:-1], name)
                    client = sp.Popen(["ip", "netns", "exec", client_ns, "setpriv", "--reuid", "65534", "--regid", "65534", "--clear-groups", str(stage / "client"), str(path)], stdin=sp.PIPE, stdout=sp.PIPE, stderr=sp.PIPE, bufsize=0)
                    pending.clear()
                    assert request("hello", vaultId=str(uuid.uuid4()))["ok"]
                    identity = str(uuid.uuid4())
                    assert request("connect", id=identity, generation=1, profileId=str(uuid.uuid4()), profile=tunnel_profile)["ok"]
                    assert wait(identity, {"connected", "failed"})["state"] == "connected", "Native DNS tunnel failed; see sanitized protocol milestones"
                    interface = receipts[identity]["interfaceName"]
                    if protocol != "OpenVPN":
                        assert json.loads((journal / "owned.json").read_text())[identity]["plan"]["fullTunnel"] is True
                        for destination, expected in [("8.8.8.8", interface), ("198.18.8.1", "ttvc"), ("10.0.2.99", "ttvc")]:
                            assert json.loads(ns(client_ns, "ip", "-j", "route", "get", destination).stdout)[0]["dev"] == expected
                    assert address.encode() in private_run("resolvectl", "dns", interface).stdout
                    assert private_run("resolvectl", "domain", interface).stdout.decode().split(":", 1)[1].split() == [domain]
                    assert b"no" in private_run("resolvectl", "default-route", interface).stdout
                    assert json.loads(ns(client_ns, "ip", "-j", "route", "get", address).stdout)[0]["dev"] == interface
                    dns_probes = {}
                    for name in (fqdn, "probe"):
                        before_query = json.loads(receipt.read_text())
                        private_run(*app_query[:-2], name, address)
                        after_query = json.loads(receipt.read_text())
                        assert after_query["udpAnswers"] > before_query["udpAnswers"] and after_query["expectedFqdnQueries"] > before_query["expectedFqdnQueries"], "Application resolution did not reach authoritative DNS through tunnel"
                        dns_probes["shortName" if name == "probe" else "fqdn"] = {"udpAnswersBefore": before_query["udpAnswers"], "udpAnswersAfter": after_query["udpAnswers"]}
                    if mode == "disconnect":
                        assert request("disconnect", id=identity, generation=1)["ok"]
                        assert wait(identity, {"disconnected", "failed"})["state"] == "disconnected"
                        client.kill()
                        client.wait(5)
                    elif mode == "client-death":
                        client.kill()
                        client.wait(5)
                    else:
                        broker.kill()
                        broker.wait(5)
                        client.wait(5)
                        assert set(json.loads((journal / "owned.json").read_text())) == {identity}, "Broker death lost DNS recovery intent"
                    deadline = time.monotonic() + 12
                    while dns_state() != dns_before or ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout != dns_routes:
                        assert time.monotonic() < deadline, "Native DNS teardown failed to restore unrelated DNS and route state"
                        time.sleep(.05)
                    if mode == "broker-death":
                        broker = start_dns_broker()
                    deadline = time.monotonic() + 8
                    while json.loads((journal / "owned.json").read_text()) != {}:
                        assert time.monotonic() < deadline, "Native DNS owned journal cleanup timed out"
                        time.sleep(.05)
                    answers_before = json.loads(receipt.read_text())["udpAnswers"]
                    for name in (fqdn, "probe"):
                        private_run(*no_dns_query[:-1], name)
                    assert json.loads(receipt.read_text())["udpAnswers"] == answers_before, "Removed tunnel still reached authoritative DNS"
                    assert interface not in {link["ifname"] for link in json.loads(ns(client_ns, "ip", "-j", "link").stdout)}
                    assert b"192.0.2.53" in private_run("resolvectl", "dns", "ttdnsbase").stdout
                    results.append({"test": "native-unix-real-resolved-" + ("openvpn-" if protocol == "OpenVPN" else "") + "dns-" + mode + "-restores-baseline", "status": "passed", "protocol": protocol, "fullTunnel": protocol != "OpenVPN", "dnsPolicy": "legacy DNS + ADAPTER_DOMAIN_SUFFIX" if protocol == "OpenVPN" else "DNS + default-domain + split-dns", "applicationResolver": "nonroot Python socket.getaddrinfo", "authoritativeQueryObserved": True, "dnsProbes": dns_probes, "fqdnAndShortNameFailBeforeAndAfterTunnel": True, "foreignDnsPreserved": True})
                if protocol != "OpenVPN":
                    rejected_config = stage / "full-dns-collision.conf"
                    rejected_config.write_text(server_config.read_text().replace(port, "4449").replace("dns-ocserv.sock", "collision-ocserv.sock").replace("device = dnsvpn", "device = collvpn").replace("10.95.0.0", "10.94.0.0").replace("dns = " + address, "dns = 10.0.2.53"))
                    with (stage / "full-dns-collision.log").open("wb") as log:
                        rejected_server = sp.Popen(["ip", "netns", "exec", server_ns, str(args.ocserv_root / "usr/sbin/ocserv"), "-f", "-c", str(rejected_config)], stdout=log, stderr=log)
                        dns_processes.append(rejected_server)
                    deadline = time.monotonic()+8
                    while b":4449" not in ns(server_ns, "ss", "-ltn").stdout:
                        assert rejected_server.poll() is None and time.monotonic() < deadline, "Full DNS collision fixture failed"
                        time.sleep(.05)
                    client = sp.Popen(["ip", "netns", "exec", client_ns, "setpriv", "--reuid", "65534", "--regid", "65534", "--clear-groups", str(stage / "client"), str(path)], stdin=sp.PIPE, stdout=sp.PIPE, stderr=sp.PIPE, bufsize=0)
                    pending.clear(); assert request("hello", vaultId=str(uuid.uuid4()))["ok"]
                    identity = str(uuid.uuid4())
                    assert request("connect", id=identity, generation=1, profileId=str(uuid.uuid4()), profile={**tunnel_profile, "server": tunnel_profile["server"].replace(port, "4449")})["ok"]
                    assert wait(identity, {"failed"}).get("error") == "dns_conflicts_with_physical_underlay"
                    assert dns_state() == dns_before and ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout == dns_routes and json.loads((journal / "owned.json").read_text()) == {}
                    client.kill(); client.wait(5)
                    results.append({"test": "native-unix-real-full-tunnel-dns-underlay-collision-rejected-before-mutation", "status": "passed", "expectedError": "dns_conflicts_with_physical_underlay", "foreignDnsPreserved": True})
                if protocol == "OpenVPN":
                    servers[0].terminate()
                    servers[0].wait(5)
                    server_config.write_text(server_config.read_text().replace("ADAPTER_DOMAIN_SUFFIX", "DOMAIN"))
                    with (stage / "dns-openvpn-domain.log").open("wb") as log:
                        rejected_server = sp.Popen(["ip", "netns", "exec", server_ns, *server_arguments], stdout=log, stderr=log)
                        dns_processes.append(rejected_server)
                    deadline = time.monotonic() + 8
                    while (":" + port).encode() not in ns(server_ns, "ss", "-ltn").stdout:
                        assert rejected_server.poll() is None and time.monotonic() < deadline, "Split-domain negative fixture did not start"
                        time.sleep(.05)
                    client = sp.Popen(["ip", "netns", "exec", client_ns, "setpriv", "--reuid", "65534", "--regid", "65534", "--clear-groups", str(stage / "client"), str(path)], stdin=sp.PIPE, stdout=sp.PIPE, stderr=sp.PIPE, bufsize=0)
                    pending.clear()
                    assert request("hello", vaultId=str(uuid.uuid4()))["ok"]
                    identity = str(uuid.uuid4())
                    assert request("connect", id=identity, generation=1, profileId=str(uuid.uuid4()), profile=tunnel_profile)["ok"]
                    assert wait(identity, {"failed"}).get("error") == "advanced_dns_policy_unsupported"
                    assert dns_policies[identity] == {"fromDhcpOptions": True, "serverCount": 1, "splitDomainCount": 1, "searchDomainCount": 0}
                    assert dns_state() == dns_before and ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout == dns_routes and json.loads((journal / "owned.json").read_text()) == {}
                    results.append({"test": "native-unix-real-openvpn-domain-policy-rejected-before-mutation", "status": "passed", "expectedError": "advanced_dns_policy_unsupported", "policy": dns_policies[identity]})
            results.append({"test": "native-unix-real-resolved-private-mount-network-dbus", "status": "passed", "version": isolation["version"], "hostServiceUsed": False})
        finally:
            if client and client.poll() is None:
                client.kill(); client.wait(5)
            for process in [broker, *reversed(dns_processes)]:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(5)
                    except sp.TimeoutExpired:
                        process.kill(); process.wait(5)
            if dns_link:
                ns(client_ns, "ip", "link", "delete", "ttdnsbase")
            if dns_underlay:
                ns(client_ns, "ip", "addr", "del", "10.0.2.15/24", "dev", "ttvc")
            if host_dns_before is not None:
                assert host_dns_state() == host_dns_before, "Host DNS configuration or resolved service changed"
                results.append({"test": "native-unix-real-resolved-host-dns-and-service-unchanged", "status": "passed"})
            ns(client_ns,"ip","link","delete","ttvforeign0")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--credentials":
        credential_checks(Path(__file__).resolve().parents[1], Path(sys.argv[2]))
        print("PASS: remembered authentication uses shared policy; OTP/retry/metadata boundaries preserved")
        raise SystemExit()
    assert len(sys.argv) == 4 and sys.argv[1] in ("--serve", "--resolved") and os.geteuid() == 0
    source, stage = Path(sys.argv[2]), Path(sys.argv[3])
    if sys.argv[1] == "--resolved":
        resolved_namespace(source, stage)
        raise SystemExit()
    native = load(source)
    assert native.lab.isolated(), "Service fixture must stay in an isolated network namespace"
    class TracedSession(native.NativeSession):
        def milestone(self,name):
            self.owner.send({"kind":"fixtureMilestone","id":self.public["id"],"milestone":name})
        def initial(self):
            self.milestone("initial")
            result=super().initial();self.milestone("initial_ready");return result
        def launch(self,engine):
            result=super().launch(engine);self.milestone("launched");return result
        def reserve(self,detail):
            self.milestone("reserve")
            if self.profile["family"] == "openVpn" and detail.get("dnsOptions"):
                policy = detail["dnsOptions"]
                self.owner.send({"kind":"fixtureDnsPolicy","id":self.public["id"],"policy":{
                    "fromDhcpOptions":policy.get("from_dhcp_options"),"serverCount":len(policy.get("servers",{})),
                    "splitDomainCount":len(policy.get("servers",{}).get("0",{}).get("domains",[])),"searchDomainCount":len(policy.get("search_domains",[]))}})
            result=super().reserve(detail);self.milestone("reserved");return result
        def configure(self,detail):
            self.milestone("configure")
            result=super().configure(detail);self.milestone("configured");return result
        def emit(self,state,**extra):
            if state in ("connected","failed","disconnected"):self.milestone(state)
            return super().emit(state,**extra)
    class TracedBroker(native.NativeBroker):
        def __init__(self,config):
            super().__init__(config)
            self.session_type=TracedSession
    native.NativeBroker=TracedBroker
    native.SOCKET = str(stage / "broker.sock")
    native.serve(json.loads((stage / "config.json").read_text()))
