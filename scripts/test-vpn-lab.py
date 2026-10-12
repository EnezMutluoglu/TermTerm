#!/usr/bin/env python3
"""Root-only Linux VPN tests in disposable network namespaces; no host routes/DNS."""
import argparse
import hashlib
import hmac
import json
import ipaddress
import os
from pathlib import Path
import secrets
import selectors
import signal
import subprocess as sp
import struct
import tempfile
import time


def run(*args, **kwargs):
    return sp.run(list(map(str, args)), check=True, capture_output=True, **kwargs)


def snapshot():
    return {
        "routes4": run("ip", "-j", "route", "show", "table", "all").stdout.decode(),
        "routes6": run("ip", "-j", "-6", "route", "show", "table", "all").stdout.decode(),
        "dns": hashlib.sha256(Path("/etc/resolv.conf").read_bytes()).hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engines", type=Path, required=True)
    parser.add_argument("--ocserv-root", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error("Run as root inside the disposable WSL lab")
    before = snapshot()
    suffix = secrets.token_hex(4)
    server_ns, client_ns = f"ttvpn-s-{suffix}", f"ttvpn-c-{suffix}"
    namespaces, processes, logs = [], [], []
    results = []

    def ns(name, *command):
        return run("ip", "netns", "exec", name, *command)

    def launch(name, command, log, stdin=False):
        handle = open(log, "wb")
        logs.append(handle)
        process = sp.Popen(["ip", "netns", "exec", name, *map(str, command)],
                           stdin=sp.PIPE if stdin else sp.DEVNULL,
                           stdout=sp.PIPE if stdin else handle, stderr=handle,
                           start_new_session=True)
        processes.append(process)
        return process

    def stop(process):
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except sp.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)

    def write(process, message):
        process.stdin.write((json.dumps(message) + "\n").encode())
        process.stdin.flush()

    try:
        for engine in ["termterm-openconnect", "termterm-openvpn"]:
            rejected = sp.run([str(args.engines / engine)], input=b"{}\n", capture_output=True, timeout=5)
            event = json.loads(rejected.stdout)
            assert rejected.returncode != 0 and event["detail"] == "isolated_lab_namespace_required"
            results.append({"test": engine + "-reject-host-namespace", "status": "passed"})
        with tempfile.TemporaryDirectory(prefix="termterm-vpn-lab-") as tmp:
            root = Path(tmp)
            root.chmod(0o711)  # ocserv's unprivileged worker must traverse to its socket.
            for name in [server_ns, client_ns]:
                run("ip", "netns", "add", name)
                namespaces.append(name)
                ns(name, "sysctl", "-qw", "net.ipv6.conf.default.accept_dad=0")
                ns(name, "ip", "link", "set", "lo", "up")
            run("ip", "link", "add", "ttvs", "netns", server_ns, "type", "veth",
                "peer", "name", "ttvc", "netns", client_ns)
            ns(server_ns, "ip", "addr", "add", "198.18.8.1/30", "dev", "ttvs")
            ns(client_ns, "ip", "addr", "add", "198.18.8.2/30", "dev", "ttvc")
            ns(server_ns, "ip", "link", "set", "ttvs", "addrgenmode", "none")
            ns(client_ns, "ip", "link", "set", "ttvc", "addrgenmode", "none")
            ns(server_ns, "ip", "link", "set", "ttvs", "up")
            ns(client_ns, "ip", "link", "set", "ttvc", "up")
            deadline = time.monotonic() + 3
            while b"ff00::/8" not in ns(client_ns, "ip", "-6", "route", "show", "table", "all").stdout:
                assert time.monotonic() < deadline, "IPv6 link initialization did not finish"
                time.sleep(.05)
            ns(server_ns, "ip", "addr", "add", "10.88.0.1/32", "dev", "lo")
            # No default route, NAT, forwarding to the host, or DNS is configured.
            run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                "-subj", "/CN=TermTerm disposable lab CA", "-keyout", root / "ca.key", "-out", root / "ca.crt")
            for kind in ["server", "client"]:
                run("openssl", "req", "-newkey", "rsa:2048", "-nodes", "-subj", f"/CN=lab-{kind}",
                    "-keyout", root / f"{kind}.key", "-out", root / f"{kind}.csr")
                ext = "extendedKeyUsage=" + ("serverAuth\nsubjectAltName=IP:198.18.8.1" if kind == "server" else "clientAuth")
                (root / f"{kind}.ext").write_text(ext + "\n")
                run("openssl", "x509", "-req", "-in", root / f"{kind}.csr", "-CA", root / "ca.crt",
                    "-CAkey", root / "ca.key", "-CAcreateserial", "-days", "1", "-extfile", root / f"{kind}.ext",
                    "-out", root / f"{kind}.crt")
            password = secrets.token_urlsafe(24)
            run(args.ocserv_root / "usr/bin/ocpasswd", "-c", root / "passwd", "labuser",
                input=(password + "\n" + password + "\n").encode())
            (root / "ocserv.conf").write_text(f'''auth = "plain[passwd={root}/passwd]"
listen-host = 198.18.8.1
tcp-port = 4443
udp-port = 4443
run-as-user = nobody
run-as-group = nogroup
socket-file = {root}/ocserv.sock
server-cert = {root}/server.crt
server-key = {root}/server.key
isolate-workers = false
max-clients = 4
max-same-clients = 4
rate-limit-ms = 0
server-stats-reset-time = 604800
keepalive = 30
dpd = 30
mobile-dpd = 30
try-mtu-discovery = false
device = ocvpn
ipv4-network = 10.89.0.0
ipv4-netmask = 255.255.255.0
route = 10.88.0.1/255.255.255.255
default-domain = lab.invalid
''')
            ocserver = launch(server_ns, [args.ocserv_root / "usr/sbin/ocserv", "-f", "-c", root / "ocserv.conf"], root / "ocserv.log")
            (root / "openvpn.conf").write_text(f'''local 198.18.8.1
port 1194
proto tcp-server
dev tun
topology subnet
server 10.90.0.0 255.255.255.0
ca {root}/ca.crt
cert {root}/server.crt
key {root}/server.key
dh none
data-ciphers AES-256-GCM:AES-128-GCM
push "route 10.88.0.1 255.255.255.255"
verb 3
''')
            ovserver = launch(server_ns, ["openvpn", "--config", root / "openvpn.conf"], root / "openvpn.log")
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if ocserver.poll() is not None or ovserver.poll() is not None:
                    raise RuntimeError("Lab server startup failed: " + (root / "ocserv.log").read_text()[-2000:] + (root / "openvpn.log").read_text()[-2000:])
                ports = ns(server_ns, "ss", "-ltn").stdout
                if b":4443" in ports and b":1194" in ports:
                    break
                time.sleep(.1)
            else:
                raise RuntimeError("Lab listeners not ready")

            def check(engine, request, case, expected, wrong_password=False, otp=None, crash=False):
                username = "labuser" + str(len(results))
                if engine == "termterm-openconnect":
                    run(args.ocserv_root / "usr/bin/ocpasswd", "-c", root / "passwd", username,
                        input=(password + "\n" + password + "\n").encode())
                    if otp is not None:
                        with (root / "users.otp").open("a") as users:
                            users.write(f"HOTP {username} - {otp_key.hex()}\n")
                net_before = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
                proc = launch(client_ns, [args.engines / engine], root / f"{case}.log", stdin=True)
                write(proc, request)
                events, failures, challenges, secret_rounds, connected = [], [], 0, 0, False
                selector = selectors.DefaultSelector()
                selector.register(proc.stdout, selectors.EVENT_READ)
                pending = b""
                deadline = time.monotonic() + 25
                try:
                    while time.monotonic() < deadline:
                        if not selector.select(.2):
                            if proc.poll() is not None:
                                break
                            continue
                        chunk = os.read(proc.stdout.fileno(), 65536)
                        if not chunk:
                            break
                        pending += chunk
                        while b"\n" in pending:
                            line, pending = pending.split(b"\n", 1)
                            event = json.loads(line)
                            kind, detail = event["kind"], event.get("detail")
                            events.append(kind)
                            if kind == "failed":
                                failures.append(detail)
                            if kind == "challenge":
                                challenges += 1
                                failures.append({"challengeFields": [(f["name"], f.get("label")) for f in detail["fields"]], "message": detail.get("message"), "error": detail.get("error")})
                                if any(f["secret"] for f in detail["fields"]):
                                    secret_rounds += 1
                                if expected == "cancel" or challenges > 3:
                                    write(proc, {"action": "cancel"})
                                else:
                                    secret = otp if otp is not None and secret_rounds == 2 else ("wrong-password" if wrong_password else password)
                                    values = {f["name"]: secret if f["secret"] else username for f in detail["fields"]}
                                    write(proc, {"action": "answer", "id": detail["id"], "values": values})
                            elif kind == "network-plan":
                                write(proc, {"action": "accept-network"})
                            elif kind == "tunnel-ready":
                                assert not detail["dns"], "Lab deliberately excludes host DNS changes"
                                assert detail["interface"] == "ttvlab"
                                prefix = ipaddress.IPv4Network("0.0.0.0/" + detail["netmask"]).prefixlen
                                address = ipaddress.IPv4Address(detail["address"])
                                ns(client_ns, "ip", "addr", "add", f"{address}/{prefix}", "dev", "ttvlab")
                                ns(client_ns, "ip", "link", "set", "ttvlab", "up")
                                for route in detail["routes"]:
                                    network = ipaddress.ip_network(route, strict=False)
                                    assert network == ipaddress.ip_network("10.88.0.1/32"), "Unexpected lab route"
                                    ns(client_ns, "ip", "route", "add", str(network), "dev", "ttvlab")
                                write(proc, {"action": "network-configured"})
                            elif kind == "connected":
                                connected = True
                                assert expected == "connected", f"Unexpected connection in {case}"
                                ns(client_ns, "ping", "-c", "2", "-W", "2", "10.88.0.1")
                                if crash:
                                    os.killpg(proc.pid, signal.SIGKILL)
                                else:
                                    write(proc, {"action": "disconnect"})
                        if proc.poll() is not None:
                            break
                    proc.wait(timeout=5)
                    server_log = root / ("ocserv-otp.log" if otp is not None else "ocserv.log")
                    assert connected == (expected == "connected"), f"{case}: {events}, {failures}; helper log: {(root / (case + '.log')).read_text()[-1500:]}; server log: {server_log.read_text()[-3000:]}"
                    assert connected or "failed" in events, f"{case}: no explicit failure"
                    if otp is not None:
                        assert secret_rounds >= 2, "Server did not request a separate OTP"
                    net_after = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
                    assert net_before == net_after, f"{case}: routes were not restored: {net_before!r} -> {net_after!r}"
                    results.append({"test": case, "status": "passed", "events": events, "realTunnelTraffic": connected, "routesRestored": True})
                    print(f"PASS {case}", flush=True)
                except Exception as error:
                    results.append({"test": case, "status": "failed", "events": events,
                                    "realTunnelTraffic": connected, "reason": str(error)})
                    raise
                finally:
                    selector.close()
                    stop(proc)

            oc = {"protocol": "anyconnect", "server": "https://198.18.8.1:4443", "caFile": str(root / "ca.crt"), "interface": "ttvlab"}
            check("termterm-openconnect", oc, "openconnect-tunnel", "connected")
            check("termterm-openconnect", oc, "openconnect-wrong-password", "failed", wrong_password=True)
            check("termterm-openconnect", oc, "openconnect-cancel", "cancel")
            check("termterm-openconnect", {**oc, "caFile": None}, "openconnect-untrusted-certificate", "failed")
            stop(ocserver)
            otp_key = secrets.token_bytes(20)

            def hotp(counter):
                digest = hmac.new(otp_key, struct.pack(">Q", counter), hashlib.sha1).digest()
                offset = digest[-1] & 15
                return f"{(struct.unpack('>I', digest[offset:offset + 4])[0] & 0x7fffffff) % 1000000:06d}"

            wrong_otp = next(f"{n:06d}" for n in range(1000000) if f"{n:06d}" not in {hotp(i) for i in range(21)})
            (root / "users.otp").write_text(f"HOTP labuser - {otp_key.hex()}\n")
            (root / "users.otp").chmod(0o600)
            occonfig = (root / "ocserv.conf").read_text().replace(f"passwd={root}/passwd]", f"passwd={root}/passwd,otp={root}/users.otp]")
            (root / "ocserv.conf").write_text(occonfig)
            ocserver = launch(server_ns, [args.ocserv_root / "usr/sbin/ocserv", "-f", "-d", "1", "-c", root / "ocserv.conf"], root / "ocserv-otp.log")
            deadline = time.monotonic() + 5
            while b":4443" not in ns(server_ns, "ss", "-ltn").stdout:
                assert ocserver.poll() is None and time.monotonic() < deadline, "OTP server did not start"
                time.sleep(.1)
            check("termterm-openconnect", oc, "openconnect-otp-tunnel", "connected", otp=hotp(1))
            check("termterm-openconnect", oc, "openconnect-wrong-otp", "failed", otp=wrong_otp)
            config = "client\ndev tun\nproto tcp-client\nremote 198.18.8.1 1194\nremote-cert-tls server\n"
            for tag in ["ca", "cert", "key"]:
                filename = "ca.crt" if tag == "ca" else "client." + ("crt" if tag == "cert" else "key")
                config += f"<{tag}>\n{(root / filename).read_text()}</{tag}>\n"
            check("termterm-openvpn", {"config": config}, "openvpn3-tunnel", "connected")
            check("termterm-openvpn", {"config": config}, "openvpn3-crash-cleanup", "connected", crash=True)
            stop(ocserver)
            (root / "ocserv.conf").write_text(occonfig.replace(f",otp={root}/users.otp", ""))
            ocserver = launch(server_ns, [args.ocserv_root / "usr/sbin/ocserv", "-f", "-c", root / "ocserv.conf"], root / "ocserv.log")
            deadline = time.monotonic() + 5
            while b":4443" not in ns(server_ns, "ss", "-ltn").stdout:
                assert ocserver.poll() is None and time.monotonic() < deadline, "Crash-test server did not start"
                time.sleep(.1)
            check("termterm-openconnect", oc, "openconnect-crash-cleanup", "connected", crash=True)
    finally:
        for process in reversed(processes):
            stop(process)
        for handle in logs:
            handle.close()
        for name in reversed(namespaces):
            remaining = run("ip", "netns", "pids", name).stdout.split()
            for pid in remaining:
                try:
                    os.kill(int(pid), signal.SIGKILL)
                except ProcessLookupError:
                    pass
            run("ip", "netns", "delete", name)
        unchanged = before == snapshot()
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps({"tests": results, "hostNetworkUnchanged": unchanged,
                                         "namespacesRemoved": True, "productionInstallationTouched": False}, indent=2) + "\n")
        assert unchanged, "Host network snapshot changed during lab test"


if __name__ == "__main__":
    main()
