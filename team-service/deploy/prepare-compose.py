#!/usr/bin/env python3
"""Create private local PostgreSQL credentials/TLS files; never overwrites a deployment.
Run as root on Linux, then supply a real HTTPS domain or the internal Caddy recipe.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--directory', required=True)
p.add_argument('--port', type=int, default=55433)
a = p.parse_args()
if os.geteuid() != 0 or not 1024 <= a.port <= 65535:
    p.error('Run as root and select an unprivileged port')
root = Path(a.directory).resolve()
root.mkdir(mode=0o700, parents=True, exist_ok=False)
config = root/'config'
config.mkdir(mode=0o750)
os.chown(config, 0, 10001)
tls = root/'postgres-tls'
tls.mkdir(mode=0o700)
values = {}
for name in ['postgres', 'migration', 'service']:
    values[name] = secrets.token_hex(32)
    file = root/(name+'-password')
    file.write_text(values[name]+'\n')
    # Each file is individually mounted for PostgreSQL's unprivileged user.
    # The containing host directory is root-only (0700).
    file.chmod(0o444)


def openssl(*args):
    subprocess.run(['openssl', *args], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


os.umask(0o077)
openssl('req', '-x509', '-newkey', 'rsa:3072', '-nodes', '-days', '825', '-subj',
        '/CN=TermTerm local PostgreSQL CA', '-keyout', str(tls/'ca.key'), '-out', str(tls/'ca.crt'))
openssl('req', '-newkey', 'rsa:3072', '-nodes', '-subj', '/CN=localhost',
        '-keyout', str(tls/'server.key'), '-out', str(tls/'server.csr'))
ext = tls/'extensions.cnf'
ext.write_text('subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n')
openssl('x509', '-req', '-in', str(tls/'server.csr'), '-CA', str(tls/'ca.crt'),
        '-CAkey', str(tls/'ca.key'), '-CAcreateserial', '-days', '825', '-extfile', str(ext), '-out', str(tls/'server.crt'))
ca = config/'postgres-ca.crt'
ca.write_bytes((tls/'ca.crt').read_bytes()); ca.chmod(0o644)
service = config/'service.json'
service.write_text(json.dumps({'listen':'127.0.0.1:41841','database':{
    'host':'localhost','port':a.port,'database':'termterm_team','username':'termterm_team_service',
    'password':values['service'],'caPath':'/etc/termterm-team/postgres-ca.crt'}}, indent=2))
os.chown(service, 0, 10001); service.chmod(0o640)
pgpass = root/'migration.pgpass'
pgpass.write_text(f"localhost:{a.port}:termterm_team:termterm_team_migrator:{values['migration']}\n")
pgpass.chmod(0o600)
(root/'deployment.env').write_text(f'TERMTERM_PRIVATE_DIR={root}\nTERMTERM_CONFIG_DIR={config}\nTERMTERM_PG_PORT={a.port}\nTERMTERM_DOMAIN=replace-with-your-dns-name\n')
print('Private configuration prepared. No database started or migration applied. Preserve the CA key offline for certificate renewal.')
