#!/usr/bin/env python3
"""Real native Team required-VPN SSH/SFTP in disposable Linux namespaces only."""
import argparse, hashlib, http.client, importlib.util, json, os, pathlib, re, secrets, select, shutil, signal, socket, socketserver, ssl, subprocess, sys, tempfile, threading, time, uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]
PRIVATE = pathlib.Path('/var/lib/termterm-team-upgrade-tests')
OWNER, SERVICE = 'termterm_team_upgrade_owner', 'termterm_team_upgrade_service'

def run(*args, **kwargs):
    return subprocess.run(list(map(str, args)), check=True, capture_output=True, **kwargs)

def snapshot():
    return {'routes4': run('ip', '-j', 'route', 'show', 'table', 'all').stdout.decode(),
            'routes6': run('ip', '-j', '-6', 'route', 'show', 'table', 'all').stdout.decode(),
            'dns': hashlib.sha256(pathlib.Path('/etc/resolv.conf').read_bytes()).hexdigest()}

def bridge(left, right):
    with left, right:
        while True:
            readable, _, _ = select.select([left, right], [], [], 30)
            if not readable:
                return
            for source in readable:
                data = source.recv(65536)
                if not data:
                    return
                (right if source is left else left).sendall(data)

def child(mode, stage):
    assert os.geteuid() == 0
    assert pathlib.Path('/proc/self/ns/net').readlink() != pathlib.Path('/proc/1/ns/net').readlink()
    config = json.loads((stage / 'config.json').read_text())
    if mode == 'broker':
        spec = importlib.util.spec_from_file_location('team_vpn_native_broker', stage / 'vpn-unix-broker.py')
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        assert module.lab.isolated()
        module.SOCKET = str(stage / 'broker.sock')
        module.serve(config['broker'])
    else:
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(stage / 'https.crt', stage / 'https.key')
        class Handler(socketserver.BaseRequestHandler):
            def handle(self):
                upstream = socket.socket(socket.AF_UNIX)
                upstream.connect(str(stage / 'team.sock'))
                bridge(self.request, upstream)
        with socketserver.ThreadingTCPServer(('127.0.0.1', config['httpsPort']), Handler) as server:
            server.daemon_threads = True
            server.socket = tls.wrap_socket(server.socket, server_side=True)
            server.serve_forever()

def main(args):
    assert os.geteuid() == 0, 'Disposable root Linux lab required'
    assert args.binary.is_file()
    before, namespaces, processes, logs = snapshot(), [], [], []
    identity = uuid.uuid4().hex
    client_ns, server_ns = 'ttteam-c-' + identity[:10], 'ttteam-s-' + identity[:10]
    case = PRIVATE / ('native-ssh-' + identity); case.mkdir(mode=0o700)
    stage = pathlib.Path(tempfile.mkdtemp(prefix='termterm-team-vpn-', dir='/run')); stage.chmod(0o755)
    report = {'result': 'failed', 'scope': 'native Team/WireGuard/SSH/SFTP', 'checks': []}
    relay, relay_thread, capture = None, None, None
    def ns(name, *command, **kwargs): return run('ip', 'netns', 'exec', name, *command, **kwargs)
    def start(name, command):
        stream = (case / (name + '.log')).open('wb'); logs.append(stream)
        process = subprocess.Popen(list(map(str, command)), stdout=stream, stderr=stream, start_new_session=True)
        processes.append(process); return process
    def sql(db, text):
        return run('runuser', '-u', 'postgres', '--', 'psql', '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-p', '55432', '-d', db, input=text.encode()).stdout.decode().strip()
    def port():
        with socket.socket() as value: value.bind(('127.0.0.1', 0)); return value.getsockname()[1]
    def wait(check, what):
        deadline = time.monotonic() + 10
        while not check():
            assert time.monotonic() < deadline, 'Fixture readiness failed: ' + what
            time.sleep(.05)
    try:
        uid = int(run('id', '-u', 'termterm_test').stdout); gid = int(run('id', '-g', 'termterm_test').stdout)
        for directory in ['client', 'remote']:
            path = stage / directory; path.mkdir(mode=0o700); os.chown(path, uid, gid)
        for name in ['cache', 'runtime', 'data']:
            path = stage / 'client' / name; path.mkdir(mode=0o700); os.chown(path, uid, gid)
        shutil.copyfile(args.binary, stage / 'desktop'); (stage / 'desktop').chmod(0o755)
        for name in ['vpn-unix-broker.py', 'vpn-lab-broker.py']:
            shutil.copyfile(ROOT / 'scripts' / name, stage / name); (stage / name).chmod(0o644)
        engines = stage / 'engines'; engines.mkdir(mode=0o755)
        originals = {name: pathlib.Path('/root/termterm-vpn-native-build/engine') / name for name in ['termterm-openconnect', 'termterm-openvpn']}
        originals.update({'vpn-policy-lab': pathlib.Path('/root/termterm-vpn-native-build/core/debug/vpn-policy-lab'), 'termterm-wireguard': args.wireguard_engine})
        pins = {}
        for name, original in originals.items():
            shutil.copyfile(original, engines / name); (engines / name).chmod(0o755)
            pins[name] = hashlib.sha256((engines / name).read_bytes()).hexdigest()
        journal = stage / 'journal'; journal.mkdir(mode=0o700)
        for name in [client_ns, server_ns]:
            run('ip', 'netns', 'add', name); namespaces.append(name)
            ns(name, 'ip', 'link', 'set', 'lo', 'up')
        run('ip', 'link', 'add', 'ttvs', 'netns', server_ns, 'type', 'veth', 'peer', 'name', 'ttvc', 'netns', client_ns)
        for name, device, address in [(server_ns, 'ttvs', '198.18.8.1/30'), (client_ns, 'ttvc', '198.18.8.2/30')]:
            ns(name, 'ip', 'addr', 'add', address, 'dev', device); ns(name, 'ip', 'link', 'set', device, 'up')
        ns(client_ns, 'ip', 'route', 'add', 'default', 'via', '198.18.8.1', 'dev', 'ttvc')
        def keys():
            private = run('wg', 'genkey').stdout.strip()
            return private, run('wg', 'pubkey', input=private + b'\n').stdout.strip()
        server_private, server_public = keys(); client_private, client_public = keys()
        ns(server_ns, 'ip', 'link', 'add', 'ttpeer', 'type', 'wireguard')
        ns(server_ns, 'wg', 'setconf', 'ttpeer', '/dev/stdin', input=b'[Interface]\nPrivateKey = ' + server_private + b'\nListenPort = 31820\n[Peer]\nPublicKey = ' + client_public + b'\nAllowedIPs = 10.97.0.2/32\n')
        ns(server_ns, 'ip', 'addr', 'add', '10.97.0.1/32', 'dev', 'ttpeer'); ns(server_ns, 'ip', 'link', 'set', 'ttpeer', 'up')
        ns(server_ns, 'ip', 'route', 'add', '10.97.0.2/32', 'dev', 'ttpeer')
        ns(server_ns, 'iptables', '-A', 'INPUT', '-i', 'ttvs', '-p', 'tcp', '-d', '10.97.0.1', '--dport', '41843', '-j', 'REJECT')
        run('ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', stage / 'hostkey')
        public_key = run('ssh-keygen', '-y', '-f', ROOT / 'tests/ssh-keys/rsa2048.pkcs1').stdout
        (stage / 'authorized_keys').write_bytes(public_key); (stage / 'authorized_keys').chmod(0o644)
        (stage / 'sshd_config').write_text(f'Port 41843\nListenAddress 10.97.0.1\nHostKey {stage}/hostkey\nPidFile {stage}/sshd.pid\nAuthorizedKeysFile {stage}/authorized_keys\nAllowUsers termterm_test\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nPermitRootLogin no\nUsePAM no\nSubsystem sftp internal-sftp\n')
        run('/usr/sbin/sshd', '-t', '-f', stage / 'sshd_config')
        sshd = start('sshd', ['ip', 'netns', 'exec', server_ns, '/usr/sbin/sshd', '-D', '-e', '-f', stage / 'sshd_config'])
        wait(lambda: b':41843' in ns(server_ns, 'ss', '-ltn').stdout, 'sshd')
        probe = subprocess.run(['ip', 'netns', 'exec', client_ns, 'python3', '-c', 'import socket;socket.create_connection(("10.97.0.1",41843),timeout=1)'], capture_output=True)
        assert probe.returncode != 0, 'SSH target reachable without tunnel'
        report['checks'].append('target-unreachable-without-tunnel-despite-physical-default-route')
        capture = start('capture', ['ip', 'netns', 'exec', client_ns, 'tcpdump', '-U', '-ni', 'ttvc', '-w', stage / 'physical.pcap', 'tcp and dst host 10.97.0.1 and dst port 41843'])
        wait(lambda: (stage / 'physical.pcap').exists(), 'packet capture')
        passwords = json.loads((PRIVATE / 'roles.json').read_text()); db = 'termterm_native_vpn_' + identity[:12]
        sql('postgres', f'CREATE DATABASE {db} OWNER {OWNER}')
        sql(db, f'REVOKE ALL ON DATABASE {db} FROM PUBLIC;GRANT CONNECT ON DATABASE {db} TO {SERVICE}')
        pgpass = case / 'pgpass'; pgpass.write_text(f'localhost:55432:*:{OWNER}:{passwords[OWNER]}\n'); pgpass.chmod(0o600)
        ca = '/etc/postgresql/18/termterm/tls/ca.crt'
        env = {**os.environ, 'PGHOST': 'localhost', 'PGPORT': '55432', 'PGDATABASE': db, 'PGUSER': OWNER, 'PGSSLROOTCERT': ca, 'PGPASSFILE': str(pgpass)}
        run('python3', ROOT / 'team-service/deploy/manage.py', 'migrate', '--service-role', SERVICE, env=env)
        bootstrap = json.loads(sql(db, f"SET ROLE {OWNER};SELECT termterm_team_v2.bootstrap_team('Disposable native VPN','leader','VPN native leader')"))
        backend, https_port = port(), port(); assert backend != https_port
        service_config = case / 'service.json'; service_config.write_text(json.dumps({'listen': f'127.0.0.1:{backend}', 'database': {'host': 'localhost', 'port': 55432, 'database': db, 'username': SERVICE, 'password': passwords[SERVICE], 'caPath': ca}})); service_config.chmod(0o600)
        service = start('service', [os.environ.get('TEAM_SERVICE_BINARY', '/root/termterm-vpn-team-build/debug/termterm-team-service'), service_config])
        def ready():
            assert service.poll() is None, 'Team fixture service exited'
            try:
                c = http.client.HTTPConnection('127.0.0.1', backend, timeout=.2); c.request('GET', '/ready'); response = c.getresponse(); ok = response.status == 200; response.read(); c.close(); return ok
            except OSError: return False
        wait(ready, 'Team service')
        class Relay(socketserver.BaseRequestHandler):
            def handle(self): bridge(self.request, socket.create_connection(('127.0.0.1', backend), timeout=25))
        relay = socketserver.ThreadingUnixStreamServer(str(stage / 'team.sock'), Relay); relay.daemon_threads = True
        relay_thread = threading.Thread(target=relay.serve_forever, daemon=True); relay_thread.start()
        run('openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', stage / 'https.key', '-out', stage / 'https.crt', '-days', '2', '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost')
        config = {'httpsPort': https_port, 'broker': {'uid': uid, 'client': str(stage / 'desktop'), 'clientSha256': hashlib.sha256((stage / 'desktop').read_bytes()).hexdigest(), 'engines': str(engines), 'pins': pins, 'journal': str(journal)}}
        (stage / 'config.json').write_text(json.dumps(config)); (stage / 'config.json').chmod(0o600)
        broker = start('broker', ['ip', 'netns', 'exec', client_ns, 'python3', __file__, '--inside', 'broker', '--stage', stage])
        proxy = start('https', ['ip', 'netns', 'exec', client_ns, 'python3', __file__, '--inside', 'tls', '--stage', stage])
        wait(lambda: (stage / 'broker.sock').exists() and b':'+str(https_port).encode() in ns(client_ns, 'ss', '-ltn').stdout, 'broker and HTTPS proxy')
        vpn = {'version': 1, 'label': 'Disposable Team WireGuard', 'family': 'wireGuard', 'source': 'fixture', 'server': '', 'groupId': None, 'username': '', 'password': '', 'realm': '', 'remember': False,
               'config': '[Interface]\nPrivateKey = ' + client_private.decode() + '\nAddress = 10.97.0.2/32\n[Peer]\nPublicKey = ' + server_public.decode() + '\nEndpoint = 198.18.8.1:31820\nAllowedIPs = 10.97.0.0/24\nPersistentKeepalive = 1\n'}
        fixture = stage / 'fixture.json'
        fixture.write_text(json.dumps({'profile': {'serviceUrl': f'https://localhost:{https_port}', 'teamId': bootstrap['teamId'], 'caPem': (stage / 'https.crt').read_text()}, 'username': 'leader', 'password': bootstrap['temporaryPassword'], 'vpn': vpn, 'sshKey': (ROOT / 'tests/ssh-keys/rsa2048.pkcs1').read_text(), 'hostFingerprint': run('ssh-keygen', '-lf', stage / 'hostkey.pub').stdout.decode().split()[1], 'root': str(stage), 'hostNetworkNamespace': pathlib.Path('/proc/self/ns/net').stat().st_ino, 'silentLoss': args.silent_loss, 'sharedRevocation': args.shared_revocation}))
        fixture.chmod(0o600); os.chown(fixture, uid, gid)
        payload = stage / 'client/payload'; payload.write_bytes(secrets.token_bytes(64*1024)); os.chown(payload, uid, gid)
        (stage / 'remote/large').write_bytes(secrets.token_bytes(65536)*512)
        native_env = {**os.environ, 'HOME': str(stage / 'client'), 'XDG_DATA_HOME': str(stage / 'client/data'), 'XDG_CACHE_HOME': str(stage / 'client/cache'), 'XDG_RUNTIME_DIR': str(stage / 'client/runtime'), 'TERMTERM_TEAM_VPN_NATIVE_FIXTURE': str(fixture), 'TERMTERM_VPN_LAB_SOCKET': str(stage / 'broker.sock'), 'NO_AT_BRIDGE': '1'}
        native_log = (case / 'native.log').open('wb'); logs.append(native_log)
        native = subprocess.Popen(['ip', 'netns', 'exec', client_ns, 'setpriv', '--reuid', str(uid), '--regid', str(gid), '--clear-groups', 'dbus-run-session', '--', 'xvfb-run', '-a', str(stage / 'desktop')], env=native_env, stdout=native_log, stderr=native_log, start_new_session=True); processes.append(native)
        if args.shared_revocation:
            wait_deadline = time.monotonic() + 150
            while not (stage / 'client/revocation-cleanup-ready').exists():
                assert native.poll() is None and time.monotonic() < wait_deadline, 'Native revocation did not disconnect before logout; see private native.log'
                time.sleep(.05)
            verification_started = time.monotonic()
            assert json.loads((journal / 'owned.json').read_text()) == {}, 'Revoked VPN journal survived before logout'
            assert not any(re.fullmatch(r'ttv[0-9a-f]{12}', link['ifname']) for link in json.loads(ns(client_ns, 'ip', '-j', 'link').stdout)), 'Revoked VPN TUN survived before logout'
            ack = stage / 'client/revocation-cleanup-verified'
            ack.write_text('journal-and-owned-tun-zero-before-logout'); ack.chmod(0o600); os.chown(ack, uid, gid)
            report['preLogoutCleanup'] = {'nativeJournalEntries': 0, 'nativeOwnedTunCount': 0, 'verificationSeconds': round(time.monotonic() - verification_started, 3)}
            assert native.wait(timeout=60) == 0, 'Native revocation fixture failed; see private native.log'
        elif args.silent_loss:
            wait_deadline = time.monotonic() + 100
            while not (stage / 'client/network-loss-ready').exists():
                assert native.poll() is None and time.monotonic() < wait_deadline, 'Native loss fixture did not reach paused transfer; see private native.log'
                time.sleep(.05)
            loss_started = time.monotonic()
            ns(client_ns, 'iptables', '-A', 'OUTPUT', '-p', 'udp', '-d', '198.18.8.1', '--dport', '31820', '-j', 'DROP')
            ns(client_ns, 'iptables', '-A', 'INPUT', '-p', 'udp', '-s', '198.18.8.1', '--sport', '31820', '-j', 'DROP')
            assert native.wait(timeout=75) == 0, 'Native silent UDP loss fixture failed; see private native.log'
            report['silentUdpLossSeconds'] = round(time.monotonic() - loss_started, 3)
            assert 29 <= report['silentUdpLossSeconds'] <= 75
        else:
            assert native.wait(timeout=210) == 0, 'Native fixture failed; see private native.log'
        report['native'] = json.loads((stage / 'client/native-result.json').read_text())
        assert report['native']['result'] == 'passed'
        assert json.loads((journal / 'owned.json').read_text()) == {}, 'Owned native journal not empty'
        assert not any(re.fullmatch(r'ttv[0-9a-f]{12}',link['ifname']) for link in json.loads(ns(client_ns, 'ip', '-j', 'link').stdout)), 'Owned native TUN survived'
        capture.send_signal(signal.SIGINT); capture.wait(timeout=5); capture = None
        escaped = run('tcpdump', '-nr', stage / 'physical.pcap').stdout.splitlines()
        assert not escaped, 'Target TCP escaped via physical veth'
        shutil.copyfile(stage / 'physical.pcap', case / 'physical.pcap')
        report.update(result='passed', realNativeTeamPaths=True, realTunnels=True, physicalTargetTcpPackets=0, nativeJournalEntries=0, nativeOwnedTunCount=0)
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try: process.wait(timeout=8)
                except subprocess.TimeoutExpired: os.killpg(process.pid, signal.SIGKILL); process.wait(timeout=3)
        if relay: relay.shutdown(); relay.server_close(); relay_thread.join(timeout=3)
        for stream in logs: stream.close()
        for name in reversed(namespaces): run('ip', 'netns', 'delete', name)
        report['namespacesRemoved'] = all(not pathlib.Path('/run/netns', name).exists() for name in namespaces)
        report['hostNetworkUnchanged'] = snapshot() == before
        report['evidence'] = str(case)
        args.report.parent.mkdir(parents=True, exist_ok=True); args.report.write_text(json.dumps(report, indent=2))
        shutil.rmtree(stage)
    assert report['result'] == 'passed' and report['namespacesRemoved'] and report['hostNetworkUnchanged']
    print('PASS: real native Team required-VPN SSH output, SFTP SHA256, VPN loss/disconnect cancellation, atomic target preservation and zero physical target TCP; namespaces removed, host network unchanged', flush=True)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=pathlib.Path)
    parser.add_argument('--report', type=pathlib.Path)
    parser.add_argument('--wireguard-engine', type=pathlib.Path, default=ROOT / '.tools/vpn-wireguard/termterm-wireguard-linux')
    parser.add_argument('--shared-revocation', action='store_true', help='Use a second account and revoke its shared VPN while SSH/SFTP and a paused transfer are live')
    parser.add_argument('--silent-loss', action='store_true', help='Blackhole WireGuard UDP after SSH/SFTP and a paused transfer are live')
    parser.add_argument('--inside', choices=['broker', 'tls'])
    parser.add_argument('--stage', type=pathlib.Path)
    arguments = parser.parse_args()
    if arguments.inside: child(arguments.inside, arguments.stage)
    else:
        assert arguments.binary and arguments.report
        assert not (arguments.silent_loss and arguments.shared_revocation), 'Select one interruption mode'
        main(arguments)
