"""Exercise the documented TLS/CA recipe in an isolated WSL PostgreSQL cluster."""
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import subprocess
import sys
import time
import uuid

ROOT=Path(__file__).resolve().parents[1]
DOC=(ROOT/'docs/TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md').read_text()
assert sys.argv[1:] in ([], ['--owner-template'], ['--local-db'], ['--socket-db'])
socket_db = sys.argv[1:] == ['--socket-db']
local_db = socket_db or sys.argv[1:] == ['--local-db']
owner_template = bool(sys.argv[1:])
owner = 'termterm_team_owner' if owner_template else 'termterm_team_migrator'
service = 'termterm_team_appuser' if owner_template else 'termterm_team_service'
name='tt_tls_'+uuid.uuid4().hex[:8]
private=Path('/var/lib/termterm-tls-guide')/name
private.mkdir(parents=True,mode=0o755)
runtime=private/'runtime';runtime.mkdir(mode=0o755)
cluster=Path('/etc/postgresql/18')/name
containers=[]


def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1',0));return s.getsockname()[1]


pgport,apiport,httpsport=port(),port(),port()
assert len({pgport,apiport,httpsport})==3


def run(args,**kwargs):
    p=subprocess.run(args,capture_output=True,text=True,timeout=90,**kwargs)
    if p.returncode:raise RuntimeError('TLS recipe command failed: '+args[0])
    return p.stdout.strip()


def sql(value):
    return run(['runuser','-u','postgres','--','psql','-X','-qAt','-p',str(pgport),'-d','postgres',
                '-v','ON_ERROR_STOP=1'],input=value)


def section(number,language):
    text=DOC.split(f'## {number}. ',1)[1].split('\n## ',1)[0]
    return re.search(r'```'+language+r'\n(.*?)\n```',text,re.S)[1]


try:
    # Same documented OpenSSL commands; only passphrase delivery is noninteractive.
    password_file=private/'ca-password';password_file.write_text(secrets.token_hex(24));password_file.chmod(0o600)
    recipe=section(3,'bash').replace('/root/termterm-pki',str(private/'pki'))
    recipe=recipe.replace('-aes-256-cbc -out',f'-aes-256-cbc -pass file:{password_file} -out')
    recipe=recipe.replace('-days 3650 -key',f'-days 3650 -passin file:{password_file} -key')
    recipe=recipe.replace('-CAkey postgres-ca.key -CAcreateserial',f'-CAkey postgres-ca.key -passin file:{password_file} -CAcreateserial')
    run(['bash','-ec',recipe])
    pki=private/'pki'
    run(['pg_createcluster','18',name,'--port',str(pgport),'--start-conf=manual','--start'])
    tls=cluster/'termterm-tls';tls.mkdir(mode=0o750)
    shutil.chown(tls,user='postgres',group='postgres')
    for source,target,mode in [('postgres-server.crt','server.crt',0o644),('postgres-server.key','server.key',0o600)]:
        shutil.copy2(pki/source,tls/target);shutil.chown(tls/target,user='postgres',group='postgres');(tls/target).chmod(mode)
    setup=section(5,'bash').split("<<'SQL'\n",1)[1].rsplit('\nSQL',1)[0]
    sql(setup.replace('/etc/postgresql/18/main/termterm-tls',str(tls)))
    hba=section(6,'conf').replace('termterm_team_migrator',owner).replace('termterm_team_service',service)
    if local_db:
        sql("ALTER SYSTEM SET ssl='off';")
        hba=f'host termterm_team {owner},{service} 127.0.0.1/32 scram-sha-256\nhost all all 0.0.0.0/0 reject\nhost all all ::/0 reject'
    socket_hba=f'local termterm_team {owner},{service} trust\nlocal termterm_upgrade_verify {owner} trust\n' if socket_db else ''
    (cluster/'pg_hba.conf').write_text(socket_hba+'local all all peer\n'+hba+'\n')
    assert sql('SELECT count(*) FROM pg_hba_file_rules WHERE error IS NOT NULL;')=='0'
    assert sql('SELECT count(*) FROM pg_file_settings WHERE error IS NOT NULL;')=='0'
    run(['pg_ctlcluster','18',name,'restart'])
    assert sql("SHOW ssl;")==('off' if local_db else 'on')
    assert sql("SELECT count(*) FROM pg_settings WHERE pending_restart;")=='0'
    owner_password,service_password=secrets.token_hex(24),secrets.token_hex(24)
    setup=(ROOT/'team-service/deploy'/('prepare-database-owner-template.sql' if owner_template else 'prepare-database.sql')).read_text()
    if socket_db:
        setup='\\set socket_auth true\n'+setup
    else:
        setup=setup.replace(f'\\password {owner}',f"ALTER ROLE {owner} PASSWORD '{owner_password}';")
        setup=setup.replace(f'\\password {service}',f"ALTER ROLE {service} PASSWORD '{service_password}';")
    sql(setup)
    ca=runtime/'postgres-ca.crt';shutil.copy2(pki/'postgres-ca.crt',ca);ca.chmod(0o644)
    pgpass=private/'migration.pgpass'
    host='/var/run/postgresql' if socket_db else ('127.0.0.1' if local_db else 'localhost')
    pgpass.write_text(f'{host}:{pgport}:termterm_team:{owner}:{owner_password}\n');pgpass.chmod(0o600)
    env={**os.environ,'PGHOST':host,'PGPORT':str(pgport),'PGDATABASE':'termterm_team','PGUSER':owner,
         'PGSSLROOTCERT':str(ca),'PGPASSFILE':str(pgpass),'PGSSLMODE':'verify-full'}
    if local_db:
        env['PGSSLMODE']='disable';env.pop('PGSSLROOTCERT')
    if socket_db:
        env.pop('PGPASSFILE');pgpass.unlink()
        assert sql(f"SELECT count(*) FROM pg_authid WHERE rolname IN ('{owner}','{service}') AND rolpassword IS NOT NULL")=='0'
    query=['psql','-X','-w','-qAt','-c','SELECT ssl FROM pg_stat_ssl WHERE pid=pg_backend_pid()']
    assert run(query,env=env)==('f' if local_db else 't')
    if not local_db:
        denied=subprocess.run(query,env={**env,'PGSSLMODE':'disable'},capture_output=True)
        assert denied.returncode!=0 and b'pg_hba.conf rejects' in denied.stderr
        denied=subprocess.run(query,env={**env,'PGHOST':'wrong.invalid','PGHOSTADDR':'127.0.0.1'},capture_output=True)
        assert denied.returncode!=0 and b'certificate' in denied.stderr
        run(['openssl','s_client','-starttls','postgres','-connect',f'localhost:{pgport}','-servername','localhost',
             '-verify_hostname','localhost','-CAfile',str(ca),'-verify_return_error'],input='')
    tool=str(ROOT/'team-service/deploy/manage.py')
    run(['python3',tool,'migrate','--service-role',service],env=env)
    if owner_template:
        assert sql("SELECT pg_get_userbyid(datdba)||'|'||datcollate||'|'||datctype||'|'||datlocprovider::text FROM pg_database WHERE datname='termterm_team'") == owner+'|en_US.UTF-8|en_US.UTF-8|c'
        assert sql(f"SELECT pg_has_role('{service}','{owner}','MEMBER')") == 'f'
        assert sql(f"SELECT pg_has_role('{service}','termterm_team','USAGE')") == 't'
        # Probe future tables too: global default ACLs must not leak Team data.
        checks=f"""BEGIN;
CREATE TABLE termterm_team_v2.permission_probe(id int);
SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname='termterm_team_v2' AND c.relkind='r'
AND (pg_get_userbyid(c.relowner)<>'{owner}' OR EXISTS (
 SELECT FROM unnest(ARRAY['{service}','termterm_team_app','termterm_team_developer','termterm_team_readonly']) r
 WHERE has_table_privilege(r,c.oid,'SELECT, INSERT, UPDATE, DELETE')));
ROLLBACK;"""
        assert run(['psql','-X','-qAt','-v','ON_ERROR_STOP=1'],input=checks,env=env)=='0'
        # Same-schema verification remains safe with the alternate account names.
        run(['python3',tool,'migrate','--service-role',service],env=env)
    run(['python3',tool,'bootstrap','--team','TLS guide lab','--username','leader','--display-name','Leader',
         '--output',str(private/'leader.json')],env=env)
    assert len(json.loads((private/'leader.json').read_text())['temporaryPassword'])==21
    if socket_db:
        sql(f'CREATE DATABASE termterm_upgrade_verify OWNER {owner}; REVOKE ALL ON DATABASE termterm_upgrade_verify FROM PUBLIC;')
        upgrade=str(ROOT/'team-service/deploy/upgrade.py')
        plan=private/'upgrade-plan.json';state=private/'upgrade-state'
        run(['python3',upgrade,'plan','--service-role',service,'--output',str(plan)],env=env)
        run(['python3',upgrade,'apply','--plan',str(plan),'--state-directory',str(state),
             '--verification-database','termterm_upgrade_verify','--service-stopped'],env=env)
    cfg={'listen':f'127.0.0.1:{apiport}','database':{'host':host,'port':pgport,'database':'termterm_team',
         'username':service,'password':service_password,'caPath':'/etc/termterm-team/postgres-ca.crt'}}
    if local_db:
        cfg['database'].pop('caPath');cfg['database']['sslMode']='disable';ca.unlink()
    if socket_db:
        cfg['database'].pop('password')
    config=runtime/'service.json';config.write_text(json.dumps(cfg));os.chown(config,0,10001);config.chmod(0o640)
    team=name+'-api';containers.append(team)
    binary=os.environ.get('TEAM_SERVICE_BINARY')
    overrides=['-v',binary+':/usr/local/bin/termterm-team-service:ro'] if binary else []
    if socket_db:
        overrides+=['-v','/var/run/postgresql:/var/run/postgresql:ro']
    image=os.environ.get('TEAM_SERVICE_IMAGE','termterm/team-service:0.4.1')
    run(['docker','run','-d','--name',team,'--network','host','--read-only','--cpus','1','--memory','512m',
         '-v',str(runtime)+':/etc/termterm-team:ro',*overrides,image])
    caddyfile=private/'Caddyfile'
    https_host='127.0.0.1' if local_db else 'team.sirket.test'
    caddyfile.write_text(f'{{\n admin off\n auto_https disable_redirects\n}}\nhttps://{https_host}:{httpsport} {{\n bind 127.0.0.1\n tls internal\n reverse_proxy 127.0.0.1:{apiport}\n}}\n')
    https=name+'-https';containers.append(https)
    run(['docker','run','-d','--name',https,'--network','host','-v',str(caddyfile)+':/etc/caddy/Caddyfile:ro',
         '-v',str(private/'caddy-data')+':/data',
         'caddy:2.10.2@sha256:c3d7ee5d2b11f9dc54f947f68a734c84e9c9666c92c88a7f30b9cba5da182adb'])
    httpsca=private/'team-https-ca.crt'
    curl=['curl','--silent','--show-error','--fail','--max-time','3','--noproxy','*','--resolve',
          f'{https_host}:{httpsport}:127.0.0.1','--cacert',str(httpsca),f'https://{https_host}:{httpsport}/ready']
    for _ in range(80):
        copy=subprocess.run(['docker','cp',https+':/data/caddy/pki/authorities/local/root.crt',str(httpsca)],capture_output=True)
        if copy.returncode==0:
            result=subprocess.run(curl,capture_output=True,text=True)
            if result.returncode==0:break
        time.sleep(.25)
    else:raise AssertionError('HTTPS with separate company CA did not become ready')
    assert json.loads(result.stdout)['schemaVersion']==64
    if socket_db:
        run(['python3',upgrade,'confirm','--state-directory',str(state),'--url',f'https://{https_host}:{httpsport}',
             '--ca-file',str(httpsca)],env=env)
        assert json.loads((state/'state.json').read_text())['phase']=='ready'
        print('PASS: socket owner plan/apply, private backup/restore database, preserved leader and IP HTTPS confirmation',flush=True)
    if owner_template:
        print('PASS: owner/appuser template, en_US.UTF-8 libc, inherited CONNECT, isolated owner, no direct current/future Team table grants, repeat migration and real service readiness.')
    if local_db:
        # Verify a real application login traverses HTTPS -> API -> password-only DB.
        credentials=json.loads((private/'leader.json').read_text())
        body=json.dumps({'teamId':credentials['teamId'],'username':'leader','password':credentials['temporaryPassword'],
                         'deviceId':str(uuid.uuid4()),'deviceName':'Local DB fixture'})
        login=curl[:-1]+[f'https://{https_host}:{httpsport}/v1/auth/login','-H','Content-Type: application/json',
                        '-H','x-termterm-client-version: 0.4.2','-H','x-termterm-api-version: 1','--data-binary','@-']
        assert json.loads(run(login,input=body))['accessToken']
        bad=json.loads(body);bad['password']='Wrong-Password-123456'
        assert subprocess.run(login,input=json.dumps(bad),text=True,capture_output=True).returncode!=0
        untrusted=curl.copy();index=untrusted.index('--cacert');del untrusted[index:index+2]
        assert subprocess.run(untrusted,capture_output=True).returncode==60
        print('PASS: PostgreSQL ssl=off, no CA file, '+('passwordless Unix socket owner/appuser' if socket_db else 'SCRAM owner/appuser')+', migration/bootstrap, HTTPS login and wrong-password rejection; schema 64.')
        if os.environ.get('TEAM_DESKTOP_FIXTURE'):
            fixture=ROOT/'.lab/team-042-socket'
            fixture.mkdir(parents=True,exist_ok=True)
            stop=fixture/'desktop.stop';stop.unlink(missing_ok=True)
            (fixture/'windows-native.json').write_text(json.dumps({'profile':{'serviceUrl':f'https://{https_host}:{httpsport}',
                'teamId':credentials['teamId'],'caPem':httpsca.read_text()},'username':'leader','password':credentials['temporaryPassword']}))
            print('Disposable socket/HTTPS desktop fixture ready',flush=True)
            deadline=time.monotonic()+3600
            while not stop.exists() and time.monotonic()<deadline:time.sleep(1)
    else:
        wrong_ca=curl.copy();wrong_ca[wrong_ca.index('--cacert')+1]=str(ca)
        assert subprocess.run(wrong_ca,capture_output=True).returncode==60, 'DB CA must not trust HTTPS CA'
        denied=subprocess.run(query,env={**env,'PGSSLROOTCERT':str(httpsca)},capture_output=True)
        assert denied.returncode!=0 and b'certificate' in denied.stderr, 'HTTPS CA must not trust DB CA'
        print('PASS: documented CA/SAN/permissions, TLS/SCRAM, plaintext/wrong hostname/wrong CA rejected; 64 migrations + leader; Docker API/Caddy HTTPS readiness.')
finally:
    for container in containers:subprocess.run(['docker','rm','-f',container],capture_output=True)
    if cluster.exists():subprocess.run(['pg_dropcluster','--stop','18',name],capture_output=True)
