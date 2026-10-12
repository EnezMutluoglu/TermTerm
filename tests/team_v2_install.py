"""Qualify deployed scripts against a separate TLS PostgreSQL + real systemd.
Run only as root in the existing disposable WSL lab. No user vault is read.
"""
import hashlib
import http.client
import json
import os
from pathlib import Path
import secrets
import shutil
import ssl
import sys
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
PRIVATE = Path('/var/lib/termterm-team-qualification')
INSTALL = Path('/opt/termterm-team-qualification')
DB = 'termterm_team_v2_install_'+uuid.uuid4().hex[:8]
OWNER = 'termterm_team_v2_install_owner'
SERVICE = 'termterm_team_v2_install_service'
UNIT = 'termterm-team-qualification.service'
CA = '/etc/postgresql/18/termterm/tls/ca.crt'
CADDY = 'caddy:2.10.2@sha256:c3d7ee5d2b11f9dc54f947f68a734c84e9c9666c92c88a7f30b9cba5da182adb'


def command(args, **kw):
    p = subprocess.run(args, capture_output=True, text=True, **kw)
    if p.returncode:
        raise RuntimeError('Qualification command failed: ' + args[0])
    return p.stdout.strip()


def sql(query, database='postgres'):
    return command(['runuser', '-u', 'postgres', '--', 'psql', '-X', '-qAt', '-p', '55432',
                    '-d', database, '-v', 'ON_ERROR_STOP=1'], input=query)


def main():
    assert os.geteuid() == 0
    PRIVATE.mkdir(mode=0o700, parents=True, exist_ok=True)
    INSTALL.mkdir(mode=0o755, parents=True, exist_ok=True)
    credentials = PRIVATE/'database.json'
    if not credentials.exists():
        credentials.write_text(json.dumps({OWNER: secrets.token_hex(32), SERVICE: secrets.token_hex(32)}))
        credentials.chmod(0o600)
    passwords = json.loads(credentials.read_text())
    for role, password in passwords.items():
        assert role in [OWNER, SERVICE] and len(password) == 64 and all(c in '0123456789abcdef' for c in password)
        sql(f"DO $$ BEGIN IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='{role}') THEN CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT; END IF; END $$;")
    if not sql(f"SELECT 1 FROM pg_database WHERE datname='{DB}'"):
        sql(f'CREATE DATABASE {DB} OWNER {OWNER}')
    sql(f'CREATE EXTENSION IF NOT EXISTS pgcrypto; REVOKE CREATE ON SCHEMA public FROM PUBLIC; REVOKE ALL ON DATABASE {DB} FROM PUBLIC; GRANT CONNECT ON DATABASE {DB} TO {OWNER},{SERVICE}', DB)
    passfile = PRIVATE/'pgpass'
    passfile.write_text(f'localhost:55432:{DB}:{OWNER}:{passwords[OWNER]}\n')
    passfile.chmod(0o600)
    env = {**os.environ, 'PGHOST': 'localhost', 'PGPORT': '55432', 'PGDATABASE': DB, 'PGUSER': OWNER,
           'PGSSLROOTCERT': CA, 'PGPASSFILE': str(passfile)}
    tool = ROOT/'team-service/deploy/manage.py'
    args = ['python3', str(tool), 'migrate', '--service-role', SERVICE]
    command(args, env=env)
    print('Migration script applied successfully',flush=True)
    before = sql('SELECT count(*) FROM termterm_team_v2.schema_migrations', DB)
    command(args, env=env)
    assert sql('SELECT count(*) FROM termterm_team_v2.schema_migrations', DB) == before
    expected = len(list((ROOT/'team-service/migrations').glob('[0-9]*.sql')))
    assert int(before) == expected
    assert int(sql('SELECT count(*) FROM termterm_team_v2.migration_file_hashes', DB)) == expected
    # Corruption must fail before any following migration or grant is applied.
    original = sql('SELECT sha256 FROM termterm_team_v2.migration_file_hashes WHERE version=1', DB)
    sql("UPDATE termterm_team_v2.migration_file_hashes SET sha256=repeat('0',64) WHERE version=1", DB)
    try:
        assert subprocess.run(args, env=env, capture_output=True).returncode != 0
    finally:
        sql(f"UPDATE termterm_team_v2.migration_file_hashes SET sha256='{original}' WHERE version=1", DB)
    output = PRIVATE/('leader-' + uuid.uuid4().hex + '.json')
    command(['python3', str(tool), 'bootstrap', '--team', 'Installation qualification', '--username', 'leader',
             '--display-name', 'Qualification leader', '--output', str(output)], env=env)
    leader = json.loads(output.read_text())
    assert len(leader['temporaryPassword']) == 21 and output.stat().st_mode & 0o077 == 0
    # Existing output cannot bootstrap a duplicate team.
    assert subprocess.run(['python3', str(tool), 'bootstrap', '--team', 'Duplicate', '--username', 'leader',
        '--display-name', 'Duplicate', '--output', str(output)], env=env, capture_output=True).returncode != 0
    assert sql(f"SELECT count(*) FROM pg_roles WHERE rolname='{SERVICE}' AND (rolsuper OR rolbypassrls OR rolcreatedb OR rolcreaterole)") == '0'
    print('Replay, checksum and bootstrap tests passed',flush=True)
    version=json.loads((ROOT/'package.json').read_text())['version']
    delivery = 'team-'+version if '-dev.' in version else 'release-'+version
    source = ROOT/f'artifacts/{delivery}/team-service-linux-x64/bin/termterm-team-service' if '--package' in sys.argv else Path('/root/termterm-team-v2-build/team-service/target/debug/termterm-team-service')
    shutil.copy2(source, INSTALL/'termterm-team-service')
    shutil.copy2(CA, INSTALL/'postgres-ca.crt')
    config = PRIVATE/'service.json'
    config.write_text(json.dumps({'listen': '127.0.0.1:41851', 'database': {'host': 'localhost', 'port': 55432,
        'database': DB, 'username': SERVICE, 'password': passwords[SERVICE], 'caPath': str(INSTALL/'postgres-ca.crt')}}))
    config.chmod(0o600)
    unit = (ROOT/'team-service/deploy/termterm-team.service').read_text()
    unit = unit.replace('User=termterm-team\nGroup=termterm-team', 'DynamicUser=yes\nLoadCredential=service.json:' + str(config))
    unit = unit.replace('/opt/termterm-team/termterm-team-service /etc/termterm-team/service.json',
                        str(INSTALL/'termterm-team-service') + ' %d/service.json')
    Path('/run/systemd/system', UNIT).write_text(unit)
    command(['systemctl', 'daemon-reload'])
    container = 'termterm-team-qualification-https'
    caddyfile = PRIVATE/'Caddyfile'
    caddyfile.write_text('''{
 admin off
 auto_https disable_redirects
}
https://localhost:41852 {
 bind 127.0.0.1
 tls internal
 reverse_proxy 127.0.0.1:41851
 header Cache-Control no-store
 request_body {
  max_size 256KB
 }
}
''')
    def get(path, context=None):
        c = http.client.HTTPSConnection('localhost', 41852, context=context, timeout=3)
        try:
            c.request('GET', path, headers={'X-TermTerm-Client-Version':'0.4.1','X-TermTerm-API-Version':'1'}); r = c.getresponse(); return r.status, json.loads(r.read())
        finally:
            c.close()
    try:
        command(['systemctl', 'start', UNIT])
        command(['docker', 'run', '-d', '--name', container, '--network', 'host', '-v',
                 str(caddyfile)+':/etc/caddy/Caddyfile:ro', '-v', str(PRIVATE/'caddy-data')+':/data', CADDY])
        root = PRIVATE/'caddy-data/caddy/pki/authorities/local/root.crt'
        for _ in range(120):
            if root.exists():
                try:
                    context = ssl.create_default_context(cafile=str(root))
                    if get('/ready', context)[0] == 200:
                        break
                except (OSError, ValueError, http.client.HTTPException):
                    pass
            time.sleep(.25)
        else:
            raise AssertionError('Systemd/Caddy service did not become ready')
        assert get('/ready', context)[1]['schemaVersion'] == expected
        assert get('/v1/me', context)[0] == 401
        sql('INSERT INTO termterm_team_v2.schema_migrations(version) VALUES(999)', DB)
        try:
            assert get('/ready', context)[0] == 503, 'Newer schema must not report ready'
            assert subprocess.run(args,env=env,capture_output=True).returncode != 0
        finally:
            sql('DELETE FROM termterm_team_v2.schema_migrations WHERE version=999', DB)
        assert get('/ready', context)[0] == 200
        try:
            get('/health')
        except ssl.SSLCertVerificationError:
            pass
        else:
            raise AssertionError('Untrusted company CA accepted')
        command(['systemctl', 'restart', UNIT])
        for _ in range(30):
            try:
                if get('/ready', context)[0] == 200:
                    break
            except (OSError, ValueError, http.client.HTTPException):
                pass
            time.sleep(.2)
        else:
            raise AssertionError('Restart failed')
        assert command(['systemctl', 'is-active', UNIT]) == 'active'
        assert get('/ready', context)[1]['schemaVersion'] == expected
        print(f'PASS: clean {expected}-migration install, replay/checksum/newer-schema rejection, private leader bootstrap, real hardened systemd + Caddy HTTPS/CA, restart and limited PostgreSQL role.')
    finally:
        subprocess.run(['docker', 'rm', '-f', container], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        subprocess.run(['systemctl', 'stop', UNIT], check=True)


if __name__ == '__main__':
    main()
