#!/usr/bin/env python3
"""Real WireGuard interoperability in one disposable Linux netns; never alters host routes/DNS."""
import argparse
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import secrets
import selectors
import socket
import struct
import subprocess as sp
import threading
import time
from vpn_lab_json import read_json, retry_io, write_json


def run(*args, **kwargs):
    return sp.run(list(map(str, args)), check=True, capture_output=True, **kwargs)


def snapshot():
    return {"routes4": run("ip", "-j", "route", "show", "table", "all").stdout.decode(),
            "routes6": run("ip", "-j", "-6", "route", "show", "table", "all").stdout.decode(),
            "dnsSha256": hashlib.sha256(Path("/etc/resolv.conf").read_bytes()).hexdigest()}


def linux_incarnation():
    return {"bootId": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
            "pid1StartTicks": Path("/proc/1/stat").read_text().split()[21]}


def persist_baseline(directory, fixture_id, network):
    value = {"version": 1, "fixtureId": fixture_id, "incarnation": linux_incarnation(), "network": network,
             "owner": {"pid": os.getpid(), "startTicks": Path("/proc/self/stat").read_text().split()[21],
                       "commandLineSha256": hashlib.sha256(Path("/proc/self/cmdline").read_bytes()).hexdigest()}}
    path = directory / "linux-baseline-before.json"
    write_json(path, value, exclusive=True)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def compare_baseline(path, fixture_id, expected_digest, incarnation, network):
    data = retry_io(path.read_bytes, reading=True)
    if hashlib.sha256(data).hexdigest() != expected_digest:
        raise ValueError("linux_baseline_digest_changed")
    baseline = json.loads(data)
    if baseline["fixtureId"] != fixture_id:
        raise ValueError("linux_baseline_fixture_changed")
    if baseline["incarnation"] != incarnation:
        return None
    return baseline["network"] == network


def keypair():
    private = run("wg", "genkey").stdout.strip()
    public = run("wg", "pubkey", input=private + b"\n").stdout.strip()
    import base64
    return private, public, base64.b64decode(private).hex(), base64.b64decode(public).hex()


def checksum(data):
    if len(data) % 2:
        data += b"\0"
    value = sum(struct.unpack("!" + "H" * (len(data) // 2), data))
    while value >> 16:
        value = (value & 65535) + (value >> 16)
    return ~value & 65535


def packet(payload):
    udp = struct.pack("!HHHH", 31824, 31823, len(payload) + 8, 0) + payload
    header = struct.pack("!BBHHHBBH4s4s", 0x45, 0, 20 + len(udp), 1, 0, 64, 17, 0,
                         ipaddress.ip_address("10.97.0.2").packed, ipaddress.ip_address("10.97.0.1").packed)
    return header[:10] + struct.pack("!H", checksum(header)) + header[12:] + udp


def private_json(path, value):
    write_json(path, value)


def tcp_receiver(receipt):
    assert os.readlink("/proc/self/ns/net") != os.readlink("/proc/1/ns/net")
    counters = {"connections": 0, "bytes": 0, "lastSha256": ""}
    private_json(receipt, counters)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server:
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind(("10.97.0.1", 31823))
        server.listen(4)
        while True:
            connection, _ = server.accept()
            with connection:
                connection.settimeout(5)
                digest, size = hashlib.sha256(), 0
                while True:
                    data = connection.recv(65536)
                    if not data:
                        break
                    size += len(data)
                    assert size <= 1024 * 1024, "Disposable payload exceeds 1 MiB"
                    digest.update(data)
                connection.sendall(digest.digest())
                counters["connections"] += 1
                counters["bytes"] += size
                counters["lastSha256"] = digest.hexdigest()
                private_json(receipt, counters)


def windows_fixture(directory, duration, report):
    source = Path(__file__).resolve().parents[1]
    directory = directory.resolve()
    assert directory.is_relative_to(source / ".lab") and not directory.exists(), "Use a new ignored .lab directory"
    assert os.geteuid() == 0 and 60 <= duration <= 14400
    before = snapshot()
    assert not run("ss", "-Hlun", "sport = :31820").stdout.strip(), "Preserve the existing UDP listener"
    directory.mkdir(parents=True, mode=0o700)
    identity = secrets.token_hex(8)
    namespace, birth_interface = "ttwg-win-" + identity[:10], "ttwg" + identity[:10]
    source_files = ["scripts/test-wireguard-lab.py", "scripts/vpn-dns-fixture.py", "scripts/wireguard-windows-relay.py", "scripts/vpn_lab_json.py"]
    source_hashes = {name: hashlib.sha256((source / name).read_bytes()).hexdigest() for name in source_files}
    baseline_digest = persist_baseline(directory, identity, before)
    processes, namespace_created, interface_created, moved = [], False, False, False
    try:
        addresses = json.loads(run("ip", "-4", "-j", "address", "show", "dev", "eth0").stdout)[0]["addr_info"]
        address = next(value["local"] for value in addresses if value["scope"] == "global")
        peer_address = ipaddress.ip_address(address)
        assert any(peer_address in ipaddress.ip_network(prefix) for prefix in ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"])
        private, _, _, public_hex = keypair()
        client_private, client_public, _, _ = keypair()
        import base64
        server_public = base64.b64encode(bytes.fromhex(public_hex)).decode()
        wrong_public = keypair()[1].decode()
        run("ip", "netns", "add", namespace)
        namespace_created = True
        # WireGuard's UDP socket remains in its birth namespace; cleartext never does.
        run("ip", "link", "add", birth_interface, "type", "wireguard")
        interface_created = True
        run("ip", "link", "set", birth_interface, "netns", namespace)
        moved = True
        run("ip", "-n", namespace, "link", "set", birth_interface, "name", "ttwg0")
        configuration = b"[Interface]\nPrivateKey = " + private + b"\nListenPort = 31820\n[Peer]\nPublicKey = " + client_public + b"\nAllowedIPs = 10.97.0.2/32\n"
        run("ip", "netns", "exec", namespace, "wg", "setconf", "ttwg0", "/dev/stdin", input=configuration)
        run("ip", "-n", namespace, "link", "set", "lo", "up")
        for cidr in ["10.97.0.1/32", "10.88.0.53/32"]:
            run("ip", "-n", namespace, "address", "add", cidr, "dev", "ttwg0")
        run("ip", "-n", namespace, "link", "set", "ttwg0", "up")
        run("ip", "-n", namespace, "route", "add", "10.97.0.2/32", "dev", "ttwg0")
        suffix = identity + ".termterm.test"
        for command in [["python3", source / "scripts/vpn-dns-fixture.py", "--name", "probe." + suffix, "--receipt", directory / "dns-receipt.json"],
                        ["python3", Path(__file__).resolve(), "--tcp-receiver", directory / "tcp-receipt.json"]]:
            processes.append(sp.Popen(list(map(str, ["ip", "netns", "exec", namespace, *command])), stdout=sp.DEVNULL, stderr=sp.DEVNULL))
        deadline = time.monotonic() + 5
        while not all((directory / name).exists() for name in ["dns-receipt.json", "tcp-receipt.json"]):
            assert all(process.poll() is None for process in processes) and time.monotonic() < deadline, "Private tunnel services did not start"
            time.sleep(.05)
        config = f"[Interface]\nPrivateKey = {client_private.decode()}\nAddress = 10.97.0.2/32\nDNS = 10.88.0.53, {suffix}\nMTU = 1420\n[Peer]\nPublicKey = {server_public}\nEndpoint = 127.0.0.1:31820\nAllowedIPs = 10.97.0.1/32, 10.88.0.53/32\nPersistentKeepalive = 25\n"
        profile = {"version": 1, "label": "Disposable kernel WireGuard", "family": "wireGuard", "source": "import", "server": "", "groupId": None,
                   "username": "", "password": "", "realm": "", "config": config, "remember": False}
        windows_path = lambda path: run("wslpath", "-w", path).stdout.decode().strip()
        data = {"version": 1, "fixtureId": identity, "profile": profile, "wrongKeyProfile": {**profile, "config": config.replace(server_public, wrong_public)},
                "kernelPeer": {"address": address, "port": 31820, "implementation": "Linux kernel WireGuard"},
                "target": "10.97.0.1", "port": 31823, "payloadBytes": 65536,
                "dnsFixture": {"profile": profile, "server": "10.88.0.53", "answer": "10.88.0.53", "suffix": suffix, "shortName": "probe", "fqdn": "probe." + suffix,
                               "receiptPath": windows_path(directory / "dns-receipt.json")},
                "peerReceiptPath": windows_path(directory / "peer-receipt.json"), "tcpReceiptPath": windows_path(directory / "tcp-receipt.json")}
        private_json(directory / "profiles.json", data)
        private_json(directory / "ready.json", {"version": 1, "fixtureId": identity, "pid": os.getpid(), "namespace": namespace, "peerPort": 31820,
                                               "durationSeconds": duration, "hostRoutesDnsChanged": before != snapshot(), "sourceSha256": source_hashes,
                                               "baselineSha256": baseline_digest, "linuxIncarnation": linux_incarnation()})
        print("WIREGUARD_WINDOWS_FIXTURE_READY", flush=True)
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline and not (directory / "stop").exists():
            assert all(process.poll() is None for process in processes), "Private tunnel service exited"
            handshake = run("ip", "netns", "exec", namespace, "wg", "show", "ttwg0", "latest-handshakes").stdout.split()
            transfer = run("ip", "netns", "exec", namespace, "wg", "show", "ttwg0", "transfer").stdout.split()
            private_json(directory / "peer-receipt.json", {"fixtureId": identity, "latestHandshakeUnix": int(handshake[1]), "receivedBytes": int(transfer[1]), "sentBytes": int(transfer[2]),
                                                        "kernelPeer": True, "namespace": namespace})
            time.sleep(.25)
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(3)
                except sp.TimeoutExpired:
                    process.kill()
                    process.wait(3)
        if interface_created and not moved:
            run("ip", "link", "delete", birth_interface)
        if namespace_created:
            run("ip", "netns", "delete", namespace)
        try:
            unchanged = compare_baseline(directory / "linux-baseline-before.json", identity, baseline_digest, linux_incarnation(), snapshot())
        except (OSError, ValueError, KeyError):
            unchanged = None
        evidence = {"version": 1, "fixtureId": identity, "namespaceRemoved": not Path("/run/netns", namespace).exists(), "hostNetworkUnchanged": unchanged,
                    "baselineSha256": baseline_digest, "baselineVerifiable": unchanged is not None,
                    "sourceSha256": source_hashes, "sourceUnchanged": source_hashes == {name: hashlib.sha256((source / name).read_bytes()).hexdigest() for name in source_files},
                    "windowsBrokerAdapterTested": False, "productionInstallationTouched": False}
        private_json(directory / "stopped.json", evidence)
        if report:
            report.parent.mkdir(parents=True, exist_ok=True)
            private_json(report, evidence)
        assert evidence["namespaceRemoved"] and evidence["hostNetworkUnchanged"]


def exercise(namespace, executable, private, public, mode, second_public):
    owner, endpoint = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
    profile = {"uapi": f"private_key={private}\nlisten_port=31821\nreplace_peers=true\npublic_key={public}\nendpoint=127.0.0.1:31820\nreplace_allowed_ips=true\nallowed_ip=10.97.0.0/24\n",
               "networkPlan": {"addresses": [{"cidr": "10.97.0.2/32", "gateway": ""}], "routes": ["10.97.0.0/24"], "dns": [], "searchDomain": "", "remote": "127.0.0.1", "excludedRoutes": ["127.0.0.1/32"], "mtu": 1420},
               "endpoints": ["127.0.0.1:31820"], "peerCount": 1}
    if mode == "wrong-key":
        profile["uapi"] = profile["uapi"].replace(public, keypair()[3])
    if mode == "unreachable":
        profile["uapi"] = profile["uapi"].replace(":31820", ":31822")
        profile["endpoints"] = ["127.0.0.1:31822"]
    if mode == "partial-peers":
        profile["uapi"] += f"public_key={keypair()[3]}\nendpoint=127.0.0.1:31822\nallowed_ip=10.98.0.0/24\n"
        profile["endpoints"].append("127.0.0.1:31822")
        profile["peerCount"] = 2
    if mode == "peer-loss":
        profile["uapi"] += f"public_key={second_public}\nendpoint=127.0.0.1:31824\nallowed_ip=10.98.0.0/24\n"
        profile["endpoints"].append("127.0.0.1:31824")
        profile["peerCount"] = 2
    child = sp.Popen(["ip", "netns", "exec", namespace, str(executable)], stdin=sp.PIPE, stdout=sp.PIPE,
                     stderr=sp.PIPE, pass_fds=(endpoint.fileno(),))
    selector = selectors.DefaultSelector()
    selector.register(child.stdout, selectors.EVENT_READ)
    session = {"sessionId": "00000000-0000-0000-0000-000000000001", "generation": 1}

    def send(value):
        child.stdin.write((json.dumps({**session, **value}) + "\n").encode())
        child.stdin.flush()

    def receive(deadline):
        if not selector.select(max(0, deadline - time.monotonic())):
            raise AssertionError("WireGuard engine event timed out")
        raw = child.stdout.readline(2 * 1024 * 1024 + 1)
        if not raw or len(raw) > 2 * 1024 * 1024:
            raise AssertionError("Engine closed/truncated control channel")
        value = json.loads(raw)
        assert value.get("sessionId") == session["sessionId"] and value.get("generation") == 1
        assert "private_key" not in raw.decode() and "preshared_key" not in raw.decode()
        return value

    started = time.monotonic()
    drop_rules = []

    def drop(direction, port):
        rule = ["OUTPUT", "-o", "lo", "-p", "udp", "--dport" if direction == "outbound" else "--sport", str(port), "-j", "DROP"]
        run("ip", "netns", "exec", namespace, "iptables", "-A", *rule)
        drop_rules.append(rule)

    def restore():
        while drop_rules:
            run("ip", "netns", "exec", namespace, "iptables", "-D", *drop_rules.pop())

    def quiet(seconds):
        assert not selector.select(seconds), "Live tunnel emitted an unexpected terminal event"
        assert child.poll() is None, "Live engine exited"

    try:
        send({"wireguard": profile})
        proposal = receive(started + 5)
        assert proposal["kind"] == "network-plan" and proposal["detail"] == profile["networkPlan"]
        send({"action": "accept-network"})
        assert receive(started + 5)["kind"] == "tunnel-ready"
        send({"action": "network-configured", "packetFd": endpoint.fileno()})
        endpoint.close()
        assert receive(started + 5)["kind"] == "progress"
        if mode == "cancel":
            send({"action": "disconnect"})
            assert receive(started + 5)["kind"] == "disconnected"
            assert child.wait(5) == 0
            return {"case": mode, "passed": True}
        if mode == "owner-death":
            child.stdin.close()
            assert child.wait(5) != 0
            return {"case": mode, "passed": True}
        event = receive(started + 34)
        if mode in ["wrong-key", "unreachable", "partial-peers"]:
            assert event["kind"] == "failed" and event["detail"] == "wireguard_peer_handshake_timeout", event
            assert time.monotonic() - started >= 29 and child.wait(5) != 0
            return {"case": mode, "passed": True, "connected": False, "boundedTimeout": True}
        assert event["kind"] == "connected" and event["detail"]["peers"] == profile["peerCount"]
        if mode == "idle":
            original = run("ip", "netns", "exec", namespace, "wg", "show", "ttwg0", "latest-handshakes").stdout.split()[1]
            quiet(18)
            refreshed = run("ip", "netns", "exec", namespace, "wg", "show", "ttwg0", "latest-handshakes").stdout.split()[1]
            assert int(refreshed) > int(original), "Idle kernel peer did not complete an explicit probe"
        if mode in ["short-drop", "blackhole", "outbound-loss", "inbound-loss", "peer-loss"]:
            loss_started = time.monotonic()
            port = 31824 if mode == "peer-loss" else 31820
            if mode != "inbound-loss":
                drop("outbound", port)
            if mode != "outbound-loss":
                drop("inbound", port)
            if mode == "short-drop":
                quiet(20)
                restore()
                quiet(12)
                assert int(run("ip", "netns", "exec", namespace, "wg", "show", "ttwg0", "latest-handshakes").stdout.split()[1]) > int(time.time() - 15), "Short loss did not recover a fresh handshake"
            else:
                event = receive(loss_started + 48)
                elapsed = time.monotonic() - loss_started
                assert event["kind"] == "failed" and event["detail"] == "wireguard_peer_unresponsive", event
                assert 29 <= elapsed <= 48 and child.wait(5) != 0, "Missing bounded liveness failure"
                return {"case": mode, "passed": True, "connectedBeforeLoss": True, "failedEventBeforeExit": True, "lossSeconds": round(elapsed, 3), "requiredPeers": profile["peerCount"]}
        payload = secrets.token_bytes(512)
        owner.send(packet(payload))
        owner.settimeout(5)
        received = owner.recv(65536)
        assert received[0] >> 4 == 4 and received[9] == 17
        header = (received[0] & 15) * 4
        assert received[header + 8:] == payload
        digest = hashlib.sha256(received[header + 8:]).hexdigest()
        send({"action": "disconnect"})
        assert receive(time.monotonic() + 5)["kind"] == "disconnected"
        assert child.wait(5) == 0
        handshake = run("ip", "netns", "exec", namespace, "wg", "show", "ttwg0", "latest-handshakes").stdout.decode()
        assert int(handshake.split()[1]) > 0
        return {"case": mode, "passed": True, "freshKernelPeerHandshake": True, "payloadBytes": len(payload), "payloadSha256": digest}
    finally:
        restore()
        owner.close()
        endpoint.close()
        selector.close()
        if child.poll() is None:
            child.kill()
            child.wait(5)
        diagnostics = child.stderr.read().decode()
        assert private not in diagnostics and public not in diagnostics


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", type=Path)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--windows-fixture", type=Path)
    parser.add_argument("--duration", type=int, default=7200)
    parser.add_argument("--tcp-receiver", type=Path)
    cases = ["success", "wrong-key", "unreachable", "partial-peers", "cancel", "owner-death", "idle", "short-drop", "blackhole", "outbound-loss", "inbound-loss", "peer-loss"]
    parser.add_argument("--case", action="append", choices=cases, help="Run selected focused cases; default runs all")
    args = parser.parse_args()
    if args.tcp_receiver:
        tcp_receiver(args.tcp_receiver)
        return
    if args.windows_fixture:
        windows_fixture(args.windows_fixture, args.duration, args.report)
        return
    if not args.engine or not args.report:
        parser.error("Use --engine/--report for interoperability, or --windows-fixture for private acceptance preparation")
    assert os.geteuid() == 0, "Run only in the disposable root Linux lab"
    before = snapshot()
    namespace = "ttwg-" + secrets.token_hex(5)
    echo = None
    results = []
    private, public, _, public_hex = keypair()
    second_private, _, _, second_public_hex = keypair()
    _, client_public, client_private_hex, _ = keypair()
    try:
        run("ip", "netns", "add", namespace)
        run("ip", "-n", namespace, "link", "set", "lo", "up")
        run("ip", "-n", namespace, "link", "add", "ttwg0", "type", "wireguard")
        config = b"[Interface]\nPrivateKey = " + private + b"\nListenPort = 31820\n[Peer]\nPublicKey = " + client_public + b"\nAllowedIPs = 10.97.0.2/32\nEndpoint = 127.0.0.1:31821\n"
        run("ip", "netns", "exec", namespace, "wg", "setconf", "ttwg0", "/dev/stdin", input=config)
        run("ip", "-n", namespace, "address", "add", "10.97.0.1/32", "dev", "ttwg0")
        run("ip", "-n", namespace, "link", "set", "ttwg0", "up")
        run("ip", "-n", namespace, "route", "add", "10.97.0.2/32", "dev", "ttwg0")
        run("ip", "-n", namespace, "link", "add", "ttwg1", "type", "wireguard")
        second_config = b"[Interface]\nPrivateKey = " + second_private + b"\nListenPort = 31824\n[Peer]\nPublicKey = " + client_public + b"\nAllowedIPs = 10.98.0.2/32\nEndpoint = 127.0.0.1:31821\n"
        run("ip", "netns", "exec", namespace, "wg", "setconf", "ttwg1", "/dev/stdin", input=second_config)
        run("ip", "-n", namespace, "link", "set", "ttwg1", "up")
        echo = sp.Popen(["ip", "netns", "exec", namespace, "python3", "-c", "import socket;s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);s.bind(('10.97.0.1',31823));print('ready',flush=True)\nwhile True:\n p,a=s.recvfrom(65535);s.sendto(p,a)"], stdout=sp.PIPE, stderr=sp.PIPE)
        assert echo.stdout.readline() == b"ready\n"
        for mode in args.case or cases:
            results.append(exercise(namespace, args.engine.resolve(), client_private_hex, public_hex, mode, second_public_hex))
            print(json.dumps(results[-1]), flush=True)
    finally:
        if echo and echo.poll() is None:
            echo.kill()
            echo.wait(5)
        run("ip", "netns", "delete", namespace)
        after = snapshot()
        report = {"version": 1, "engine": "official-wireguard-go", "peer": "Linux kernel WireGuard", "results": results,
                  "hostNetworkUnchanged": before == after, "namespaceRemoved": not Path("/run/netns", namespace).exists(),
                  "macosTested": False, "windowsBrokerAdapterTested": False}
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n")
        assert report["hostNetworkUnchanged"] and report["namespaceRemoved"]


if __name__ == "__main__":
    main()
