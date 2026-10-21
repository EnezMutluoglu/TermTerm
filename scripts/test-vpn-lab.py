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
from datetime import datetime, timedelta, timezone


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
    parser.add_argument("--policy", type=Path, required=True,
                        help="vpn-core vpn-policy-lab binary; shared reservation implementation")
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error("Run as root inside the disposable WSL lab")
    before = snapshot()
    suffix = secrets.token_hex(4)
    server_ns, client_ns = f"ttvpn-s-{suffix}", f"ttvpn-c-{suffix}"
    namespaces, processes, logs = [], [], []
    results = []
    source = Path(__file__).resolve().parents[1]
    source_files = sorted((source / "vpn-engine").glob("*.cpp")) + sorted((source / "vpn-engine").glob("*.hpp"))
    source_files += sorted((source / "vpn-core/src").rglob("*.rs")) + [Path(__file__).resolve()]
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
                ext = "extendedKeyUsage=" + ("serverAuth\nsubjectAltName=IP:198.18.8.1" if kind == "server" else "clientAuth")
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
default-domain = lab.invalid
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
            ns(client_ns, "python3", sender)
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
        unchanged = before == snapshot()
        args.report.parent.mkdir(parents=True, exist_ok=True)
        evidence = {"tests": results, "hostNetworkUnchanged": unchanged,
                    "namespacesRemoved": True, "productionInstallationTouched": False,
                    "utc": datetime.now(timezone.utc).isoformat(),
                    "sourceSha256": tested_source, "sourceUnchangedDuringTest": tested_source == source_hashes(),
                    "binarySha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in
                                     [args.engines / "termterm-openconnect", args.engines / "termterm-openvpn", args.policy]},
                    "unverified": ["Windows desktop connection", "Fortinet server", "GlobalProtect server",
                                   "production privileged broker", "split DNS", "IPv6 tunnel", "macOS"]}
        args.report.write_text(json.dumps(evidence, indent=2) + "\n")
        assert unchanged, "Host network snapshot changed during lab test"
        assert evidence["sourceUnchangedDuringTest"], "Test sources changed during the run; repeat on a stable source snapshot"


if __name__ == "__main__":
    main()
