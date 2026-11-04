#!/usr/bin/env python3
"""Disposable WSL kernel-WireGuard/HTTPS/SSH fixture for the native Windows example."""
import argparse
import hashlib
import http.client
import importlib.util
import ipaddress
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import socketserver
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import uuid

import team_vpn_ssh as lab

ROOT = Path(__file__).resolve().parents[1]


def destination(path, root=ROOT):
    path = path.resolve()
    if (root / '.lab').is_symlink() or not path.is_relative_to((root / '.lab').resolve()) or path.exists():
        raise ValueError('Use a new ignored .lab fixture directory; preserve existing fixtures')
    return path


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
    path.chmod(0o600)


def fixture(args):
    assert os.geteuid() == 0, 'Disposable root WSL lab required'
    directory = destination(args.directory)
    assert 120 <= args.duration <= 14400
    assert not lab.run('ss', '-Hlun', 'sport = :31820').stdout.strip(), 'Preserve existing UDP listener'
    directory.mkdir(parents=True, mode=0o700)
    before = lab.snapshot()
    identity = uuid.uuid4().hex[:16]
    namespace, birth = 'ttteam-win-' + identity[:10], 'tttw' + identity[:10]
    stage = Path(tempfile.mkdtemp(prefix='termterm-team-win-', dir='/run')); stage.chmod(0o755)
    case = lab.PRIVATE / ('windows-ssh-' + identity); case.mkdir(mode=0o700)
    db = 'termterm_win_vpn_' + identity
    sources = ['tests/team_vpn_windows.py', 'tests/team_vpn_ssh.py', 'src-tauri/examples/team_vpn_e2e.rs',
               'scripts/wireguard-windows-relay.py', 'scripts/vpn_lab_json.py', 'team-client/src/desktop.rs', 'src-tauri/src/team_v2/terminal.rs']
    pins = {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in sources}
    spec = importlib.util.spec_from_file_location('team_windows_relay', ROOT / 'scripts/wireguard-windows-relay.py')
    sys.path.insert(0, str(ROOT / 'scripts'))
    relay_module = importlib.util.module_from_spec(spec); spec.loader.exec_module(relay_module)
    processes, streams = [], []
    namespace_created = interface_created = moved = database_created = False
    https = https_thread = None
    report = {'version': 1, 'fixtureId': identity, 'result': 'failed', 'scope': 'WSL fixture only; Windows acceptance is separate'}

    def sql(database, query):
        return lab.run('runuser', '-u', 'postgres', '--', 'psql', '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-p', '55432', '-d', database, input=query.encode()).stdout.decode().strip()

    def ns(*command, **kwargs): return lab.run('ip', 'netns', 'exec', namespace, *command, **kwargs)

    def start(name, command):
        stream = (case / (name + '.log')).open('wb'); streams.append(stream)
        process = subprocess.Popen(list(map(str, command)), stdout=stream, stderr=stream)
        processes.append(process); return process

    def wait(check, label):
        deadline = time.monotonic() + 15
        while not check():
            assert all(p.poll() is None for p in processes) and time.monotonic() < deadline, 'Fixture readiness failed: ' + label
            time.sleep(.05)

    def port():
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0)); return listener.getsockname()[1]

    try:
        uid = int(lab.run('id', '-u', 'termterm_test').stdout)
        gid = int(lab.run('id', '-g', 'termterm_test').stdout)
        client = directory / 'client'; client.mkdir(mode=0o700)
        (client / 'data').mkdir(mode=0o700)
        (client / 'payload').write_bytes(secrets.token_bytes(65536))
        remote = stage / 'remote'; remote.mkdir(mode=0o700); os.chown(remote, uid, gid)
        (remote / 'large').write_bytes(secrets.token_bytes(65536) * 512)
        os.chown(remote / 'large', uid, gid)
        def keys():
            private = lab.run('wg', 'genkey').stdout.strip()
            return private, lab.run('wg', 'pubkey', input=private + b'\n').stdout.strip()
        private, public = keys(); client_private, client_public = keys()
        addresses = json.loads(lab.run('ip', '-4', '-j', 'address', 'show', 'dev', 'eth0').stdout)[0]['addr_info']
        address = next(value['local'] for value in addresses if value['scope'] == 'global')
        assert any(ipaddress.ip_address(address) in ipaddress.ip_network(prefix) for prefix in ['10.0.0.0/8', '172.16.0.0/12', '192.168.0.0/16'])
        lab.run('ip', 'netns', 'add', namespace); namespace_created = True
        # The kernel UDP socket stays on WSL; cleartext SSH exists only in this namespace.
        lab.run('ip', 'link', 'add', birth, 'type', 'wireguard'); interface_created = True
        lab.run('ip', 'link', 'set', birth, 'netns', namespace); moved = True
        ns('ip', 'link', 'set', birth, 'name', 'ttwg0')
        ns('wg', 'setconf', 'ttwg0', '/dev/stdin', input=b'[Interface]\nPrivateKey = ' + private + b'\nListenPort = 31820\n[Peer]\nPublicKey = ' + client_public + b'\nAllowedIPs = 10.97.0.2/32\n')
        ns('ip', 'link', 'set', 'lo', 'up')
        ns('ip', 'address', 'add', '10.97.0.1/32', 'dev', 'ttwg0')
        ns('ip', 'link', 'set', 'ttwg0', 'up')
        ns('ip', 'route', 'add', '10.97.0.2/32', 'dev', 'ttwg0')
        lab.run('ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', stage / 'hostkey')
        (stage / 'authorized_keys').write_bytes(lab.run('ssh-keygen', '-y', '-f', ROOT / 'tests/ssh-keys/rsa2048.pkcs1').stdout)
        (stage / 'authorized_keys').chmod(0o644)
        (stage / 'sshd_config').write_text(f'Port 41843\nListenAddress 10.97.0.1\nHostKey {stage}/hostkey\nPidFile {stage}/sshd.pid\nAuthorizedKeysFile {stage}/authorized_keys\nAllowUsers termterm_test\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nPermitRootLogin no\nUsePAM no\nSubsystem sftp internal-sftp\n')
        lab.run('/usr/sbin/sshd', '-t', '-f', stage / 'sshd_config')
        start('sshd', ['ip', 'netns', 'exec', namespace, '/usr/sbin/sshd', '-D', '-e', '-f', stage / 'sshd_config'])
        wait(lambda: b':41843' in ns('ss', '-ltn').stdout, 'SSH')
        assert len(json.loads(ns('ip', '-j', 'link').stdout)) == 2, 'Cleartext namespace acquired another interface'
        passwords = json.loads((lab.PRIVATE / 'roles.json').read_text())
        sql('postgres', f'CREATE DATABASE {db} OWNER {lab.OWNER}'); database_created = True
        sql(db, f'REVOKE ALL ON DATABASE {db} FROM PUBLIC;GRANT CONNECT ON DATABASE {db} TO {lab.SERVICE}')
        pgpass = case / 'pgpass'; pgpass.write_text(f'localhost:55432:*:{lab.OWNER}:{passwords[lab.OWNER]}\n'); pgpass.chmod(0o600)
        ca = '/etc/postgresql/18/termterm/tls/ca.crt'
        env = {**os.environ, 'PGHOST': 'localhost', 'PGPORT': '55432', 'PGDATABASE': db, 'PGUSER': lab.OWNER, 'PGSSLROOTCERT': ca, 'PGPASSFILE': str(pgpass)}
        lab.run('python3', ROOT / 'team-service/deploy/manage.py', 'migrate', '--service-role', lab.SERVICE, env=env)
        bootstrap = json.loads(sql(db, f"SET ROLE {lab.OWNER};SELECT termterm_team_v2.bootstrap_team('Disposable Windows VPN','leader','Windows VPN leader')"))
        backend = port()
        config = case / 'service.json'; write_json(config, {'listen': f'127.0.0.1:{backend}', 'database': {'host': 'localhost', 'port': 55432, 'database': db, 'username': lab.SERVICE, 'password': passwords[lab.SERVICE], 'caPath': ca}})
        service = start('service', [args.service, config])
        def ready():
            try:
                connection = http.client.HTTPConnection('127.0.0.1', backend, timeout=.2)
                connection.request('GET', '/ready'); response = connection.getresponse()
                ok = response.status == 200; response.read(); connection.close(); return ok
            except OSError: return False
        wait(ready, 'Team service')
        lab.run('openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', stage / 'https.key', '-out', stage / 'https.crt', '-days', '2', '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost')
        class Handler(socketserver.BaseRequestHandler):
            def handle(self): lab.bridge(self.request, socket.create_connection(('127.0.0.1', backend), timeout=25))
        https = socketserver.ThreadingTCPServer(('127.0.0.1', 0), Handler); https.daemon_threads = True
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); tls.load_cert_chain(stage / 'https.crt', stage / 'https.key')
        https.socket = tls.wrap_socket(https.socket, server_side=True)
        https_thread = threading.Thread(target=https.serve_forever, daemon=True); https_thread.start()
        vpn = {'version': 1, 'label': 'Disposable Windows Team WireGuard', 'family': 'wireGuard', 'source': 'fixture', 'server': '', 'groupId': None, 'username': '', 'password': '', 'realm': '', 'remember': False,
               'config': f'[Interface]\nPrivateKey = {client_private.decode()}\nAddress = 10.97.0.2/32\nMTU = 1420\n[Peer]\nPublicKey = {public.decode()}\nEndpoint = 127.0.0.1:31820\nAllowedIPs = 10.97.0.1/32\nPersistentKeepalive = 1\n'}
        windows_path = lambda path: lab.run('wslpath', '-w', path).stdout.decode().strip()
        fixture_data = {'version': 1, 'fixtureId': identity, 'profile': {'serviceUrl': f'https://localhost:{https.server_address[1]}', 'teamId': bootstrap['teamId'], 'caPem': (stage / 'https.crt').read_text()},
                        'username': 'leader', 'password': bootstrap['temporaryPassword'], 'vpn': vpn,
                        'sshKey': (ROOT / 'tests/ssh-keys/rsa2048.pkcs1').read_text(), 'hostFingerprint': lab.run('ssh-keygen', '-lf', stage / 'hostkey.pub').stdout.decode().split()[1],
                        'root': windows_path(directory), 'remoteRoot': str(remote), 'sharedRevocation': args.mode == 'shared'}
        write_json(directory / 'fixture.json', fixture_data)
        # Reuse the existing loopback-only Windows ciphertext relay without a new control protocol.
        write_json(directory / 'profiles.json', {'fixtureId': identity, 'kernelPeer': {'address': address, 'port': 31820}})
        write_json(directory / 'ready.json', {'version': 1, 'fixtureId': identity, 'pid': os.getpid(), 'namespace': namespace, 'peerPort': 31820, 'durationSeconds': args.duration, 'sourceSha256': pins})
        print('TEAM_VPN_WINDOWS_FIXTURE_READY', flush=True)
        deadline = time.monotonic() + args.duration
        while time.monotonic() < deadline and not (directory / 'stop').exists():
            assert service.poll() is None and all(p.poll() is None for p in processes), 'Private fixture process exited'
            handshake = ns('wg', 'show', 'ttwg0', 'latest-handshakes').stdout.split()
            transfer = ns('wg', 'show', 'ttwg0', 'transfer').stdout.split()
            lab_write = {'fixtureId': identity, 'kernelPeer': True, 'latestHandshakeUnix': int(handshake[1]), 'receivedBytes': int(transfer[1]), 'sentBytes': int(transfer[2])}
            # Windows readers can briefly deny replacement; reuse the bounded existing receipt writer.
            relay_module.write_receipt(directory / 'peer-receipt.json', lab_write)
            time.sleep(.25)
        report['result'] = 'stopped'
    finally:
        if https: https.shutdown(); https.server_close(); https_thread.join(timeout=3)
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try: process.wait(timeout=8)
                except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=3)
        for stream in streams: stream.close()
        if interface_created and not moved: lab.run('ip', 'link', 'delete', birth)
        if namespace_created: lab.run('ip', 'netns', 'delete', namespace)
        if database_created: sql('postgres', f'DROP DATABASE {db}')
        report.update(namespaceRemoved=not Path('/run/netns', namespace).exists(), hostNetworkUnchanged=lab.snapshot() == before, disposableDatabaseRemoved=database_created, fixtureProcessesExited=all(p.poll() is not None for p in processes),
                      sourceSha256=pins, sourceUnchanged=pins == {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in sources}, productionInstallationTouched=False)
        write_json(directory / 'stopped.json', report)
        assert stage.parent == Path('/run') and stage.name.startswith('termterm-team-win-')
        shutil.rmtree(stage)
    assert report['namespaceRemoved'] and report['hostNetworkUnchanged'] and report['sourceUnchanged']


def self_test():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary); (root / '.lab').mkdir()
        assert destination(root / '.lab/new', root) == root / '.lab/new'
        existing = root / '.lab/existing'; existing.mkdir(); (existing / 'preserved').write_text('keep')
        for value in [root / 'outside', root / '.lab/../escaped', existing]:
            try: destination(value, root)
            except ValueError: pass
            else: raise AssertionError('Unsafe or existing fixture destination accepted')
        assert (existing / 'preserved').read_text() == 'keep'
    print('PASS: fixture path containment, traversal rejection and preservation; no network operations')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path)
    parser.add_argument('--mode', choices=['personal', 'shared'], default='shared')
    parser.add_argument('--duration', type=int, default=3600)
    parser.add_argument('--service', type=Path, default=Path('/root/termterm-vpn-team-build/debug/termterm-team-service'))
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test: self_test()
    else:
        assert args.directory and args.service.is_file()
        fixture(args)
