"""Exercise the documented TLS/CA recipe in an isolated WSL PostgreSQL cluster."""
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import subprocess
import time
import uuid

ROOT=Path(__file__).resolve().parents[1]
DOC=(ROOT/'docs/TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md').read_text()
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
    hba=section(6,'conf')
    (cluster/'pg_hba.conf').write_text('local all all peer\n'+hba+'\n')
    assert sql('SELECT count(*) FROM pg_hba_file_rules WHERE error IS NOT NULL;')=='0'
    assert sql('SELECT count(*) FROM pg_file_settings WHERE error IS NOT NULL;')=='0'
    run(['pg_ctlcluster','18',name,'restart'])
    assert sql("SHOW ssl;")=='on'
    assert sql("SELECT count(*) FROM pg_settings WHERE pending_restart;")=='0'
    owner_password,service_password=secrets.token_hex(24),secrets.token_hex(24)
    setup=(ROOT/'team-service/deploy/prepare-database.sql').read_text()
    setup=setup.replace('\\password termterm_team_migrator',f"ALTER ROLE termterm_team_migrator PASSWORD '{owner_password}';")
    setup=setup.replace('\\password termterm_team_service',f"ALTER ROLE termterm_team_service PASSWORD '{service_password}';")
    sql(setup)
    ca=runtime/'postgres-ca.crt';shutil.copy2(pki/'postgres-ca.crt',ca);ca.chmod(0o644)
    pgpass=private/'migration.pgpass'
    pgpass.write_text(f'localhost:{pgport}:termterm_team:termterm_team_migrator:{owner_password}\n');pgpass.chmod(0o600)
    env={**os.environ,'PGHOST':'localhost','PGPORT':str(pgport),'PGDATABASE':'termterm_team','PGUSER':'termterm_team_migrator',
         'PGSSLROOTCERT':str(ca),'PGPASSFILE':str(pgpass),'PGSSLMODE':'verify-full'}
    query=['psql','-X','-w','-qAt','-c','SELECT ssl FROM pg_stat_ssl WHERE pid=pg_backend_pid()']
    assert run(query,env=env)=='t'
    denied=subprocess.run(query,env={**env,'PGSSLMODE':'disable'},capture_output=True)
    assert denied.returncode!=0 and b'pg_hba.conf rejects' in denied.stderr
    denied=subprocess.run(query,env={**env,'PGHOST':'wrong.invalid','PGHOSTADDR':'127.0.0.1'},capture_output=True)
    assert denied.returncode!=0 and b'certificate' in denied.stderr
    run(['openssl','s_client','-starttls','postgres','-connect',f'localhost:{pgport}','-servername','localhost',
         '-verify_hostname','localhost','-CAfile',str(ca),'-verify_return_error'],input='')
    tool=str(ROOT/'team-service/deploy/manage.py')
    run(['python3',tool,'migrate','--service-role','termterm_team_service'],env=env)
    run(['python3',tool,'bootstrap','--team','TLS guide lab','--username','leader','--display-name','Leader',
         '--output',str(private/'leader.json')],env=env)
    assert len(json.loads((private/'leader.json').read_text())['temporaryPassword'])==21
    cfg={'listen':f'127.0.0.1:{apiport}','database':{'host':'localhost','port':pgport,'database':'termterm_team',
         'username':'termterm_team_service','password':service_password,'caPath':'/etc/termterm-team/postgres-ca.crt'}}
    config=runtime/'service.json';config.write_text(json.dumps(cfg));os.chown(config,0,10001);config.chmod(0o640)
    team=name+'-api';containers.append(team)
    run(['docker','run','-d','--name',team,'--network','host','--read-only','--cpus','1','--memory','512m',
         '-v',str(runtime)+':/etc/termterm-team:ro','termterm/team-service:0.4.1'])
    caddyfile=private/'Caddyfile'
    caddyfile.write_text(f'{{\n admin off\n auto_https disable_redirects\n}}\nhttps://team.sirket.test:{httpsport} {{\n bind 127.0.0.1\n tls internal\n reverse_proxy 127.0.0.1:{apiport}\n}}\n')
    https=name+'-https';containers.append(https)
    run(['docker','run','-d','--name',https,'--network','host','-v',str(caddyfile)+':/etc/caddy/Caddyfile:ro',
         '-v',str(private/'caddy-data')+':/data',
         'caddy:2.10.2@sha256:c3d7ee5d2b11f9dc54f947f68a734c84e9c9666c92c88a7f30b9cba5da182adb'])
    httpsca=private/'team-https-ca.crt'
    curl=['curl','--silent','--show-error','--fail','--max-time','3','--noproxy','*','--resolve',
          f'team.sirket.test:{httpsport}:127.0.0.1','--cacert',str(httpsca),f'https://team.sirket.test:{httpsport}/ready']
    for _ in range(80):
        copy=subprocess.run(['docker','cp',https+':/data/caddy/pki/authorities/local/root.crt',str(httpsca)],capture_output=True)
        if copy.returncode==0:
            result=subprocess.run(curl,capture_output=True,text=True)
            if result.returncode==0:break
        time.sleep(.25)
    else:raise AssertionError('HTTPS with separate company CA did not become ready')
    assert json.loads(result.stdout)['schemaVersion']==64
    wrong_ca=curl.copy();wrong_ca[wrong_ca.index('--cacert')+1]=str(ca)
    assert subprocess.run(wrong_ca,capture_output=True).returncode==60, 'DB CA must not trust HTTPS CA'
    denied=subprocess.run(query,env={**env,'PGSSLROOTCERT':str(httpsca)},capture_output=True)
    assert denied.returncode!=0 and b'certificate' in denied.stderr, 'HTTPS CA must not trust DB CA'
    print('PASS: documented encrypted CA/OpenSSL SAN recipe; private key permissions; ALTER SYSTEM + HBA validation/restart; TLS/SCRAM; plaintext/wrong hostname/wrong CA rejected; 64 migrations + leader; real Docker API/Caddy with exported company CA and HTTPS readiness.')
finally:
    for container in containers:subprocess.run(['docker','rm','-f',container],capture_output=True)
    if cluster.exists():subprocess.run(['pg_dropcluster','--stop','18',name],capture_output=True)
