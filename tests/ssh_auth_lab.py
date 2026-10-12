"""Disposable loopback peer; uses independent SSH protocol implementation.

python -m venv .tools/auth-venv
.tools/auth-venv/Scripts/python -m pip install -r tests/ssh-auth-requirements.txt
Not a production OTP provider. No arbitrary command execution or network relay.
"""
import base64
import json
import logging
import select
import socket
import sys
import threading
import time
from cryptography.hazmat.primitives.serialization import SSHCertificateBuilder, SSHCertificateType
import paramiko as p

logging.getLogger("paramiko").addHandler(logging.NullHandler())
PASSWORD = "Disposable-Auth-Password-2026!"
KEY = p.RSAKey.from_private_key_file(sys.argv[1])
STOP = threading.Event()
TRANSPORTS = []


class Peer(p.ServerInterface):
    def __init__(self):
        self.first = False
        self.round = 0
        self.relays = set()

    def get_allowed_auths(self, user):
        if self.first:
            return "password" if user == "key_password" else "keyboard-interactive"
        return "publickey,password,keyboard-interactive"

    def accepted(self, user):
        if user.endswith("_mfa") or user == "key_password":
            self.first = True
            return p.AUTH_PARTIALLY_SUCCESSFUL
        return p.AUTH_SUCCESSFUL

    def check_auth_publickey(self, user, key):
        if user == "cert_mfa":
            blob = getattr(key, "public_blob", None)
            if blob and blob.key_blob == CERTIFICATE:
                return self.accepted(user)
        elif user in ("key", "key_mfa", "key_password") and key == KEY:
            return self.accepted(user)
        return p.AUTH_FAILED

    def check_auth_password(self, user, password):
        if password != PASSWORD:
            return p.AUTH_FAILED
        if user == "key_password":
            return p.AUTH_SUCCESSFUL if self.first else p.AUTH_FAILED
        return self.accepted(user) if user in ("password", "password_mfa") else p.AUTH_FAILED

    def query(self, prompt):
        return p.InteractiveQuery("Disposable laboratory MFA", "Protocol challenge test", (prompt, False))

    def check_auth_interactive(self, user, submethods):
        if not self.first and user != "interactive":
            return p.AUTH_FAILED
        self.round = 1
        return self.query("One-time code")

    def check_auth_interactive_response(self, answers):
        if answers != (["654321"] if self.round == 1 else ["confirm"]):
            return p.AUTH_FAILED
        if self.round == 2:
            return p.AUTH_SUCCESSFUL
        self.round = 2
        return self.query("Confirmation")

    def check_channel_request(self, kind, chanid):
        return p.OPEN_SUCCEEDED if kind == "session" else p.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

    def check_channel_pty_request(self, *args):
        return True

    def check_channel_shell_request(self, channel):
        channel.sendall(b"TT_AUTHENTICATED\r\n")
        return True

    def check_channel_direct_tcpip_request(self, chanid, origin, destination):
        if destination != ("127.0.0.1", PORT):
            return p.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED
        self.relays.add(chanid)
        return p.OPEN_SUCCEEDED


def channel_loop(channel, relay):
    remote = None
    try:
        if relay:
            remote = socket.create_connection(("127.0.0.1", PORT), timeout=5)
        while not STOP.is_set() and not channel.closed:
            readers = [channel, remote] if remote else [channel]
            for source in select.select(readers, [], [], .2)[0]:
                data = source.recv(32768)
                if not data:
                    return
                (remote if source is channel and remote else channel).sendall(data)
    except (OSError, EOFError, p.SSHException):
        pass
    finally:
        channel.close()
        if remote:
            remote.close()


def serve(sock):
    transport = p.Transport(sock)
    TRANSPORTS.append(transport)
    try:
        transport.add_server_key(HOST)
        peer = Peer()
        transport.start_server(server=peer)
        while not STOP.is_set() and transport.is_active():
            channel = transport.accept(.2)
            if channel:
                threading.Thread(target=channel_loop, args=(channel, channel.get_id() in peer.relays), daemon=True).start()
    except (OSError, EOFError, p.SSHException):
        pass
    finally:
        transport.close()


ca = p.RSAKey.generate(2048)
now = int(time.time())
cert = (SSHCertificateBuilder().public_key(KEY.key.public_key()).serial(1)
        .type(SSHCertificateType.USER).key_id(b"disposable").valid_principals([b"cert_mfa"])
        .valid_after(now - 60).valid_before(now + 3600).sign(ca.key).public_bytes().decode("ascii"))
CERTIFICATE = base64.b64decode(cert.split()[1])
HOST = p.RSAKey.generate(2048)
with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    listener.listen(16)
    listener.settimeout(.2)
    PORT = listener.getsockname()[1]
    print(json.dumps({"port": PORT, "certificate": cert}), flush=True)
    def stopped():
        sys.stdin.buffer.read(1)
        STOP.set()
    threading.Thread(target=stopped, daemon=True).start()
    while not STOP.is_set():
        try:
            sock, _ = listener.accept()
            threading.Thread(target=serve, args=(sock,), daemon=True).start()
        except socket.timeout:
            pass
    for transport in TRANSPORTS:
        transport.close()
