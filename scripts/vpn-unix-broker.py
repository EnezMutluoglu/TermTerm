#!/usr/bin/env python3
"""Private development VPN service. Configuration and executable pins are root-selected."""
import argparse
import errno
import fcntl
import hashlib
import importlib.util
import ipaddress
import json
import os
from pathlib import Path
import re
import select
import signal
import socket
import stat
import struct
import subprocess
import sys
import threading
import time
import uuid

spec = importlib.util.spec_from_file_location("termterm_vpn_lab", Path(__file__).with_name("vpn-lab-broker.py"))
lab = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lab)
CAPABILITY = b"TERMTERM-VPN-BROKER/1\n"
SOCKET = "/run/termterm-vpn-dev/broker.sock" if sys.platform == "linux" else "/private/var/run/termterm-vpn-dev/broker.sock"
CONFIG = "/etc/termterm-vpn-dev/broker.json" if sys.platform == "linux" else "/Library/Application Support/TermTerm VPN Dev/broker.json"


def contract(candidate=None):
    require(candidate in (None, "0.4.4"), "unsupported_candidate_version")
    name = "termterm-vpn" if candidate else "termterm-vpn-dev"
    identity = "local.termterm.desktop" if candidate else "local.termterm.desktop.vpndev"
    return {"identity": identity, "socket": "/run/" + name + "/broker.sock" if sys.platform == "linux" else "/private/var/run/" + name + "/broker.sock",
            "config": "/etc/" + name + "/broker.json" if sys.platform == "linux" else "/Library/Application Support/" + ("TermTerm VPN" if candidate else "TermTerm VPN Dev") + "/broker.json"}


def validate_configuration(config, candidate=None):
    selected = contract(candidate)
    fields = {"uid", "client", "clientSha256", "engines", "pins", "journal"}
    optional = {"caFile", "clientRequirement", "python", "pythonSha256"}
    if candidate:
        fields |= {"applicationIdentifier", "candidateVersion"}
        if sys.platform == "linux":
            optional |= {"clientInstall"}
    require(isinstance(config, dict) and fields <= set(config) and not set(config) - fields - optional, "invalid_service_configuration")
    require(type(config["uid"]) is int and config["uid"] > 0, "nonroot_client_uid_required")
    if candidate:
        require(config["applicationIdentifier"] == selected["identity"] and config["candidateVersion"] == candidate, "candidate_identity_version_mismatch")
        helpers = {"vpn-policy-lab", "termterm-openvpn", "termterm-openconnect", "termterm-wireguard"} | ({"termterm-vpn-macos"} if sys.platform == "darwin" else set())
        require(isinstance(config["pins"], dict) and set(config["pins"]) == helpers, "candidate_helper_pins_required")
        if sys.platform == "linux":
            installation = config.get("clientInstall", "deb")
            require(installation in {"deb", "appimage"}, "unsupported_candidate_installation")
            client = "/opt/termterm-appimage/usr/bin/termterm" if installation == "appimage" else "/usr/bin/termterm"
            require(config["client"] == client and config["engines"] == "/opt/termterm-vpn/bin" and config["journal"] == "/var/lib/termterm-vpn", "candidate_installation_paths_required")
        else:
            require(config["client"] == "/Applications/TermTerm.app/Contents/MacOS/termterm"
                    and config["engines"] == "/Library/PrivilegedHelperTools/local.termterm.desktop.broker"
                    and config["journal"] == "/Library/Application Support/TermTerm VPN/journal", "candidate_installation_paths_required")


def require(condition, error):
    if not condition:
        raise ValueError(error)


def trusted(path, directory=False):
    path = Path(path)
    require(path.is_absolute() and path == path.resolve(), "trusted_path_must_be_absolute_without_symlinks")
    for part in [path, *path.parents]:
        metadata = part.lstat()
        require(metadata.st_uid == 0 and metadata.st_mode & 0o022 == 0, "trusted_path_must_be_root_owned")
        require(not stat.S_ISLNK(metadata.st_mode), "trusted_path_symlink_rejected")
    require(path.is_dir() if directory else path.is_file(), "trusted_path_wrong_type")
    return path


def pin(path, digest):
    path = trusted(path)
    require(isinstance(digest, str) and re.fullmatch(r"[0-9a-f]{64}", digest), "invalid_executable_pin")
    with path.open("rb") as stream:
        require(sha256(stream) == digest, "executable_pin_mismatch")
    return path


def sha256(stream):
    digest = hashlib.sha256()
    for block in iter(lambda: stream.read(1024 * 1024), b""):
        digest.update(block)
    return digest.hexdigest()


def command(binary, *args, input=None):
    return subprocess.run([binary, *map(str, args)], input=input, check=True, capture_output=True, timeout=8).stdout


def verify_loader_paths(binary):
    if sys.platform == "darwin":
        for row in command("/usr/bin/otool", "-L", binary).decode().splitlines()[1:]:
            dependency = row.strip().split(" (", 1)[0]
            if dependency.startswith(("/usr/lib/", "/System/Library/")):
                continue  # System dylibs may exist only in the protected dyld shared cache.
            if dependency.startswith(("@loader_path/", "@executable_path/")):
                dependency = binary.parent / dependency.split("/", 1)[1]
            require(not str(dependency).startswith("@"), "unresolved_runtime_dependency")
            trusted(Path(dependency).resolve())
    else:
        dynamic = command("/usr/bin/readelf", "-d", binary).decode()
        for search in re.findall(r"\((?:RPATH|RUNPATH)\).*\[(.*?)\]", dynamic):
            for directory in search.split(":"):
                directory = directory.replace("${ORIGIN}", str(binary.parent)).replace("$ORIGIN", str(binary.parent))
                require(directory.startswith("/") and "$" not in directory, "unsafe_runtime_library_path")
                trusted(Path(directory).resolve(), True)


def canonical_uuid(value):
    require(isinstance(value, str) and str(uuid.UUID(value)) == value, "invalid_uuid")
    return value


def frame(stream):
    def pairs(values):
        result = {}
        for key, value in values:
            require(key not in result, "duplicate_json_field")
            result[key] = value
        return result
    line = stream.readline(lab.LIMIT + 1)
    if not line:
        raise EOFError()
    require(len(line) <= lab.LIMIT and line.endswith(b"\n"), "message_too_large")
    value = json.loads(line, object_pairs_hook=pairs)
    require(isinstance(value, dict), "invalid_message")
    return value


def subtract(routes, exclusions):
    result = list(routes)
    for exclusion in exclusions:
        next_routes = []
        for route in result:
            if route.version != exclusion.version or not route.overlaps(exclusion):
                next_routes.append(route)
            elif exclusion.subnet_of(route):
                next_routes.extend(route.address_exclude(exclusion))
        result = next_routes
        require(len(result) <= 1024, "too_many_effective_routes")
    return sorted(set(result), key=lambda route: (route.version, int(route.network_address), route.prefixlen))


def plan(detail):
    require(isinstance(detail, dict), "invalid_network_plan")
    allowed = {"addresses", "address", "netmask", "address6", "routes", "excludedRoutes", "dns", "dnsDomains", "searchDomain", "proxyPac", "remote", "mtu", "dnsOptions", "interface"}
    require(not set(detail) - allowed, "unsupported_network_option")
    require(not detail.get("proxyPac"), "proxy_policy_unsupported")
    # OpenVPN's advanced DNS flags require explicit implementation; never silently flatten them.
    require(not detail.get("dnsOptions"), "advanced_dns_policy_unsupported")
    addresses = [ipaddress.ip_interface(value["cidr"]) for value in detail.get("addresses", [])]
    if detail.get("address"):
        addresses.append(ipaddress.ip_interface(detail["address"] + "/" + detail["netmask"]))
    require(not detail.get("address6") and 1 <= len(addresses) <= 8 and all(value.version == 4 for value in addresses), "ipv4_tunnel_addresses_required")
    require(all(not value.ip.is_unspecified and not value.ip.is_loopback and not value.ip.is_multicast for value in addresses), "invalid_tunnel_address")
    routes = [ipaddress.ip_network(value, strict=False) for value in detail.get("routes", [])]
    exclusions = [ipaddress.ip_network(value, strict=False) for value in detail.get("excludedRoutes", [])]
    require(len(routes) <= 1024 and len(exclusions) <= 1024 and all(value.version == 4 for value in routes + exclusions), "ipv4_routes_required")
    remote = ipaddress.ip_address(detail["remote"])
    require(remote.version == 4 and not remote.is_unspecified and not remote.is_multicast, "invalid_transport_peer")
    full_tunnel = any(value.prefixlen == 0 for value in routes)
    holes = [ipaddress.ip_network(str(remote) + "/32"), ipaddress.ip_network("127.0.0.0/8"), ipaddress.ip_network("169.254.0.0/16")]
    if full_tunnel:
        holes += [ipaddress.ip_network("224.0.0.0/4"), ipaddress.ip_network("255.255.255.255/32")]
    routes = subtract(routes, exclusions + holes)
    dns = [ipaddress.ip_address(value) for value in detail.get("dns", [])]
    require(len(dns) <= 8 and all(value.version == 4 and not value.is_unspecified and not value.is_loopback and not value.is_multicast and any(value in route for route in routes) for value in dns), "dns_must_be_routed_inside_tunnel")
    search = detail.get("searchDomain", "").split()
    domains = list(detail.get("dnsDomains", [])) + search
    require(len(domains) <= 64 and all(isinstance(value, str) and len(value) <= 253 and all(re.fullmatch(r"[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?", label) for label in value.rstrip(".").split(".")) for value in domains), "invalid_dns_domain")
    require(not domains or dns, "dns_domains_require_servers")
    mtu = detail.get("mtu", 0) or 1420
    require(type(mtu) is int and 576 <= mtu <= 9000, "invalid_mtu")
    return {"addresses": [str(value) for value in addresses], "routes": [str(value) for value in routes], "dns": [str(value) for value in dns],
            "domains": sorted(set(value.rstrip(".").lower() for value in domains)),
            "searchDomains": list(dict.fromkeys(value.rstrip(".").lower() for value in search)), "remote": str(remote), "mtu": mtu, "fullTunnel": full_tunnel}


class Journal:
    def __init__(self, directory):
        self.directory = trusted(directory, True)
        self.path = self.directory / "owned.json"
        self.lock = os.open(self.directory / "broker.lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        require(os.fstat(self.lock).st_uid == 0 and os.fstat(self.lock).st_mode & 0o077 == 0, "invalid_journal_lock")
        fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        self.entries = {}
        if self.path.exists():
            trusted(self.path)
            require(self.path.stat().st_size <= 2 * 1024 * 1024, "invalid_journal_size")
            self.entries = json.loads(self.path.read_text())
            require(isinstance(self.entries, dict) and len(self.entries) <= 3, "invalid_journal")

    def save(self):
        temporary = self.directory / (".owned-" + uuid.uuid4().hex)
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        try:
            with os.fdopen(descriptor, "wb") as stream:
                stream.write(json.dumps(self.entries, separators=(",", ":")).encode())
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, self.path)
            directory = os.open(self.directory, os.O_DIRECTORY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
        finally:
            temporary.unlink(missing_ok=True)


class LinuxNetwork:
    def __init__(self):
        self.boot = Path("/proc/sys/kernel/random/boot_id").read_text().strip()
        self.ip = str(trusted("/usr/sbin/ip".replace("/usr/sbin", "/usr/bin") if Path("/usr/bin/ip").exists() else "/usr/sbin/ip"))
        self.resolver = "/usr/bin/resolvectl"

    def links(self):
        return json.loads(command(self.ip, "-j", "link", "show"))

    def full_tunnel_underlay(self, network, active_names):
        if not network.get("fullTunnel", False):
            return None, {}
        physical = json.loads(command(self.ip, "-j", "-4", "route", "get", network["remote"]))
        if len(physical) != 1 or physical[0].get("type", "unicast") != "unicast":
            return None, {}
        interface = physical[0].get("dev")
        if not interface or interface in active_names:
            return None, {}
        links = json.loads(command(self.ip, "-j", "-d", "link", "show", "dev", interface))
        if (len(links) != 1 or links[0].get("ifname") != interface or links[0].get("link_type") != "ether"
                or links[0].get("linkinfo", {}).get("info_kind") not in (None, "veth") or "UP" not in links[0].get("flags", [])):
            return None, {}
        addresses = json.loads(command(self.ip, "-j", "-4", "addr", "show", "dev", interface))
        prefixes = {}
        for link in addresses:
            if link.get("ifname") != interface:
                continue
            for address in link.get("addr_info", []):
                if address.get("family") == "inet" and address.get("scope") == "global":
                    value = ipaddress.ip_interface(str(address["local"]) + "/" + str(address["prefixlen"]))
                    if value.network.prefixlen > 0:
                        prefixes.setdefault(value.network, set()).add(str(value.ip))
        return interface, prefixes

    def preflight(self, network, active):
        if network["dns"]:
            require(Path("/etc/resolv.conf").resolve() == Path("/run/systemd/resolve/stub-resolv.conf"), "systemd_resolved_stub_required_for_owned_dns")
            trusted(self.resolver)
            command(self.resolver, "status")
        requested = [ipaddress.ip_network(value) for value in network["routes"]]
        connected = [ipaddress.ip_network(value, strict=False) for value in network["addresses"]]
        active_names = {entry["interface"] for entry in active}
        underlay, prefixes = self.full_tunnel_underlay(network, active_names)
        for route in json.loads(command(self.ip, "-j", "-4", "route", "show", "table", "main")):
            destination = route.get("dst", "default")
            if destination == "default" or route.get("dev") in active_names:
                continue
            existing = ipaddress.ip_network(destination, strict=False)
            kernel_connected = (route.get("dev") == underlay and route.get("protocol") == "kernel" and route.get("scope") == "link"
                                and route.get("type", "unicast") == "unicast" and not route.get("gateway")
                                and route.get("prefsrc") in prefixes.get(existing, set()))
            require(not any(existing.overlaps(value) for value in connected)
                    and not any(existing.overlaps(value) and not (kernel_connected and existing.prefixlen > value.prefixlen) for value in requested), "existing_route_conflict")
            require(not kernel_connected or not any(ipaddress.ip_address(value) in existing for value in network["dns"]), "dns_conflicts_with_physical_underlay")

    def identity(self, interface):
        require(re.fullmatch(r"ttv[0-9a-f]{12}", interface), "invalid_owned_interface")
        matches = [value for value in self.links() if value["ifname"] == interface]
        require(len(matches) == 1 and Path("/sys/class/net", interface, "tun_flags").exists(), "owned_tun_missing")
        require(not matches[0].get("ifalias"), "interface_already_owned")
        return matches[0]["ifindex"]

    def apply(self, entry):
        interface, network = entry["interface"], entry["plan"]
        command(self.ip, "link", "set", interface, "alias", entry["marker"])
        # IPv4-only helpers must not receive kernel-generated IPv6 control traffic.
        ipv6 = Path("/proc/sys/net/ipv6/conf", interface, "disable_ipv6")
        if ipv6.exists():
            ipv6.write_text("1\n")
            require(ipv6.read_text().strip() == "1", "owned_ipv6_disable_failed")
        for address in network["addresses"]:
            command(self.ip, "-4", "addr", "add", address, "dev", interface)
        command(self.ip, "link", "set", interface, "mtu", network["mtu"], "up")
        for route in network["routes"]:
            command(self.ip, "-4", "route", "add", route, "dev", interface, "proto", "196", "metric", "42760")
        if network["dns"]:
            command(self.resolver, "dns", interface, *network["dns"])
            search = network.get("searchDomains", [])
            domains = search + ["~" + domain for domain in network["domains"] if domain not in search]
            command(self.resolver, "domain", interface, *(domains or ["~."]))
            command(self.resolver, "default-route", interface, "no" if network["domains"] else "yes")

    def cleanup(self, entry):
        if entry["boot"] != self.boot:
            return
        matches = [value for value in self.links() if value["ifindex"] == entry["index"]]
        if not matches:
            return
        link = matches[0]
        require(link["ifname"] == entry["interface"] and link.get("ifalias") == entry["marker"], "owned_interface_identity_changed")
        # Only this nonpersistent TUN, verified by boot/index/name/marker, is removed.
        if entry["plan"]["dns"]:
            command(self.resolver, "revert", entry["interface"])
        command(self.ip, "link", "delete", "dev", entry["interface"])


class DarwinNetwork:
    def __init__(self, helper):
        self.helper = helper
        self.boot = command("/usr/sbin/sysctl", "-n", "kern.boottime").decode().strip()

    def links(self):
        return [{"ifindex": index, "ifname": name} for index, name in socket.if_nameindex()]

    def physical(self, peer):
        response = command("/sbin/route", "-n", "get", "-inet", peer).decode()
        matches = re.findall(r"^\s*interface:\s*(\S+)\s*$", response, re.MULTILINE)
        require(len(matches) == 1, "transport_route_missing")
        return matches[0]

    def preflight(self, network, active):
        requested = [ipaddress.ip_network(value) for value in network["routes"]]
        requested += [ipaddress.ip_network(value, strict=False) for value in network["addresses"]]
        owned = {entry["interface"] for entry in active}
        # shortcut: netstat flags do not prove a physical connected route; keep overlaps blocked until native provenance and Mac acceptance exist.
        for row in command("/usr/sbin/netstat", "-rn", "-f", "inet").decode().splitlines():
            fields = row.split()
            if len(fields) < 4 or fields[0] == "default" or not fields[0][0].isdigit() or fields[3] in owned:
                continue
            destination, _, prefix = fields[0].partition("/")
            components = destination.split(".")
            if len(components) < 4:
                destination = ".".join(components + ["0"] * (4 - len(components)))
            existing = ipaddress.ip_network(destination + "/" + (prefix or str(len(components) * 8)), strict=False)
            require(not any(existing.overlaps(value) for value in requested), "existing_route_conflict")

    def identity(self, interface):
        require(re.fullmatch(r"utun[0-9]{1,10}", interface), "invalid_owned_utun")
        return socket.if_nametoindex(interface)

    def create_tun(self):
        owner, child = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
        process = subprocess.Popen([str(self.helper), "create-tun", str(child.fileno())], pass_fds=[child.fileno()], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        child.close()
        owner.settimeout(5)
        try:
            name, ancillary, flags, _ = owner.recvmsg(64, socket.CMSG_SPACE(4))
            require(flags == 0 and len(ancillary) == 1 and ancillary[0][:2] == (socket.SOL_SOCKET, socket.SCM_RIGHTS) and len(ancillary[0][2]) == 4, "invalid_utun_handoff")
            descriptor = struct.unpack("i", ancillary[0][2])[0]
            require(process.wait(5) == 0, "utun_helper_failed")
            os.set_inheritable(descriptor, False)
            return descriptor, name.decode("ascii")
        finally:
            owner.close()
            if process.poll() is None:
                process.kill()
                process.wait()

    def apply(self, entry):
        interface, network = entry["interface"], entry["plan"]
        for address in network["addresses"]:
            assigned = ipaddress.ip_interface(address)
            command("/sbin/ifconfig", interface, "inet", assigned.ip, assigned.ip, "netmask", assigned.netmask, "alias")
        command("/sbin/ifconfig", interface, "mtu", network["mtu"], "up")
        for route in network["routes"]:
            command("/sbin/route", "-n", "add", "-inet", "-net", route, "-interface", interface)
        if network["dns"]:
            command(str(self.helper), "dns-set", interface, entry["id"], input=json.dumps({"interface": interface, "dns": network["dns"], "domains": network["domains"], "searchDomains": network.get("searchDomains", [])}).encode())

    def cleanup(self, entry):
        # A disappeared utun removes its routes. Never delete a potentially reused interface.
        if entry["boot"] == self.boot:
            deadline = time.monotonic() + 3
            while any(link["ifindex"] == entry["index"] and link["ifname"] == entry["interface"] for link in self.links()):
                require(time.monotonic() < deadline, "utun_still_owned_or_identity_ambiguous")
                time.sleep(.05)
        network = entry["plan"]
        if network["dns"]:
            command(str(self.helper), "dns-remove", entry["interface"], entry["id"], input=json.dumps({"interface": entry["interface"], "dns": network["dns"], "domains": network["domains"], "searchDomains": network.get("searchDomains", [])}).encode())


class NativeBroker(lab.Broker):
    def __init__(self, config):
        self.config = config
        self.engines = trusted(config["engines"], True)
        for name in ("termterm-openconnect", "termterm-openvpn", "termterm-wireguard"):
            binary = pin(self.engines / name, config["pins"][name])
            verify_loader_paths(binary)
        policy = pin(self.engines / "vpn-policy-lab", config["pins"]["vpn-policy-lab"])
        ca_file = trusted(config["caFile"]) if config.get("caFile") else None
        super().__init__(self.engines, policy, ca_file)
        self.journal = Journal(config["journal"])
        if sys.platform == "darwin":
            helper = pin(self.engines / "termterm-vpn-macos", config["pins"]["termterm-vpn-macos"])
            self.network = DarwinNetwork(helper)
        else:
            require(sys.platform == "linux", "unsupported_native_platform")
            self.network = LinuxNetwork()
        for identity, entry in list(self.journal.entries.items()):
            canonical_uuid(identity)
            self.network.cleanup(entry)
            del self.journal.entries[identity]
            self.journal.save()
        self.session_type = NativeSession
        self.families = ("openVpn", "fortinet", "globalProtect", "cisco", "wireGuard")

    def hello(self):
        return {"version": 2, "labOnly": False, "mode": "authenticatedNative", "identity": self.config.get("applicationIdentifier", "local.termterm.desktop.vpndev") + ".broker", "maxConnections": 3, "networkLease": True, "quiesce": True}


class NativeSession(lab.Session):
    def __init__(self, *args):
        super().__init__(*args)
        self.network_plan = None
        self.entry = None
        self.capability = None
        self.packet = None
        self.tun = None
        self.pump = None
        self.quiesced = threading.Event()
        self.stop_started = False
        self.transport_peers = []

    def emit(self, state, **extra):
        if state == "connected":
            require(self.entry is not None, "network_lease_missing")
            self.owner.send({"kind": "networkLease", "id": self.public["id"], "generation": self.public["generation"],
                             "lease": {"id": self.public["id"], "generation": self.public["generation"], "interfaceIndex": self.entry["index"],
                                       "interfaceName": self.interface, "addresses": self.network_plan["addresses"], "addressFamilies": ["ipv4"]}})
        super().emit(state, **extra)

    def request_quiesce(self):
        if self.entry is None or self.owner.dead():
            return
        self.owner.send({"kind": "quiesceRequired", "id": self.public["id"], "generation": self.public["generation"]})
        require(self.quiesced.wait(10) or self.owner.dead(), "dependent_socket_cleanup_timeout")

    def stop(self):
        if self.stop_started:
            return
        self.stop_started = True
        def stop_owned():
            try:
                self.request_quiesce()
                super(NativeSession, self).stop()
            except ValueError:
                self.broker.failed = True
                self.emit("disconnecting", error="dependent_socket_cleanup_timeout")
        threading.Thread(target=stop_owned, daemon=True).start()

    def initial(self):
        if self.profile["family"] == "wireGuard":
            response = self.broker.policy_call({"action": "wireGuardConfig", "profile": self.profile})
            self.transport_peers = [endpoint.rsplit(":", 1)[0] for endpoint in response["result"]["endpoints"]]
            self.packet, self.engine_packet = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
            return "termterm-wireguard", {"sessionId": self.public["id"], "generation": self.public["generation"], "wireguard": response["result"]}
        engine, initial = super().initial()
        initial.update(sessionId=self.public["id"], generation=self.public["generation"])
        return engine, initial

    def launch(self, engine):
        self.capability, engine_capability = socket.socketpair()
        environment = {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "TERMTERM_VPN_BROKER_FD": str(engine_capability.fileno())}
        descriptors = [engine_capability.fileno()]
        if self.packet:
            descriptors.append(self.engine_packet.fileno())
        try:
            process = subprocess.Popen([str(self.broker.engines / engine)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                       env=environment, pass_fds=descriptors)
            self.capability.sendall(CAPABILITY)
            return process
        finally:
            engine_capability.close()

    def reserve(self, detail):
        require(self.network_plan is None, "duplicate_network_plan")
        network = plan(detail)
        peers = self.transport_peers or [network["remote"]]
        active = [session.entry for session in self.broker.active.values() if session.entry]
        for session in self.broker.active.values():
            if session is self or session.network_plan is None:
                continue
            other = session.network_plan
            other_peers = session.transport_peers or [other["remote"]]
            require(not any(ipaddress.ip_address(peer) in ipaddress.ip_network(route) for peer in other_peers for route in network["routes"])
                    and not any(ipaddress.ip_address(peer) in ipaddress.ip_network(route) for peer in peers for route in other["routes"]), "transport_peer_route_conflict")
        for peer in peers:
            if sys.platform == "linux":
                physical = json.loads(command(self.broker.network.ip, "-j", "-4", "route", "get", peer))
                require(physical and physical[0].get("dev") not in {entry["interface"] for entry in active}, "transport_peer_uses_other_vpn")
            else:
                require(self.broker.network.physical(peer) not in {entry["interface"] for entry in active}, "transport_peer_uses_other_vpn")
        self.broker.network.preflight(network, active)
        self.broker.policy_call({"action": "reserve", "id": self.public["id"], "generation": self.public["generation"], "splitDns": True,
                                 "plan": {"routes": network["routes"] + [str(ipaddress.ip_network(value, strict=False)) for value in network["addresses"]],
                                          "dnsServers": network["dns"], "dnsDomains": network["domains"]}})
        self.reserved = True
        self.network_plan = network

    def configure(self, detail):
        require(self.reserved and plan(detail) == self.network_plan, "network_plan_changed_after_approval")
        if self.profile["family"] == "wireGuard":
            if sys.platform == "darwin":
                self.tun, self.interface = self.broker.network.create_tun()
            else:
                self.tun = os.open("/dev/net/tun", os.O_RDWR | os.O_CLOEXEC)
                fcntl.ioctl(self.tun, 0x400454CA, struct.pack("16sH", self.interface.encode(), 0x1001 | 0x8000))
        else:
            if sys.platform == "darwin":
                self.interface = detail.get("interface", "")
                require(re.fullmatch(r"utun[0-9]{1,10}", self.interface), "interface_mismatch")
            else:
                require(detail.get("interface") == self.interface, "interface_mismatch")
        index = self.broker.network.identity(self.interface)
        self.entry = {"id": self.public["id"], "interface": self.interface, "index": index, "boot": self.broker.network.boot,
                      "marker": self.broker.config.get("applicationIdentifier", "local.termterm.desktop.vpndev") + ":" + self.public["id"], "plan": self.network_plan}
        # Intent is durable before the first host-network mutation.
        self.broker.journal.entries[self.public["id"]] = self.entry
        self.broker.journal.save()
        self.broker.network.apply(self.entry)
        if self.packet:
            self.pump = threading.Thread(target=self.pump_packets, daemon=True)
            self.pump.start()

    def configured(self):
        result = {"action": "network-configured"}
        if self.packet:
            result["packetFd"] = self.engine_packet.fileno()
        return result

    def write(self, value):
        value.update(sessionId=self.public["id"], generation=self.public["generation"])
        super().write(value)

    def pump_packets(self):
        try:
            while self.process.poll() is None and not self.cancelled:
                ready, _, _ = select.select([self.tun, self.packet], [], [], 0.2)
                for source in ready:
                    packet = os.read(self.tun, 65536) if source == self.tun else self.packet.recv(65536)
                    if sys.platform == "darwin" and source == self.tun:
                        require(packet[:4] == struct.pack("!I", socket.AF_INET), "ipv4_utun_prefix_required")
                        packet = packet[4:]
                    require(packet and packet[0] >> 4 == 4, "ipv4_packet_required")
                    if source == self.tun:
                        require(self.packet.send(packet) == len(packet), "packet_write_failed")
                    else:
                        framed = struct.pack("!I", socket.AF_INET) + packet if sys.platform == "darwin" else packet
                        require(os.write(self.tun, framed) == len(framed), "tun_write_failed")
        except (OSError, ValueError):
            self.stop()

    def cleanup(self):
        self.request_quiesce()
        if self.capability:
            self.capability.close()
        if self.tun is not None:
            os.close(self.tun)
            self.tun = None
        if self.pump:
            self.pump.join(timeout=1)
        if self.packet:
            self.packet.close()
            self.engine_packet.close()
        if self.entry:
            self.broker.network.cleanup(self.entry)
            del self.broker.journal.entries[self.public["id"]]
            self.broker.journal.save()


class MacPeer:
    def __init__(self, process):
        self.process = process

    def fileno(self):
        return self.process.stdout.fileno()

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
        self.process.wait(timeout=3)
        self.process.stdout.close()


def authenticate(connection, config):
    if sys.platform == "darwin":
        helper = pin(Path(config["engines"]) / "termterm-vpn-macos", config["pins"]["termterm-vpn-macos"])
        require(isinstance(config.get("clientRequirement"), str) and 1 <= len(config["clientRequirement"]) <= 4096, "signed_client_requirement_required")
        process = subprocess.Popen([str(helper), "authenticate", str(connection.fileno()), str(config["uid"]), config["client"], config["clientRequirement"]],
                                   pass_fds=[connection.fileno()], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        try:
            require(select.select([process.stdout], [], [], 5)[0] and process.stdout.readline(32) == b"authenticated\n", "client_signature_rejected")
            return MacPeer(process)
        except Exception:
            process.kill()
            process.wait(timeout=3)
            process.stdout.close()
            raise
    require(sys.platform == "linux", "unsupported_native_platform")
    pid, uid, _ = struct.unpack("3i", connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
    require(uid == config["uid"] and pid > 1, "client_identity_rejected")
    # pidfd remains held for the entire owner session; recycled PIDs cannot acquire it.
    pidfd = os.pidfd_open(pid)
    try:
        before = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19]
        expected = pin(config["client"], config["clientSha256"])
        require(Path(f"/proc/{pid}/exe").resolve() == expected, "client_executable_rejected")
        with open(f"/proc/{pid}/exe", "rb") as stream:
            require(sha256(stream) == config["clientSha256"], "client_process_pin_rejected")
        require(before == Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19] and not select.select([pidfd], [], [], 0)[0], "client_process_changed")
        return pidfd
    except Exception:
        os.close(pidfd)
        raise


def serve(config):
    pin(config["client"], config["clientSha256"])
    require(type(config["uid"]) is int and config["uid"] > 0, "nonroot_client_uid_required")
    broker = NativeBroker(config)
    socket_path = Path(SOCKET)
    trusted(socket_path.parent, True)
    if socket_path.exists() or socket_path.is_symlink():
        metadata = socket_path.lstat()
        require(stat.S_ISSOCK(metadata.st_mode) and metadata.st_uid == config["uid"], "foreign_broker_socket_rejected")
        with socket.socket(socket.AF_UNIX) as probe:
            try:
                probe.connect(SOCKET)
            except OSError as error:
                require(error.errno == errno.ECONNREFUSED, "broker_socket_state_unknown")
            else:
                raise ValueError("broker_already_running")
        socket_path.unlink()
    listener = socket.socket(socket.AF_UNIX)
    owners = []
    try:
        listener.bind(SOCKET)
        os.chmod(SOCKET, 0o600)
        os.chown(SOCKET, config["uid"], -1)
        listener.listen(4)
        # Python runs signal handlers on the main thread even when an owner receives SIGTERM.
        listener.settimeout(0.5)
        signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
        while True:
            try:
                connection, _ = listener.accept()
            except socket.timeout:
                continue
            try:
                require(len([owner for owner, thread in owners if thread.is_alive()]) < 4, "too_many_clients")
                pidfd = authenticate(connection, config)
            except Exception:
                connection.close()
                continue
            connection.setsockopt(socket.SOL_SOCKET, socket.SO_SNDTIMEO, struct.pack("ll", 2, 0))
            owner = NativeOwner(connection, broker)
            owner.pidfd = pidfd
            def run_owner(owner=owner, pidfd=pidfd):
                try:
                    owner.run()
                finally:
                    if isinstance(pidfd, int):
                        os.close(pidfd)
                    else:
                        pidfd.close()
            thread = threading.Thread(target=run_owner, daemon=True)
            owners = [(owner, thread) for owner, thread in owners if thread.is_alive()]
            owners.append((owner, thread))
            thread.start()
    finally:
        listener.close()
        for owner, _ in owners:
            try:
                owner.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        for _, thread in owners:
            thread.join(timeout=12)
        broker.policy.terminate()
        broker.policy.wait(timeout=3)
        socket_path.unlink(missing_ok=True)


class NativeOwner(lab.Owner):
    def dead(self):
        return bool(select.select([self.pidfd], [], [], 0)[0])

    def run(self):
        closing = False
        try:
            self.connection.settimeout(10)
            with self.connection.makefile("rb") as stream:
                while True:
                    request = frame(stream)
                    action = request.get("action")
                    fields = {"hello": {"vaultId"}, "connect": {"id", "generation", "profileId", "profile"}, "answer": {"id", "generation", "challengeId", "values"},
                              "ssoAnswer": {"id", "generation", "challengeId", "result"}, "disconnect": {"id", "generation"}, "quiesced": {"id", "generation"}, "sessions": set(), "close": set()}
                    require(action in fields and set(request) == fields[action] | {"action", "requestId"}, "invalid_request_fields")
                    require(type(request["requestId"]) is int and 0 <= request["requestId"] < 2 ** 53, "invalid_request_id")
                    result, launch = None, None
                    try:
                        if action == "hello":
                            require(self.vault_id is None, "duplicate_handshake")
                            self.vault_id = canonical_uuid(request["vaultId"])
                            self.connection.settimeout(None)
                            result = self.broker.hello()
                        else:
                            require(self.vault_id is not None, "handshake_required")
                            require(not closing or action == "quiesced", "owner_is_closing")
                            if "id" in request:
                                canonical_uuid(request["id"])
                                require(type(request["generation"]) is int and 0 < request["generation"] < 2 ** 53, "invalid_generation")
                            if action == "connect":
                                canonical_uuid(request["profileId"])
                                self.sessions = {key: session for key, session in self.sessions.items() if session.thread.is_alive()}
                                launch = self.broker.start(self, request)
                                result = launch.public.copy()
                            elif action in ("disconnect", "answer", "ssoAnswer", "quiesced"):
                                session = self.sessions.get(request["id"])
                                require(session and request["generation"] == session.public["generation"] and session.public["state"] not in ("failed", "disconnected"), "stale_session")
                                if action == "quiesced":
                                    session.quiesced.set()
                                elif action == "disconnect":
                                    session.stop()
                                elif action == "answer":
                                    session.answer(request)
                                else:
                                    session.sso_answer(request)
                            elif action == "sessions":
                                result = [session.public.copy() for session in self.sessions.values()]
                            elif action == "close":
                                closing = True
                                for session in self.sessions.values():
                                    if session.thread.is_alive():
                                        session.stop()
                                def close_after_cleanup():
                                    for session in self.sessions.values():
                                        if session.thread.is_alive():
                                            session.thread.join(timeout=12)
                                    if not any(session.thread.is_alive() for session in self.sessions.values()):
                                        self.connection.shutdown(socket.SHUT_RDWR)
                                threading.Thread(target=close_after_cleanup, daemon=True).start()
                        self.send({"kind": "reply", "requestId": request["requestId"], "ok": True, "result": result})
                        if launch:
                            launch.thread.start()
                    except Exception:
                        # Never echo raw profile material or authentication errors across this boundary.
                        self.send({"kind": "reply", "requestId": request["requestId"], "ok": False, "error": "vpn_request_rejected"})
        except (EOFError, OSError, ValueError, KeyError, TypeError):
            pass
        finally:
            self.closed.set()
            # Control EOF is not proof that the authenticated application's TCP sockets died.
            while not self.dead() and any(session.thread.is_alive() for session in self.sessions.values()):
                select.select([self.pidfd], [], [], 0.2)
            for session in self.sessions.values():
                if session.thread.is_alive():
                    session.stop_started = False
                    session.stop()
            for session in self.sessions.values():
                if session.thread.is_alive():
                    session.thread.join(timeout=10)
            self.connection.close()


def main():
    global SOCKET
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", choices=["0.4.4"], help="Use the fixed production candidate identity and installation paths")
    parser.add_argument("--check", action="store_true", help="Verify immutable installed inputs without listening or network changes")
    arguments = parser.parse_args()
    require(os.geteuid() == 0, "root_service_required")
    trusted(Path(__file__).resolve())
    trusted(Path(__file__).with_name("vpn-lab-broker.py").resolve())
    selected = contract(arguments.candidate)
    SOCKET = selected["socket"]
    config_path = trusted(selected["config"])
    require(config_path.stat().st_size <= 65536, "configuration_too_large")
    config = json.loads(config_path.read_text())
    validate_configuration(config, arguments.candidate)
    if sys.platform == "darwin":
        require({"python", "pythonSha256"} <= set(config), "configured_python_runtime_required")
        require(Path(sys.executable).resolve() == pin(config["python"], config["pythonSha256"]), "configured_python_runtime_mismatch")
    pin(config["client"], config["clientSha256"])
    for name, digest in config["pins"].items():
        require(name in {"vpn-policy-lab", "termterm-openvpn", "termterm-openconnect", "termterm-wireguard", "termterm-vpn-macos"}, "unknown_helper")
        pin(Path(config["engines"]) / name, digest)
    if not arguments.check:
        serve(config)


if __name__ == "__main__":
    main()
