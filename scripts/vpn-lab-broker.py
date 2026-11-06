#!/usr/bin/env python3
"""Authenticated, namespace-only VPN broker. Never install as a host service."""
import argparse
import ipaddress
import json
import os
from pathlib import Path
import signal
import socket
import stat
import struct
import subprocess
import threading
import time
from urllib.parse import urlsplit

LIMIT = 2 * 1024 * 1024


def frame(stream):
    line = stream.readline(LIMIT + 1)
    if not line:
        raise EOFError()
    if len(line) > LIMIT or not line.endswith(b"\n"):
        raise ValueError("message_too_large")
    value = json.loads(line)
    if not isinstance(value, dict):
        raise ValueError("invalid_message")
    return value


def isolated():
    return os.geteuid() == 0 and os.stat("/proc/self/ns/net").st_ino != os.stat("/proc/1/ns/net").st_ino


class Broker:
    def __init__(self, engines, policy, ca_file=None):
        self.engines = engines
        self.ca_file = ca_file
        self.policy = subprocess.Popen([str(policy)], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.lock = threading.RLock()
        self.active = {}
        self.failed = False
        self.session_type = Session
        self.families = ("openVpn", "fortinet", "globalProtect", "cisco")

    def hello(self):
        namespace = os.stat("/proc/self/ns/net")
        return {"version": 1, "labOnly": True, "maxConnections": 3,
                "networkNamespace": {"device": namespace.st_dev, "inode": namespace.st_ino}}

    def policy_call(self, request):
        with self.lock:
            self.policy.stdin.write((json.dumps(request) + "\n").encode())
            self.policy.stdin.flush()
            result = frame(self.policy.stdout)
            if not result["ok"]:
                raise ValueError(result.get("error", "network_policy_rejected"))
            return result

    def start(self, owner, request):
        identity, generation = request["id"], request["generation"]
        if not isinstance(identity, str) or len(identity) != 36 or any(c not in "0123456789abcdef-" for c in identity):
            raise ValueError("invalid_session_id")
        if type(generation) is not int or not 0 < generation < 2 ** 53:
            raise ValueError("invalid_generation")
        profile = request["profile"]
        family = profile["family"]
        if family not in self.families:
            raise ValueError("unsupported_protocol")
        with self.lock:
            if self.failed:
                raise ValueError("network_policy_unavailable")
            if identity in self.active or len(self.active) >= 3:
                raise ValueError("maximum_three_connections")
            # The Rust importer rejects hooks and external root-readable file references.
            self.policy_call({"action": "validateProfile", "profile": profile})
            session = self.session_type(self, owner, identity, generation, request["profileId"], profile)
            self.active[identity] = session
            owner.sessions[identity] = session
            return session


def sso_origin(server):
    target = urlsplit(server)
    if target.scheme != "https" or not target.hostname or target.username or target.password:
        raise ValueError("invalid_sso_origin")
    host = "[" + target.hostname + "]" if ":" in target.hostname else target.hostname
    return "https://" + host + (":" + str(target.port) if target.port and target.port != 443 else "")


class Session:
    def __init__(self, broker, owner, identity, generation, profile_id, profile):
        self.broker, self.owner, self.profile = broker, owner, profile
        self.public = {"id": identity, "generation": generation, "profileId": profile_id,
                       "state": "connecting", "vaultId": owner.vault_id}
        self.process = None
        self.cancelled = False
        self.reserved = False
        self.interface = "ttv" + identity.replace("-", "")[:12]
        self.input_lock = threading.Lock()
        self.challenge = None
        self.challenge_at = None
        self.credential_autofill_used = False
        self.sso_origin = None
        self.thread = threading.Thread(target=self.run, daemon=True)

    def emit(self, state, **extra):
        with self.broker.lock:
            self.public = {**self.public, "state": state, **extra}
            if state != "challenge":
                self.public.pop("challenge", None)
            message = {"kind": "session", "session": self.public.copy()}
        self.owner.send(message)

    def write(self, value):
        with self.input_lock:
            if not self.process or self.process.poll() is not None:
                raise ValueError("session_closed")
            self.process.stdin.write((json.dumps(value) + "\n").encode())
            self.process.stdin.flush()

    def stop(self):
        self.cancelled = True
        self.emit("disconnecting")
        with self.input_lock:
            if self.process and self.process.poll() is None:
                # TERM closes even an authentication request blocked in a library callback.
                self.process.terminate()
                process = self.process
                def force_stop():
                    if process.poll() is None:
                        process.kill()
                timer = threading.Timer(2, force_stop)
                timer.daemon = True
                timer.start()

    def answer(self, request):
        challenge = self.challenge
        if not challenge or request["challengeId"] != challenge["id"]:
            raise ValueError("stale_challenge")
        if challenge.get("kind") == "sso":
            raise ValueError("sso_result_required")
        values = request["values"]
        if not isinstance(values, dict) or set(values) != {f["name"] for f in challenge["fields"]}:
            raise ValueError("invalid_answer_fields")
        for field in challenge["fields"]:
            value = values[field["name"]]
            if not isinstance(value, str) or len(value.encode()) > 4096 or "\0" in value:
                raise ValueError("invalid_answer")
            choices = field.get("choices", [])
            if choices and value not in {c["value"] for c in choices}:
                raise ValueError("invalid_choice")
        if self.profile["family"] == "globalProtect" and any(f["name"] == "gateway" and f["purpose"] == "choice" for f in challenge["fields"]):
            field = next(f for f in challenge["fields"] if f["name"] == "gateway")
            if values["gateway"] not in {c["value"] for c in field.get("choices", [])}:
                raise ValueError("invalid_gateway_choice")
            self.sso_origin = sso_origin("https://" + values["gateway"])
        self.challenge = None
        self.emit("connecting")
        self.write({"action": "answer", "id": request["challengeId"], "values": values})

    def initial_credential_fields(self, challenge):
        fields = challenge["fields"]
        if (challenge.get("kind") is None and all(challenge.get(key) in (None, "") for key in ("message", "error"))
                and len(fields) == 1 and fields[0].get("name") == "privateKeyPassword"
                and fields[0].get("purpose") == "keyPassword" and fields[0].get("secret") is True):
            return None
        if self.credential_autofill_used:
            return None
        if any(f.get("secret") is True or f.get("purpose") in ("secret", "password", "otp", "keyPassword") for f in fields):
            self.credential_autofill_used = True
        # Reuse the Windows matcher, without sending values or private SSO data to policy.
        metadata = {"fields": [{key: f.get(key) for key in ("name", "purpose", "label")} for f in fields],
                    "message": challenge.get("message"), "error": challenge.get("error")}
        return self.broker.policy_call({"action": "passwordChallengeFields", "challenge": metadata})["result"]

    def sso_answer(self, request):
        challenge = self.challenge
        if not challenge or challenge.get("kind") != "sso" or request["challengeId"] != challenge["id"] or time.monotonic() - self.challenge_at >= 300:
            raise ValueError("stale_challenge")
        private, result = challenge["private"], request["result"]
        if (not isinstance(result, dict) or set(result) != {"uri", "username", "token", "tokenName", "status", "authStatus"}
                or result["uri"] != private["expectedOrigin"] + "/SAML20/SP/ACS"
                or not isinstance(result["status"], int) or not 200 <= result["status"] < 300 or result["authStatus"] != "1"
                or result["tokenName"] not in ("prelogin-cookie", "portal-userauthcookie")):
            raise ValueError("invalid_sso_result")
        for key in ("username", "token"):
            value = result[key]
            if not isinstance(value, str) or not 1 <= len(value.encode()) <= 4096 or any(ord(c) < 32 or ord(c) == 127 for c in value):
                raise ValueError("invalid_sso_result")
        self.challenge = None
        self.emit("connecting")
        self.write({"action": "ssoAnswer", "id": request["challengeId"], "result": result})

    def network(self, detail):
        if detail.get("dns") or detail.get("dnsDomains") or detail.get("searchDomain") or detail.get("proxyPac") or detail.get("excludedRoutes") or any(detail.get("dnsOptions", {}).values()):
            raise ValueError("lab_broker_dns_proxy_exclusions_unsupported")
        if detail.get("address6"):
            raise ValueError("lab_broker_ipv6_address_unsupported")
        addresses = [str(ipaddress.ip_interface(a["cidr"])) for a in detail.get("addresses", [])]
        if detail.get("address"):
            prefix = ipaddress.IPv4Network("0.0.0.0/" + detail["netmask"]).prefixlen
            addresses.append(str(ipaddress.ip_interface(detail["address"] + "/" + str(prefix))))
        if not addresses or len(addresses) > 8:
            raise ValueError("invalid_addresses")
        routes = [str(ipaddress.ip_network(r, strict=False)) for r in detail["routes"]]
        # Full tunnel needs pinned transport bypass and DNS/firewall ownership first.
        if any(ipaddress.ip_network(r).prefixlen == 0 for r in routes):
            raise ValueError("lab_broker_full_tunnel_unsupported")
        mtu = detail.get("mtu", 0)
        if type(mtu) is not int or mtu and not 576 <= mtu <= 9000:
            raise ValueError("invalid_mtu")
        return addresses, routes, mtu

    def configure(self, detail):
        addresses, routes, mtu = self.network(detail)
        if detail.get("interface") != self.interface:
            raise ValueError("interface_mismatch")
        def ip(*args):
            subprocess.run(["/usr/sbin/ip", *args], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
        for address in addresses:
            ip("addr", "add", address, "dev", self.interface)
        if mtu:
            ip("link", "set", self.interface, "mtu", str(mtu))
        ip("link", "set", self.interface, "up")
        for route in routes:
            ip("-6" if ":" in route else "-4", "route", "add", route, "dev", self.interface)

    def initial(self):
        family = self.profile["family"]
        engine = "termterm-openvpn" if family == "openVpn" else "termterm-openconnect"
        initial = {"interface": self.interface}
        if family == "openVpn":
            initial["config"] = self.profile["config"]
        else:
            initial.update(protocol={"fortinet": "fortinet", "globalProtect": "gp", "cisco": "anyconnect"}[family],
                           server=self.profile["server"], realm=self.profile["realm"])
            if self.broker.ca_file:
                initial["caFile"] = str(self.broker.ca_file)
        return engine, initial

    def launch(self, engine):
        return subprocess.Popen([str(self.broker.engines / engine)], stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)

    def reserve(self, detail):
        addresses, routes, _ = self.network(detail)
        self.broker.policy_call({"action": "reserve", "id": self.public["id"], "generation": self.public["generation"],
                                 "plan": {"routes": routes + [str(ipaddress.ip_network(a, strict=False)) for a in addresses], "dnsServers": [], "dnsDomains": []}})
        self.reserved = True

    def configured(self):
        return {"action": "network-configured"}

    def cleanup(self):
        pass

    def run(self):
        error = None
        try:
            profile = self.profile
            family = profile["family"]
            engine, initial = self.initial()
            with self.input_lock:
                if self.cancelled:
                    return
                self.process = self.launch(engine)
            self.write(initial)
            while True:
                message = frame(self.process.stdout)
                kind, detail = message["kind"], message.get("detail", {})
                if self.cancelled:
                    break
                if kind == "challenge":
                    if detail.get("kind") == "sso":
                        origin = self.sso_origin or sso_origin(profile["server"])
                        if (family != "globalProtect" or detail.get("provider") != "globalProtect" or detail["fields"]
                                or detail["private"]["expectedOrigin"] != origin or detail["private"]["expectedPath"] != "/SAML20/SP/ACS"):
                            raise ValueError("invalid_sso_challenge")
                        self.challenge_at = time.monotonic()
                    self.challenge = detail
                    fields = detail["fields"]
                    credential_fields = self.initial_credential_fields(detail)
                    if profile["remember"] and profile["password"] and credential_fields:
                        user, password = credential_fields
                        self.answer({"challengeId": detail["id"], "values": {user: profile["username"], password: profile["password"]}})
                    else:
                        for field in fields:
                            if (field["name"], field.get("purpose")) in (("username", "username"), ("user", "text")):
                                field["value"] = profile["username"]
                        self.emit("challenge", challenge=detail)
                elif kind == "network-plan":
                    with self.broker.lock:
                        self.reserve(detail)
                    self.write({"action": "accept-network"})
                elif kind == "tunnel-ready":
                    with self.broker.lock:
                        self.configure(detail)
                    self.write(self.configured())
                elif kind == "connected":
                    if not self.reserved:
                        raise ValueError("unreserved_tunnel")
                    self.emit("connected")
                elif kind == "failed":
                    # Engine errors are bounded codes; raw authentication/cookie logs stay private.
                    error = detail if isinstance(detail, str) else detail.get("error", detail.get("name", "vpn_engine_failed"))
                elif kind == "disconnected":
                    break
        except EOFError:
            error = error or (None if self.cancelled else "engine_exited")
        except Exception as exc:
            error = str(exc)[:256]
        finally:
            if self.process:
                with self.input_lock:
                    if self.process.poll() is None:
                        self.process.terminate()
                try:
                    self.process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=3)
            # Kernel removes nonpersistent TUN and all owned routes before releasing reservation.
            with self.broker.lock:
                cleaned = True
                try:
                    self.cleanup()
                except Exception:
                    cleaned = False
                    self.broker.failed = True
                    error = "owned_network_cleanup_failed"
                if self.reserved and cleaned:
                    try:
                        self.broker.policy_call({"action": "release", "id": self.public["id"], "generation": self.public["generation"]})
                    except Exception:
                        self.broker.failed = True
                        error = error or "network_policy_unavailable"
                self.broker.active.pop(self.public["id"], None)
                self.profile = None
            self.emit("failed" if error and not self.cancelled else "disconnected", **({"error": error} if error and not self.cancelled else {}))


class Owner:
    def __init__(self, connection, broker):
        self.connection, self.broker = connection, broker
        self.sessions = {}
        self.write_lock = threading.Lock()
        self.closed = threading.Event()
        self.vault_id = None

    def send(self, value):
        if self.closed.is_set():
            return
        try:
            with self.write_lock:
                self.connection.sendall((json.dumps(value) + "\n").encode())
        except OSError:
            self.closed.set()
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

    def run(self):
        try:
            with self.connection.makefile("rb") as stream:
                while True:
                    request = frame(stream)
                    result, launch = None, None
                    try:
                        action = request["action"]
                        if action == "hello":
                            if self.vault_id or not isinstance(request.get("vaultId"), str) or len(request["vaultId"]) > 128:
                                raise ValueError("invalid_handshake")
                            self.vault_id = request["vaultId"]
                            result = self.broker.hello()
                        elif not self.vault_id:
                            raise ValueError("handshake_required")
                        elif action == "connect":
                            launch = self.broker.start(self, request)
                            result = launch.public.copy()
                        elif action in ("disconnect", "answer", "ssoAnswer"):
                            session = self.sessions.get(request["id"])
                            if not session or request["generation"] != session.public["generation"] or session.public["state"] in ("failed", "disconnected"):
                                raise ValueError("stale_session")
                            if action == "disconnect":
                                session.stop()
                            elif action == "ssoAnswer":
                                session.sso_answer(request)
                            else:
                                session.answer(request)
                        elif action == "sessions":
                            with self.broker.lock:
                                result = [s.public.copy() for s in self.sessions.values()]
                        elif action == "close":
                            return
                        else:
                            raise ValueError("unsupported_action")
                        self.send({"kind": "reply", "requestId": request.get("requestId"), "ok": True, "result": result})
                        if launch:
                            launch.thread.start()
                    except Exception as exc:
                        self.send({"kind": "reply", "requestId": request.get("requestId"), "ok": False, "error": str(exc)[:256]})
        except (EOFError, OSError, ValueError):
            pass
        finally:
            for session in list(self.sessions.values()):
                if session.thread.is_alive():
                    session.stop()
            for session in list(self.sessions.values()):
                if session.thread.is_alive():
                    session.thread.join(timeout=8)
            self.connection.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--socket", type=Path, required=True)
    parser.add_argument("--uid", type=int, required=True)
    parser.add_argument("--engines", type=Path, required=True)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--ca-file", type=Path, help="Root-selected lab CA; never a client-supplied path")
    args = parser.parse_args()
    if not isolated():
        parser.error("isolated_lab_namespace_required")
    parent = args.socket.parent
    if not parent.is_dir() or parent.stat().st_uid != 0 or parent.stat().st_mode & 0o022:
        parser.error("socket_directory_must_be_root_owned_and_not_writable_by_others")
    for binary in [args.policy, args.engines / "termterm-openvpn", args.engines / "termterm-openconnect"]:
        if not binary.is_absolute() or not binary.is_file() or any(p.stat().st_uid != 0 or p.stat().st_mode & 0o022 for p in [binary.resolve(), *binary.resolve().parents]):
            parser.error("engine_must_be_root_owned_and_not_writable_by_others")
    if args.socket.exists():
        parser.error("socket_already_exists")
    if args.ca_file and (not args.ca_file.is_absolute() or not args.ca_file.is_file() or args.ca_file.stat().st_uid != 0 or args.ca_file.stat().st_mode & 0o022):
        parser.error("lab_ca_must_be_root_owned_and_private")
    broker = Broker(args.engines, args.policy, args.ca_file)
    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    owners, threads = [], []
    try:
        listener.bind(str(args.socket))
        os.chmod(args.socket, 0o600)
        os.chown(args.socket, args.uid, -1)
        listener.listen(4)
        def stop(*_):
            raise KeyboardInterrupt()
        signal.signal(signal.SIGTERM, stop)
        while True:
            connection, _ = listener.accept()
            _, uid, _ = struct.unpack("3i", connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
            if uid != args.uid:
                connection.close()
                continue
            owner = Owner(connection, broker)
            connection.setsockopt(socket.SOL_SOCKET, socket.SO_SNDTIMEO, struct.pack("ll", 2, 0))
            owners.append(owner)
            thread = threading.Thread(target=owner.run, daemon=True)
            threads.append(thread)
            thread.start()
    except KeyboardInterrupt:
        pass
    finally:
        listener.close()
        for owner in owners:
            try:
                owner.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        for thread in threads:
            thread.join(timeout=10)
        broker.policy.terminate()
        broker.policy.wait(timeout=3)
        args.socket.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
