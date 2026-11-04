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
import socket
import uuid
import subprocess as sp
import struct
import tempfile
import time
from datetime import datetime, timedelta, timezone


def run(*args, **kwargs):
    return sp.run(list(map(str, args)), check=True, capture_output=True, **kwargs)


def snapshot():
    return {
        "routes4": run("ip", "-j", "route", "show", "table", "all").stdout.decode(),
        "routes6": run("ip", "-j", "-6", "route", "show", "table", "all").stdout.decode(),
        "dns": hashlib.sha256(Path("/etc/resolv.conf").read_bytes()).hexdigest(),
    }


def desktop_many_routes(nested=False):
    if nested:
        routes = [route for prefix in range(27, 33) for route in ipaddress.IPv4Network("10.88.0.0/24").subnets(new_prefix=prefix)]
        assert len(set(routes)) == 504 and all(route.subnet_of(ipaddress.IPv4Network("10.88.0.0/24")) for route in routes)
        return routes
    routes = [ipaddress.IPv4Network(f"10.88.{2 + number // 128}.{2 * (number % 128)}/32") for number in range(504)]
    assert len(set(routes)) == 504 and not any(route.overlaps(ipaddress.IPv4Network("10.88.0.0/24")) for route in routes)
    assert all(not route.overlaps(other) for index, route in enumerate(routes) for other in routes[index + 1:])
    return routes


def desktop_full_exclusions(many_routes=False, nested=False):
    # Keep only the disposable target and assigned tunnel ranges in the full plan.
    excluded = [ipaddress.IPv4Network("0.0.0.0/0")]
    extras = desktop_many_routes() if many_routes and not nested else []
    for allowed in [ipaddress.IPv4Network("10.88.0.0/24"), ipaddress.IPv4Network("10.96.0.0/24"), *extras]:
        excluded = [piece for network in excluded for piece in (
            network.address_exclude(allowed) if allowed.subnet_of(network) else [network])]
    assert sum(n.num_addresses for n in excluded) == 2**32 - 512 - len(extras)
    assert not any(ipaddress.IPv4Address(address) in n for n in excluded for address in ["10.88.0.53", "10.88.0.54", "10.96.0.2"])
    assert all(not route.overlaps(n) for route in extras for n in excluded)
    assert all(any(ipaddress.IPv4Address(address) in n for n in excluded) for address in ["127.0.0.1", "10.88.1.1"])
    return sorted(excluded)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engines", type=Path, required=True)
    parser.add_argument("--ocserv-root", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--policy", type=Path, required=True,
                        help="vpn-core vpn-policy-lab binary; shared reservation implementation")
    parser.add_argument("--desktop-fixture", type=Path,
                        help="Keep private disposable profiles and loopback-only desktop forwards inside this worktree's .lab")
    parser.add_argument("--desktop-duration", type=int, default=3600)
    parser.add_argument("--native-service", action="store_true", help="Also exercise the root-owned authenticated native Unix service with pinned peer identity")
    parser.add_argument("--desktop-dns", action="store_true",
                        help="Add a separate tunnel-only DNS fixture; requires --desktop-fixture")
    parser.add_argument("--desktop-full-tunnel", action="store_true",
                        help="Original /0 with exclusions preserving host Internet; requires --desktop-dns")
    parser.add_argument("--desktop-many-routes", action="store_true",
                        help="505 disjoint owned routes; requires --desktop-full-tunnel")
    parser.add_argument("--desktop-nested-routes", action="store_true",
                        help="Use 505 overlapping routes; requires --desktop-many-routes")
    args = parser.parse_args()
    if args.desktop_dns and not args.desktop_fixture:
        parser.error("--desktop-dns requires --desktop-fixture")
    if args.desktop_full_tunnel and not args.desktop_dns:
        parser.error("--desktop-full-tunnel requires --desktop-dns")
    if args.desktop_many_routes and not args.desktop_full_tunnel:
        parser.error("--desktop-many-routes requires --desktop-full-tunnel")
    if args.desktop_nested_routes and not args.desktop_many_routes:
        parser.error("--desktop-nested-routes requires --desktop-many-routes")
    if os.geteuid() != 0:
        parser.error("Run as root inside the disposable WSL lab")
    before = snapshot()
    suffix = secrets.token_hex(4)
    server_ns, client_ns = f"ttvpn-s-{suffix}", f"ttvpn-c-{suffix}"
    namespaces, processes, logs = [], [], []
    results = []
    completed = False
    completed_source = None
    source = Path(__file__).resolve().parents[1]
    desktop_files = []
    if args.desktop_fixture:
        args.desktop_fixture = args.desktop_fixture.resolve()
        assert args.desktop_fixture.is_relative_to((source / ".lab").resolve())
        assert args.desktop_fixture.is_dir() and not any(args.desktop_fixture.iterdir()), "Desktop fixture directory must exist and be empty/private"
        assert 1 <= args.desktop_duration <= 7200
    source_files = sorted((source / "vpn-engine").glob("*.cpp")) + sorted((source / "vpn-engine").glob("*.hpp"))
    source_files += sorted((source / "vpn-core/src").rglob("*.rs")) + [Path(__file__).resolve()]
    source_files += [source / "scripts/vpn-lab-broker.py"]
    if args.native_service:
        source_files += [source / path for path in ["scripts/vpn-unix-broker.py", "scripts/test-vpn-unix-service.py", "scripts/vpn-dns-fixture.py", "scripts/vpn_lab_json.py", "vpn-engine/macos-platform.m"]]
    if args.desktop_dns:
        source_files += [source / "scripts/vpn-dns-fixture.py", source / "scripts/vpn_lab_json.py"]
    source_files += [source / p for p in ["vpn-engine/CMakeLists.txt", "vpn-engine/dependencies.json",
                                         "vpn-core/Cargo.toml", "vpn-core/Cargo.lock", "scripts/prepare-vpn-lab.py"]]
    def source_hashes():
        return {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest() for p in source_files}
    tested_source = source_hashes()
    policy = sp.Popen([str(args.policy)], stdin=sp.PIPE, stdout=sp.PIPE)

    def policy_request(message):
        policy.stdin.write((json.dumps(message) + "\n").encode())
        policy.stdin.flush()
        ready = selectors.DefaultSelector()
        try:
            ready.register(policy.stdout, selectors.EVENT_READ)
            assert ready.select(3), "Network policy timed out"
            return json.loads(policy.stdout.readline())
        finally:
            ready.close()

    def reserve_network(identity, detail):
        routes = [str(ipaddress.ip_network(r, strict=False)) for r in detail["routes"]]
        addresses = [a["cidr"] for a in detail.get("addresses", [])]
        if detail.get("address"):
            addresses.append(detail["address"] + "/" + detail["netmask"])
        routes += [str(ipaddress.ip_network(a, strict=False)) for a in addresses]
        return policy_request({"action": "reserve", "id": identity, "generation": 1,
                               "plan": {"routes": routes, "dnsServers": detail["dns"],
                                        "dnsDomains": detail.get("dnsDomains", [])}})

    def release_network(identity):
        assert policy_request({"action": "release", "id": identity, "generation": 1})["ok"]

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

    def configure_network(interface, detail):
        assert not detail["dns"], "Lab deliberately excludes host DNS changes"
        assert not detail.get("excludedRoutes"), "Lab does not support excluded routes"
        assert not detail.get("proxyPac"), "Lab does not apply proxy PAC scripts"
        assert detail["interface"] == interface
        addresses = [a["cidr"] for a in detail.get("addresses", [])]
        if detail.get("address"):
            prefix = ipaddress.IPv4Network("0.0.0.0/" + detail["netmask"]).prefixlen
            addresses.append(f'{detail["address"]}/{prefix}')
        for address in addresses:
            ns(client_ns, "ip", "addr", "add", str(ipaddress.ip_interface(address)), "dev", interface)
        ns(client_ns, "ip", "link", "set", interface, "up")
        for route in detail["routes"]:
            network = ipaddress.ip_network(route, strict=False)
            assert str(network) in ["10.88.0.1/32", "10.88.0.2/32", "10.88.0.3/32"], "Unexpected lab route"
            ns(client_ns, "ip", "route", "add", str(network), "dev", interface)

    try:
        capabilities = json.loads(run(args.engines / "termterm-openconnect", "--capabilities").stdout)
        assert {"fortinet", "gp", "anyconnect"}.issubset(capabilities["detail"]["protocols"])
        assert capabilities["detail"]["isolatedLabOnly"]
        results.append({"test": "compiled-fortinet-globalprotect-protocols", "status": "passed",
                        "meaning": "compiled protocol availability only; not a company-server connection"})
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
            for n in [2, 3]:
                ns(server_ns, "ip", "addr", "add", f"10.88.0.{n}/32", "dev", "lo")
            # No default route, NAT, forwarding to the host, or DNS is configured.
            run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                "-subj", "/CN=TermTerm disposable lab CA", "-keyout", root / "ca.key", "-out", root / "ca.crt")
            # WSL clock resynchronization can step backward during startup. Give
            # disposable fixtures a small validity margin; TLS verification stays on.
            now = datetime.now(timezone.utc)
            start_date = (now - timedelta(minutes=5)).strftime("%Y%m%d%H%M%SZ")
            end_date = (now + timedelta(days=1)).strftime("%Y%m%d%H%M%SZ")
            (root / "index").write_text("")
            (root / "serial").write_text("1000\n")
            (root / "certificates").mkdir()
            (root / "ca.conf").write_text(f'''[ca]
default_ca = lab
[lab]
database = {root}/index
serial = {root}/serial
new_certs_dir = {root}/certificates
certificate = {root}/ca.crt
private_key = {root}/ca.key
default_md = sha256
policy = subject_policy
unique_subject = no
[subject_policy]
commonName = supplied
[ca_ext]
basicConstraints = critical,CA:true
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
''')
            run("openssl", "req", "-new", "-key", root / "ca.key", "-subj", "/CN=TermTerm disposable lab CA", "-out", root / "ca.csr")
            run("openssl", "ca", "-batch", "-notext", "-selfsign", "-config", root / "ca.conf",
                "-in", root / "ca.csr", "-out", root / "backdated-ca.crt", "-extensions", "ca_ext",
                "-startdate", start_date, "-enddate", end_date)
            (root / "backdated-ca.crt").replace(root / "ca.crt")
            for kind in ["server", "client"]:
                run("openssl", "req", "-newkey", "rsa:2048", "-nodes", "-subj", f"/CN=lab-{kind}",
                    "-keyout", root / f"{kind}.key", "-out", root / f"{kind}.csr")
                ext = "extendedKeyUsage=" + ("serverAuth\nsubjectAltName=IP:198.18.8.1,IP:127.0.0.1" if kind == "server" else "clientAuth")
                (root / f"{kind}.ext").write_text("basicConstraints=critical,CA:false\n" + ext + "\n")
                run("openssl", "ca", "-batch", "-notext", "-config", root / "ca.conf", "-in", root / f"{kind}.csr",
                    "-startdate", start_date, "-enddate", end_date, "-extfile", root / f"{kind}.ext",
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
''')
            ocserver = launch(server_ns, [args.ocserv_root / "usr/sbin/ocserv", "-f", "-c", root / "ocserv.conf"], root / "ocserv.log")
            (root / "openvpn.conf").write_text(f'''local 198.18.8.1
port 1194
proto tcp-server
dev tun
duplicate-cn
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
                    groups = ["-g", "employees"] if request.get("realm") else []
                    run(args.ocserv_root / "usr/bin/ocpasswd", "-c", root / "passwd", *groups, username,
                        input=(password + "\n" + password + "\n").encode())
                    if otp is not None:
                        with (root / "users.otp").open("a") as users:
                            users.write(f"HOTP {username} - {otp_key.hex()}\n")
                net_before = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
                proc = launch(client_ns, [args.engines / engine], root / f"{case}.log", stdin=True)
                write(proc, request)
                events, failures, challenges, secret_rounds, connected = [], [], 0, 0, False
                reserved = False
                choice_prompt = False
                selector = selectors.DefaultSelector()
                selector.register(proc.stdout, selectors.EVENT_READ)
                pending = b""
                secret_tail = b""
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
                        assert password.encode() not in secret_tail + chunk, "Engine exposed a password on its event channel"
                        secret_tail = (secret_tail + chunk)[-4096:]
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
                                choice_prompt = choice_prompt or any(f.get("choices") for f in detail["fields"])
                                failures.append({"challengeFields": [(f["name"], f.get("label")) for f in detail["fields"]], "message": detail.get("message"), "error": detail.get("error")})
                                if any(f["secret"] for f in detail["fields"]):
                                    secret_rounds += 1
                                if expected == "cancel" or challenges > 3:
                                    write(proc, {"action": "cancel"})
                                else:
                                    secret = otp if otp is not None and secret_rounds == 2 else ("wrong-password" if wrong_password else password)
                                    values = {f["name"]: f["choices"][0]["value"] if f.get("choices") else otp if f.get("purpose") == "otp" else secret if f["secret"] else username
                                              for f in detail["fields"]}
                                    write(proc, {"action": "answer", "id": detail["id"], "values": values})
                            elif kind == "network-plan":
                                assert reserve_network(case, detail)["ok"], "Unexpected route conflict"
                                reserved = True
                                write(proc, {"action": "accept-network"})
                            elif kind == "tunnel-ready":
                                assert reserved
                                configure_network(request["interface"], detail)
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
                    server_log = root / (("ov-dynamic.log" if "dynamic" in case else "ov-auth.log" if otp is not None else "openvpn.log")
                                         if engine == "termterm-openvpn" else "realm.log" if request.get("realm") else "ocserv-otp.log" if otp is not None else "ocserv.log")
                    diagnostic = (root / "dynamic-trace").read_text() if (root / "dynamic-trace").exists() else ""
                    assert connected == (expected == "connected"), f"{case}: {events}, {failures}; helper log: {(root / (case + '.log')).read_text()[-1500:]}; server log: {server_log.read_text()[-3000:]}; auth stages: {diagnostic[-1000:]}"
                    assert connected or "failed" in events, f"{case}: no explicit failure"
                    if otp is not None and engine == "termterm-openconnect":
                        assert secret_rounds >= 2, "Server did not request a separate OTP"
                    if request.get("realm") == "missing":
                        assert choice_prompt, "Unknown realm must ask for an explicit selection"
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
                    if reserved:
                        release_network(case)

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
            check("termterm-openvpn", {"config": config, "interface": "ttvlab"}, "openvpn3-tunnel", "connected")
            check("termterm-openvpn", {"config": config, "interface": "ttvlab"}, "openvpn3-crash-cleanup", "connected", crash=True)
            run("openssl", "pkcs8", "-topk8", "-in", root / "client.key", "-out", root / "encrypted-client.key",
                "-v2", "aes-256-cbc", "-passout", "stdin", input=(password + "\n").encode())
            encrypted_config = config.replace((root / "client.key").read_text(), (root / "encrypted-client.key").read_text())
            encrypted_request = {"config": encrypted_config, "interface": "ttvlab"}
            check("termterm-openvpn", encrypted_request, "openvpn3-encrypted-private-key", "connected")
            check("termterm-openvpn", encrypted_request, "openvpn3-wrong-key-password", "failed", wrong_password=True)
            check("termterm-openvpn", encrypted_request, "openvpn3-key-prompt-cancel", "cancel")
            stop(ocserver)
            (root / "ocserv.conf").write_text(occonfig.replace(f",otp={root}/users.otp", ""))
            ocserver = launch(server_ns, [args.ocserv_root / "usr/sbin/ocserv", "-f", "-c", root / "ocserv.conf"], root / "ocserv.log")
            deadline = time.monotonic() + 5
            while b":4443" not in ns(server_ns, "ss", "-ltn").stdout:
                assert ocserver.poll() is None and time.monotonic() < deadline, "Crash-test server did not start"
                time.sleep(.1)
            check("termterm-openconnect", oc, "openconnect-crash-cleanup", "connected", crash=True)
            # Two OpenVPN instances plus one OpenConnect tunnel, in the same client
            # namespace. These are not Fortinet/GlobalProtect server simulations.
            stop(ovserver)
            original_ov = (root / "openvpn.conf").read_text()
            parallel_servers = {}
            for index in [2, 3]:
                text = original_ov.replace("10.88.0.1", f"10.88.0.{index}")
                if index == 3:
                    text = text.replace("1194", "1195").replace("10.90.0.0", "10.91.0.0")
                (root / f"ov{index}.conf").write_text(text)
                parallel_servers[index] = launch(server_ns, ["openvpn", "--config", root / f"ov{index}.conf"], root / f"ov{index}.log")
            deadline = time.monotonic() + 5
            while b":1195" not in ns(server_ns, "ss", "-ltn").stdout:
                assert time.monotonic() < deadline, "Parallel lab servers not ready"
                time.sleep(.1)
            concurrent_before = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            receiver = root / "receive-digest.py"
            receiver.write_text('''import hashlib, socket, sys
with socket.socket() as listener:
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((sys.argv[1], 18443)); listener.listen(4)
    while True:
        connection, _ = listener.accept()
        with connection:
            connection.settimeout(5)
            digest = hashlib.sha256()
            while data := connection.recv(65536):
                digest.update(data)
            connection.sendall(digest.digest())
''')
            for index in [1, 2, 3]:
                launch(server_ns, ["python3", receiver, f"10.88.0.{index}"], root / f"digest{index}.log")
            deadline = time.monotonic() + 5
            while ns(server_ns, "ss", "-ltn").stdout.count(b":18443") != 3:
                assert time.monotonic() < deadline, "Payload test listeners not ready"
                time.sleep(.1)

            def connect_live(engine, request, identity, reject=False):
                proc = launch(client_ns, [args.engines / engine], root / f"{identity}.log", stdin=True)
                write(proc, request)
                selector = selectors.DefaultSelector()
                selector.register(proc.stdout, selectors.EVENT_READ)
                deadline, pending, denied = time.monotonic() + 25, b"", False
                try:
                    while time.monotonic() < deadline:
                        if not selector.select(.2):
                            continue
                        chunk = os.read(proc.stdout.fileno(), 65536)
                        if not chunk:
                            break
                        pending += chunk
                        while b"\n" in pending:
                            line, pending = pending.split(b"\n", 1)
                            event = json.loads(line)
                            kind, detail = event["kind"], event.get("detail")
                            if kind == "challenge":
                                values = {f["name"]: password if f["secret"] else "labuser" for f in detail["fields"]}
                                write(proc, {"action": "answer", "id": detail["id"], "values": values})
                            elif kind == "network-plan":
                                accepted = reserve_network(identity, detail)["ok"]
                                assert accepted != reject, f"Unexpected reservation result: {identity}"
                                denied = not accepted
                                write(proc, {"action": "accept-network" if accepted else "reject-network"})
                            elif kind == "tunnel-ready":
                                assert not reject, "Rejected VPN must not create a TUN"
                                configure_network(request["interface"], detail)
                                write(proc, {"action": "network-configured"})
                            elif kind == "connected":
                                assert not reject
                                return proc
                            elif kind == "failed":
                                assert reject and denied, f"Unexpected helper failure: {detail}"
                                proc.wait(timeout=5)
                                return proc
                    raise AssertionError(f"Parallel tunnel {identity} did not complete")
                finally:
                    selector.close()

            live = []
            for index in [1, 2, 3]:
                request = {**oc, "interface": f"ttvp{index}"} if index == 1 else {
                    "config": config if index == 2 else config.replace("1194", "1195"), "interface": f"ttvp{index}"}
                proc = connect_live("termterm-openconnect" if index == 1 else "termterm-openvpn", request, f"parallel-{index}")
                live.append(proc)
            for index in [1, 2, 3]:
                route = ns(client_ns, "ip", "route", "get", f"10.88.0.{index}").stdout
                assert f"dev ttvp{index}".encode() in route
                ns(client_ns, "ping", "-c", "2", "-W", "2", f"10.88.0.{index}")
            results.append({"test": "three-concurrent-real-tunnels", "status": "passed", "realTunnelTraffic": True,
                            "engines": ["OpenConnect/AnyConnect", "OpenVPN3", "OpenVPN3"]})
            sender = root / "send-digest.py"
            sender.write_text('''import concurrent.futures, hashlib, secrets, socket
def send(index):
    payload = secrets.token_bytes(1024 * 1024)
    with socket.create_connection((f"10.88.0.{index}", 18443), timeout=10) as connection:
        connection.sendall(payload); connection.shutdown(socket.SHUT_WR)
        digest = b""
        while len(digest) < 32:
            part = connection.recv(32 - len(digest))
            assert part, "Truncated server digest"
            digest += part
        assert digest == hashlib.sha256(payload).digest(), "Tunnel payload changed"
with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
    list(pool.map(send, [1, 2, 3]))
''')
            def verify_payloads():
                try:
                    ns(client_ns, "python3", sender)
                except sp.CalledProcessError as exc:
                    raise AssertionError("Concurrent TCP payload test failed: " + exc.stderr.decode(errors="replace")[-3000:]) from exc
            verify_payloads()
            results.append({"test": "three-concurrent-tcp-payload-digests", "status": "passed",
                            "bytesPerTunnel": 1024 * 1024, "hash": "SHA256"})
            # A fourth overlapping server plan must fail before touching the network.
            overlap_before = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            connect_live("termterm-openvpn", {"config": config, "interface": "ttvp4"}, "parallel-conflict", reject=True)
            assert overlap_before == ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            results.append({"test": "live-overlap-rejected-before-network-change", "status": "passed"})
            write(live[1], {"action": "disconnect"})
            live[1].wait(timeout=5)
            release_network("parallel-2")
            for index in [1, 3]:
                assert live[index - 1].poll() is None
                ns(client_ns, "ping", "-c", "2", "-W", "2", f"10.88.0.{index}")
            results.append({"test": "disconnect-middle-keeps-other-tunnels", "status": "passed"})
            stop(parallel_servers[3])
            live[2].wait(timeout=5)
            release_network("parallel-3")
            assert live[0].poll() is None
            ns(client_ns, "ping", "-c", "2", "-W", "2", "10.88.0.1")
            results.append({"test": "server-loss-stops-only-affected-vpn", "status": "passed"})
            # Closed control channel is a dead desktop/broker, not a persistent VPN.
            for index in [1]:
                live[index - 1].stdin.close()
                live[index - 1].wait(timeout=5)
                release_network(f"parallel-{index}")
            assert concurrent_before == ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            results.append({"test": "concurrent-parent-disconnect-cleanup", "status": "passed", "routesRestored": True})
            # Exercise the same authenticated IPC that the native desktop uses.
            parallel_servers[3] = launch(server_ns, ["openvpn", "--config", root / "ov3.conf"], root / "broker-ov3.log")
            broker_socket = root / "broker.sock"
            broker = launch(client_ns, ["python3", source / "scripts/vpn-lab-broker.py", "--socket", broker_socket,
                                        "--uid", "0", "--engines", args.engines, "--policy", args.policy,
                                        "--ca-file", root / "ca.crt"], root / "broker.log")
            deadline = time.monotonic() + 10
            while not broker_socket.exists():
                assert broker.poll() is None, (root / "broker.log").read_text()
                assert time.monotonic() < deadline, "Broker did not listen"
                time.sleep(.05)
            connection = socket.socket(socket.AF_UNIX)
            connection.settimeout(30)
            connection.connect(str(broker_socket))
            broker_stream = connection.makefile("rb")
            broker_states, request_number = {}, 0
            def broker_read():
                message = json.loads(broker_stream.readline())
                if message["kind"] == "session":
                    state = message["session"]
                    broker_states[state["id"]] = state
                return message
            def broker_request(action, **fields):
                nonlocal request_number
                request_number += 1
                connection.sendall((json.dumps({"action": action, "requestId": request_number, **fields}) + "\n").encode())
                while True:
                    message = broker_read()
                    if message["kind"] == "reply" and message["requestId"] == request_number:
                        return message
            def broker_wait(identity, desired):
                deadline = time.monotonic() + 30
                while time.monotonic() < deadline:
                    state = broker_states.get(identity, {})
                    if state.get("state") in desired:
                        return state
                    if state.get("state") == "challenge":
                        challenge = state["challenge"]
                        values = {f["name"]: password if f["secret"] else "labuser" for f in challenge["fields"]}
                        assert broker_request("answer", id=identity, generation=1, challengeId=challenge["id"], values=values)["ok"]
                    else:
                        broker_read()
                raise AssertionError("Broker session timed out")
            assert broker_request("hello", vaultId="disposable-vault")["ok"]
            broker_ids = [str(uuid.uuid4()) for _ in range(3)]
            def broker_profile(index):
                return {"version":1,"label":"Disposable lab","family":"cisco" if index == 0 else "openVpn",
                        "source":"lab","server":oc["server"] if index == 0 else "","groupId":None,
                        "username":"labuser","password":"","realm":"","remember":False,
                        "config":"" if index == 0 else config if index == 1 else config.replace("1194", "1195")}
            for index, identity in enumerate(broker_ids):
                reply = broker_request("connect", id=identity, generation=1, profileId=f"lab-{index}", profile=broker_profile(index))
                assert reply["ok"], reply
                state = broker_wait(identity, {"connected", "failed"})
                assert state["state"] == "connected", state
            verify_payloads()
            results.append({"test":"authenticated-broker-three-real-tunnels-tcp", "status":"passed", "bytesPerTunnel":1024*1024})
            before_reject = ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            assert not broker_request("connect", id=str(uuid.uuid4()), generation=1, profileId="four", profile=broker_profile(1))["ok"]
            assert not broker_request("disconnect", id=broker_ids[1], generation=2)["ok"]
            assert before_reject == ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout
            results.append({"test":"broker-rejects-fourth-and-stale-generation", "status":"passed"})
            assert broker_request("disconnect", id=broker_ids[1], generation=1)["ok"]
            assert broker_wait(broker_ids[1], {"disconnected", "failed"})["state"] == "disconnected"
            for target in [1, 3]:
                ns(client_ns, "ping", "-c", "1", "-W", "2", f"10.88.0.{target}")
            results.append({"test":"broker-middle-disconnect-preserves-other-traffic", "status":"passed"})
            conflict_id = str(uuid.uuid4())
            assert broker_request("connect", id=conflict_id, generation=1, profileId="conflict", profile=broker_profile(2))["ok"]
            rejected = broker_wait(conflict_id, {"failed"})
            assert "çakış" in rejected["error"], rejected
            results.append({"test":"broker-conflict-before-network-change", "status":"passed"})
            broker_stream.close()
            connection.close()
            deadline = time.monotonic() + 10
            while concurrent_before != ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout:
                assert time.monotonic() < deadline, "Broker desktop EOF left routes"
                time.sleep(.05)
            results.append({"test":"broker-desktop-eof-cleans-owned-routes", "status":"passed"})
            stop(broker)
            denied_socket = root / "denied.sock"
            denied_broker = launch(client_ns, ["python3", source / "scripts/vpn-lab-broker.py", "--socket", denied_socket,
                                             "--uid", "65534", "--engines", args.engines, "--policy", args.policy], root / "denied-broker.log")
            deadline = time.monotonic() + 5
            while not denied_socket.exists():
                assert denied_broker.poll() is None, (root / "denied-broker.log").read_text()
                assert time.monotonic() < deadline
                time.sleep(.05)
            with socket.socket(socket.AF_UNIX) as denied:
                denied.settimeout(3)
                denied.connect(str(denied_socket))
                assert denied.recv(1) == b"", "Wrong peer UID accepted"
            results.append({"test":"broker-rejects-wrong-peer-uid", "status":"passed"})
            stop(denied_broker)
            # Restart with fresh transport and kill the broker, retaining the client socket.
            broker = launch(client_ns, ["python3", source / "scripts/vpn-lab-broker.py", "--socket", broker_socket,
                                        "--uid", "0", "--engines", args.engines, "--policy", args.policy,
                                        "--ca-file", root / "ca.crt"], root / "broker-restart.log")
            deadline = time.monotonic() + 5
            while not broker_socket.exists():
                assert broker.poll() is None, (root / "broker-restart.log").read_text()
                assert time.monotonic() < deadline
                time.sleep(.05)
            connection = socket.socket(socket.AF_UNIX)
            connection.settimeout(30)
            connection.connect(str(broker_socket))
            broker_stream = connection.makefile("rb")
            assert broker_request("hello", vaultId="disposable-vault")["ok"]
            crash_id = str(uuid.uuid4())
            assert broker_request("connect", id=crash_id, generation=1, profileId="crash", profile=broker_profile(1))["ok"]
            assert broker_wait(crash_id, {"connected", "failed"})["state"] == "connected"
            os.kill(broker.pid, signal.SIGKILL)
            broker.wait(timeout=5)
            deadline = time.monotonic() + 5
            while concurrent_before != ns(client_ns, "ip", "-j", "route", "show", "table", "all").stdout:
                assert time.monotonic() < deadline, "Broker SIGKILL left routes"
                time.sleep(.05)
            broker_stream.close()
            connection.close()
            broker_socket.unlink(missing_ok=True)
            results.append({"test":"broker-sigkill-cleans-native-tun-owner", "status":"passed"})
            if args.native_service:
                import importlib.util
                native_spec = importlib.util.spec_from_file_location("native_service_tests", source / "scripts/test-vpn-unix-service.py")
                native_tests = importlib.util.module_from_spec(native_spec)
                native_spec.loader.exec_module(native_tests)
                (root / "native-auth.json").write_text(json.dumps({"password":password}))
                (root / "native-auth.json").chmod(0o600)
                native_tests.exercise(source,args,root,client_ns,server_ns,broker_profile,ns,results)
            # Disposable authentication server: validate the real SCRV1 wire response.
            static_otp = f"{secrets.randbelow(1000000):06d}"
            (root / "expected-auth.json").write_text(json.dumps({"password": password, "otp": static_otp}))
            (root / "expected-auth.json").chmod(0o600)
            verifier = root / "verify-auth.py"
            verifier.write_text('''#!/usr/bin/python3
import base64, hmac, json, pathlib, sys
expected = json.loads(pathlib.Path(__file__).with_name("expected-auth.json").read_text())
try:
    username, response = pathlib.Path(sys.argv[1]).read_text().splitlines()[:2]
    kind, password, otp = response.split(":")
    valid = kind == "SCRV1" and username.startswith("labuser")
    valid = valid and hmac.compare_digest(base64.b64decode(password, validate=True).decode(), expected["password"])
    valid = valid and hmac.compare_digest(base64.b64decode(otp, validate=True).decode(), expected["otp"])
except Exception:
    valid = False
sys.exit(0 if valid else 1)
''')
            verifier.chmod(0o700)
            auth_server = original_ov.replace("1194", "1196").replace("10.90.0.0", "10.92.0.0")
            auth_server += f"\nscript-security 2\nauth-user-pass-verify {verifier} via-file\n"
            (root / "ov-auth.conf").write_text(auth_server)
            launch(server_ns, ["openvpn", "--config", root / "ov-auth.conf"], root / "ov-auth.log")
            deadline = time.monotonic() + 5
            while b":1196" not in ns(server_ns, "ss", "-ltn").stdout:
                assert time.monotonic() < deadline, "OTP OpenVPN server not ready"
                time.sleep(.1)
            auth_config = config.replace("1194", "1196") + '\nauth-user-pass\nstatic-challenge "One-time code" 0\n'
            auth_request = {"config": auth_config, "interface": "ttvlab"}
            check("termterm-openvpn", auth_request, "openvpn3-static-otp", "connected", otp=static_otp)
            wrong_static_otp = "000000" if static_otp != "000000" else "000001"
            check("termterm-openvpn", auth_request, "openvpn3-wrong-static-otp", "failed", otp=wrong_static_otp)
            check("termterm-openvpn", auth_request, "openvpn3-auth-cancel", "cancel")
            dynamic_verifier = root / "verify-dynamic.py"
            dynamic_verifier.write_text('''#!/usr/bin/python3
import base64, hashlib, hmac, json, os, pathlib, secrets, sys
root = pathlib.Path(__file__).parent
expected = json.loads((root / "expected-auth.json").read_text())
def trace(value):
    with (root / "dynamic-trace").open("a") as output:
        output.write(json.dumps(value) + "\\n")
try:
    username, response = pathlib.Path(sys.argv[1]).read_text().splitlines()[:2]
    state_file = root / ("challenge-" + hashlib.sha256(username.encode()).hexdigest())
    if response.startswith("CRV1::"):
        _, _, state, _, otp = response.split(":", 4)
        state_ok = hmac.compare_digest(state_file.read_text(), state)
        otp_ok = hmac.compare_digest(otp, expected["otp"])
        valid = state_ok and otp_ok
        trace({"stage": "response", "stateMatches": state_ok, "otpMatches": otp_ok})
        state_file.unlink(missing_ok=True)
        sys.exit(0 if valid else 1)
    if not hmac.compare_digest(response, expected["password"]):
        sys.exit(1)
    state = secrets.token_hex(24)
    state_file.write_text(state); state_file.chmod(0o600)
    trace({"stage": "challenge"})
    reason = "CRV1:R:" + state + ":" + base64.b64encode(username.encode()).decode() + ":One-time code"
    pathlib.Path(os.environ["auth_failed_reason_file"]).write_text(reason)
except Exception as error:
    trace({"stage": "error", "type": type(error).__name__})
sys.exit(1)
''')
            dynamic_verifier.chmod(0o700)
            dynamic_server = auth_server.replace("1196", "1197").replace("10.92.0.0", "10.93.0.0").replace(str(verifier), str(dynamic_verifier))
            (root / "ov-dynamic.conf").write_text(dynamic_server)
            launch(server_ns, ["openvpn", "--config", root / "ov-dynamic.conf"], root / "ov-dynamic.log")
            deadline = time.monotonic() + 5
            while b":1197" not in ns(server_ns, "ss", "-ltn").stdout:
                assert time.monotonic() < deadline, "Dynamic OTP server not ready"
                time.sleep(.1)
            dynamic_request = {"config": config.replace("1194", "1197") + "\nauth-user-pass\n", "interface": "ttvlab"}
            check("termterm-openvpn", dynamic_request, "openvpn3-server-dynamic-otp", "connected", otp=static_otp)
            check("termterm-openvpn", dynamic_request, "openvpn3-wrong-dynamic-otp", "failed", otp=wrong_static_otp)
            realm_config = (root / "ocserv.conf").read_text().replace("4443", "4444").replace("10.89.0.0", "10.94.0.0")
            realm_config = realm_config.replace("ocserv.sock", "realm.sock").replace("device = ocvpn", "device = realmtun")
            realm_config += "\nselect-group = employees[Employees]\nselect-group = admins[Administrators]\n"
            (root / "realm.conf").write_text(realm_config)
            launch(server_ns, [args.ocserv_root / "usr/sbin/ocserv", "-f", "-c", root / "realm.conf"], root / "realm.log")
            deadline = time.monotonic() + 5
            while b":4444" not in ns(server_ns, "ss", "-ltn").stdout:
                assert time.monotonic() < deadline, "Realm server not ready"
                time.sleep(.1)
            realm_request = {**oc, "server": "https://198.18.8.1:4444", "realm": "employees"}
            check("termterm-openconnect", realm_request, "openconnect-saved-realm", "connected")
            check("termterm-openconnect", {**realm_request, "realm": "missing"}, "openconnect-unknown-realm-prompts", "connected")
            completed = True
            completed_source = source_hashes()
            if args.desktop_fixture:
                ns(server_ns, "ip", "addr", "add", "10.88.0.4/32", "dev", "lo")
                fourth = original_ov.replace("1194", "1198").replace("10.90.0.0", "10.95.0.0").replace("10.88.0.1", "10.88.0.4")
                (root / "desktop-ov4.conf").write_text(fourth)
                launch(server_ns, ["openvpn", "--config", root / "desktop-ov4.conf"], root / "desktop-ov4.log")
                relay = root / "desktop-forward.py"
                relay.write_text('''import os,socket,subprocess,sys,threading
def bridge(connection, process):
    def send():
        try:
            while data := connection.recv(65536):
                process.stdin.write(data); process.stdin.flush()
        except OSError: pass
        finally: process.stdin.close()
    worker=threading.Thread(target=send,daemon=True); worker.start()
    try:
        while data := os.read(process.stdout.fileno(),65536): connection.sendall(data)
    except OSError: pass
    finally:
        connection.close(); process.terminate(); process.wait()
if sys.argv[1] == "target":
    connection=socket.create_connection(("198.18.8.1",int(sys.argv[2])),timeout=5); connection.settimeout(None)
    def send():
        while data := os.read(0,65536): connection.sendall(data)
        connection.shutdown(socket.SHUT_WR)
    worker=threading.Thread(target=send,daemon=True); worker.start()
    while data := connection.recv(65536): os.write(1,data)
else:
    listener=socket.socket(); listener.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
    listener.bind(("127.0.0.1",int(sys.argv[2]))); listener.listen(8)
    while True:
        connection,_=listener.accept()
        process=subprocess.Popen(["ip","netns","exec",sys.argv[1],"python3",__file__,"target",sys.argv[3]],stdin=subprocess.PIPE,stdout=subprocess.PIPE)
        threading.Thread(target=bridge,args=(connection,process),daemon=True).start()
''')
                forwards = [(31194,1194),(31195,1195),(31198,1198),(31443,4443)]
                if args.desktop_dns:
                    dns_suffix = secrets.token_hex(8) + ".termterm.test"
                    ns(server_ns, "ip", "addr", "add", "10.88.0.53/32", "dev", "lo")
                    dns_config = original_ov.replace("1194", "1199").replace("10.90.0.0", "10.96.0.0").replace("10.88.0.1", "10.88.0.53")
                    if args.desktop_full_tunnel:
                        dns_config = dns_config.replace('push "route 10.88.0.53 255.255.255.255"', 'push "route 0.0.0.0 0.0.0.0"')
                        for excluded in desktop_full_exclusions(args.desktop_many_routes, args.desktop_nested_routes):
                            dns_config += f'\npush "route {excluded.network_address} {excluded.netmask} net_gateway"\n'
                        ns(server_ns, "ip", "addr", "add", "10.88.0.54/32", "dev", "lo")
                        launch(server_ns, ["python3", receiver, "10.88.0.54"], root / "desktop-full-digest.log")
                    dns_config += f'\npush "dhcp-option DNS 10.88.0.53"\npush "dhcp-option ADAPTER_DOMAIN_SUFFIX {dns_suffix}"\n'
                    (root / "desktop-dns.conf").write_text(dns_config)
                    launch(server_ns, ["openvpn", "--config", root / "desktop-dns.conf"], root / "desktop-dns.log")
                    forwards.append((31199,1199))
                for local, remote in forwards:
                    log = open(root / f"desktop-forward-{local}.log", "wb"); logs.append(log)
                    forward = sp.Popen(["python3",relay,server_ns,str(local),str(remote)],stdout=log,stderr=log,start_new_session=True)
                    processes.append(forward)
                    deadline = time.monotonic() + 5
                    while f"pid={forward.pid},".encode() not in run("ss","-ltnp",f"sport = :{local}").stdout:
                        assert forward.poll() is None, f"Desktop forward {local} failed"
                        assert time.monotonic() < deadline, f"Desktop forward {local} not ready"
                        time.sleep(.05)
                fixture = args.desktop_fixture
                def private_file(name, content):
                    destination = fixture / name
                    with open(destination, "x") as handle:
                        os.chmod(destination, 0o600); handle.write(content)
                    desktop_files.append(destination)
                    return destination
                ca_path = private_file("ca.crt", (root / "ca.crt").read_text())
                profiles = []
                for number, local_port, original_port, target in [(1,31194,1194,"10.88.0.2"),(2,31195,1195,"10.88.0.3"),(3,31198,1198,"10.88.0.4")]:
                    content = config.replace("198.18.8.1 1194",f"127.0.0.1 {local_port}") + "\nverify-x509-name lab-server name\n"
                    path = private_file(f"desktop-{number}.ovpn", content)
                    profiles.append({"profile":{**broker_profile(1),"label":f"Disposable OpenVPN {number}","config":content},"target":target,"path":str(path)})
                ca_windows = run("wslpath", "-w", ca_path).stdout.decode().strip()
                probe_profiles = profiles[:2] + [{"profile":{**broker_profile(0),"server":"https://127.0.0.1:31443"},"target":"10.88.0.1","caFile":ca_windows}]
                fixture_data = {"profiles":profiles,"probeProfiles":probe_profiles,"username":"labuser","password":password}
                if args.desktop_dns:
                    dns_receipt = private_file("dns-receipt.json", '{}\n')
                    desktop_files.append(dns_receipt.with_suffix(".tmp"))
                    responder = launch(server_ns, ["python3", source / "scripts/vpn-dns-fixture.py", "--name", "probe." + dns_suffix,
                                                   "--receipt", dns_receipt], root / "desktop-dns-responder.log")
                    deadline = time.monotonic() + 5
                    while b":1199" not in ns(server_ns, "ss", "-ltn").stdout or not all(
                            b"10.88.0.53:53" in ns(server_ns, "ss", flag).stdout for flag in ["-lun", "-ltn"]):
                        assert responder.poll() is None and time.monotonic() < deadline, "Disposable DNS fixture not ready"
                        time.sleep(.05)
                    dns_profile = {**broker_profile(1),"label":"Disposable DNS OpenVPN",
                                   "config":config.replace("198.18.8.1 1194", "127.0.0.1 31199") + "\nverify-x509-name lab-server name\n"}
                    private_file("desktop-dns.ovpn", dns_profile["config"])
                    fixture_data["dnsFixture"] = {"profile":dns_profile,"server":"10.88.0.53","suffix":dns_suffix,
                                                   "shortName":"probe","fqdn":"probe." + dns_suffix,"answer":"10.88.0.53",
                                                   "receiptPath":run("wslpath", "-w", dns_receipt).stdout.decode().strip()}
                    if args.desktop_full_tunnel:
                        assert 'push "route 10.88.0.54' not in dns_config
                        fixture_data["dnsFixture"]["fullTunnel"] = {"target":"10.88.0.54","port":18443,"peer":"127.0.0.1",
                                                                     "excludedTarget":"10.88.1.1","hostInternetPreserved":True,
                                                                     "serverExclusions":[str(n) for n in desktop_full_exclusions(args.desktop_many_routes, args.desktop_nested_routes)]}
                        if args.desktop_many_routes:
                            fixture_data["dnsFixture"]["fullTunnel"].update({"manyRoutesDisjoint": not args.desktop_nested_routes,
                                "manyRoutesNested": args.desktop_nested_routes,
                                "manyRoutePrefixes": [str(n) for n in desktop_many_routes(args.desktop_nested_routes)]})
                            if args.desktop_nested_routes:
                                fixture_data["dnsFixture"]["fullTunnel"]["nestedRouteTargets"] = ["10.88.0.2", "10.88.0.3"]
                    results.append({"test":"desktop-dns-fixture-prepared","status":"prepared","hostDnsChanged":False,
                                    "normalWindowsResolverTested":False})
                private_file("profiles.json", json.dumps(fixture_data,indent=2))
                private_file("ready.json",json.dumps({"pid":os.getpid(),"ports":[local for local, _ in forwards],"durationSeconds":args.desktop_duration,"hostRoutesDnsChanged":False}))
                print("DESKTOP_FIXTURE_READY " + str(fixture),flush=True)
                deadline = time.monotonic() + args.desktop_duration
                while time.monotonic() < deadline and not (fixture / "stop").exists():
                    time.sleep(.25)
    finally:
        policy.stdin.close()
        try:
            policy.wait(timeout=3)
        except sp.TimeoutExpired:
            policy.kill(); policy.wait(timeout=3)
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
        for path in desktop_files:
            path.unlink(missing_ok=True)
        unchanged = before == snapshot()
        args.report.parent.mkdir(parents=True, exist_ok=True)
        evidence = {"tests": results, "completed": completed, "hostNetworkUnchanged": unchanged,
                    "namespacesRemoved": True, "productionInstallationTouched": False,
                    "utc": datetime.now(timezone.utc).isoformat(),
                    "sourceSha256": tested_source, "sourceUnchangedDuringTest": tested_source == (completed_source if args.desktop_fixture and completed_source else source_hashes()),
                    "binarySha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in
                                     [args.engines / "termterm-openconnect", args.engines / "termterm-openvpn", args.policy]},
                    "unverified": ["Windows desktop connection", "Fortinet server", "GlobalProtect server",
                                   "production privileged broker", "split DNS", "IPv6 tunnel", "macOS"]}
        if args.native_service:
            evidence["nativeUnixService"]={"scope":"Root-owned helpers and authenticated nonroot pinned fixture client in a disposable Linux network namespace", "installedOnHost":False, "desktopClientTested":False,
                "clientAuthentication":"SO_PEERCRED + pidfd + exact executable path/SHA-256", "protocol":2,
                "wireguardBinarySha256":hashlib.sha256((source/".tools/vpn-wireguard/termterm-wireguard-linux").read_bytes()).hexdigest()}
            evidence["unverified"]=["Host-installed production service", "Linux full desktop SSO flow", "Fortinet server", "GlobalProtect server", "split DNS with real systemd-resolved", "IPv6 tunnel", "macOS"]
            if any(test["test"] == "native-unix-real-resolved-private-mount-network-dbus" and test["status"] == "passed" for test in results):
                evidence["unverified"].remove("split DNS with real systemd-resolved")
        args.report.write_text(json.dumps(evidence, indent=2) + "\n")
        assert unchanged, "Host network snapshot changed during lab test"
        assert evidence["sourceUnchangedDuringTest"], "Test sources changed during the run; repeat on a stable source snapshot"


if __name__ == "__main__":
    main()
