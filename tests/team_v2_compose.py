"""Both packaged Compose variants; disposable WSL-only qualification state."""
import http.client
import hashlib
import json
import os
from pathlib import Path
import ssl
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
VERSION = json.loads((ROOT/'package.json').read_text())['version']
DELIVERY = 'team-'+VERSION if '-dev.' in VERSION else 'release-'+VERSION
DEPLOY = Path(os.environ.get('TERMTERM_TEST_STAGE', str(ROOT/f'artifacts/{DELIVERY}/team-service-linux-x64')))/'deploy'
IMAGE_ARCHIVE = os.environ.get('TERMTERM_TEST_IMAGE')
RUN=uuid.uuid4().hex[:10]
PRIVATE = Path('/var/lib/termterm-compose-qualification')/RUN
assert DEPLOY.is_dir()
subprocess.run(['python3',str(DEPLOY/'prepare-compose.py'),'--directory',str(PRIVATE),'--port','55434'],check=True,stdout=subprocess.DEVNULL)
config_file=PRIVATE/'config/service.json'
config=json.loads(config_file.read_text());config['listen']='127.0.0.1:41851'
config_file.write_text(json.dumps(config))
EXPECTED=json.loads((DEPLOY.parent/'compatibility.json').read_text())['schemaVersion']
environment = PRIVATE/'test.env'
caddy = PRIVATE/'Caddyfile.test'
caddy.write_text('''{
 admin off
 auto_https disable_redirects
}
https://localhost:41853 {
 bind 127.0.0.1
 tls internal
 reverse_proxy 127.0.0.1:41851
 header Cache-Control no-store
 request_body {
  max_size 256KB
 }
}
''')
environment.write_text((PRIVATE/'deployment.env').read_text().replace('replace-with-your-dns-name','localhost:41853')+
                       f'TERMTERM_CADDY_FILE={caddy}\nTERMTERM_HEALTH_URL=http://127.0.0.1:41851/health\n')
base = ['docker','compose','--project-name','termterm-team-qualification-'+RUN,'--env-file',str(environment),
        '-f',str(DEPLOY/'compose.yaml')]
separate = base + ['-f',str(DEPLOY/'compose.postgres.yaml')]


def run(args, **kw):
    p = subprocess.run(args, capture_output=True, text=True, **kw)
    if p.returncode:
        raise RuntimeError('Compose qualification command failed: '+args[0])
    return p.stdout.strip()


def get(path, context):
    c = http.client.HTTPSConnection('localhost',41853,context=context,timeout=3)
    try:
        c.request('GET',path,headers={'X-TermTerm-Client-Version':'0.4.1','X-TermTerm-API-Version':'1'});r=c.getresponse();return r.status,json.loads(r.read())
    finally:
        c.close()


try:
    if IMAGE_ARCHIVE:
        archive=Path(IMAGE_ARCHIVE)
        metadata=json.loads(archive.with_name(archive.name.removesuffix('.tar.gz')+'.json').read_text())
        with archive.open('rb') as stream:
            assert hashlib.file_digest(stream,'sha256').hexdigest()==metadata['archive']['sha256']
        run(['docker','image','load','-i',str(archive)])
        image_info=json.loads(run(['docker','image','inspect',metadata['image']]))[0]
        assert image_info['Id']==metadata['imageId']
        assert image_info['Config']['User']=='10001:10001'
        digest=run(['docker','run','--rm','--network','none','--entrypoint','sha256sum',metadata['image'],
                    '/usr/local/bin/termterm-team-service']).split()[0]
        assert digest==metadata['binarySha256']==hashlib.sha256((DEPLOY.parent/'bin/termterm-team-service').read_bytes()).hexdigest()
        release=run(['docker','run','--rm','--network','none','--entrypoint','cat',metadata['image'],'/etc/os-release'])
        assert 'VERSION_ID="13"' in release and 'VERSION_CODENAME=trixie' in release
    run(separate+['up','-d','--build','postgres'])
    db_env={**os.environ,'PGHOST':'localhost','PGPORT':'55434','PGDATABASE':'termterm_team','PGUSER':'termterm_team_migrator',
      'PGPASSFILE':str(PRIVATE/'migration.pgpass'),'PGSSLROOTCERT':str(PRIVATE/'config/postgres-ca.crt'),'PGSSLMODE':'verify-full'}
    for _ in range(80):
        if subprocess.run(['psql','-X','-qAt','-c','SELECT 1'],env=db_env,capture_output=True).returncode==0:break
        time.sleep(.5)
    else:raise AssertionError('Fresh isolated PostgreSQL did not become ready')
    run(['python3',str(DEPLOY/'manage.py'),'migrate','--service-role','termterm_team_service'],env=db_env)
    run(separate + ['up','-d','--no-build' if IMAGE_ARCHIVE else '--build','team','https'])
    ca = PRIVATE/'https-ca.crt'
    for _ in range(80):
        p = subprocess.run(separate+['cp','https:/data/caddy/pki/authorities/local/root.crt',str(ca)],capture_output=True)
        if p.returncode == 0:
            context = ssl.create_default_context(cafile=str(ca))
            try:
                if get('/ready',context)[0] == 200:break
            except (OSError,ValueError,http.client.HTTPException):pass
        time.sleep(.5)
    else:raise AssertionError('Packaged Compose service did not become ready')
    assert get('/ready',context)[1]['schemaVersion'] == EXPECTED
    cid=run(base+['ps','-q','team'])
    details=json.loads(run(['docker','inspect',cid]))[0]
    limits=details['HostConfig']
    assert limits['Memory']==512*1024*1024 and limits['MemorySwap']==limits['Memory']
    assert limits['NanoCpus']==1_000_000_000 and limits['PidsLimit']==128
    assert limits['ReadonlyRootfs'] and limits['RestartPolicy']['Name']=='unless-stopped'
    assert limits['LogConfig']['Config']=={'max-size':'10m','max-file':'3'}
    for _ in range(80):
        if json.loads(run(['docker','inspect',cid]))[0]['State']['Health']['Status']=='healthy':break
        time.sleep(.5)
    else:raise AssertionError('Image healthcheck failed')
    # Simulate a real crash from the parent namespace, not Docker's manual stop.
    prior=details['RestartCount']
    import signal
    assert details['State']['Pid']>1
    os.kill(details['State']['Pid'],signal.SIGKILL)
    for _ in range(80):
        info=json.loads(run(['docker','inspect',cid]))[0]
        if info['RestartCount']>prior:
            try:
                if get('/ready',context)[0]==200:break
            except (OSError,ValueError,http.client.HTTPException):pass
        time.sleep(.5)
    else:raise AssertionError('Crashed Team process did not restart and become ready')
    assert get('/v1/me',context)[0] == 401
    try:get('/health',ssl.create_default_context())
    except ssl.SSLCertVerificationError:pass
    else:raise AssertionError('Untrusted HTTPS CA accepted')
    # Private service role must not read the normalized tables directly.
    cfg=json.loads((PRIVATE/'config/service.json').read_text())['database']
    pgpass=PRIVATE/'service.pgpass';pgpass.write_text(f"localhost:55434:termterm_team:{cfg['username']}:{cfg['password']}\n");pgpass.chmod(0o600)
    env={**os.environ,'PGHOST':'localhost','PGPORT':'55434','PGDATABASE':'termterm_team','PGUSER':cfg['username'],
         'PGPASSFILE':str(pgpass),'PGSSLROOTCERT':str(PRIVATE/'config/postgres-ca.crt'),'PGSSLMODE':'verify-full'}
    psql=['psql','-X','-qAt','-v','ON_ERROR_STOP=1']
    assert subprocess.run(psql,input='SELECT * FROM termterm_team_v2.team_accounts;',text=True,env=env,capture_output=True).returncode != 0
    assert subprocess.run(psql,input='SELECT 1;',text=True,env={**env,'PGSSLMODE':'disable'},capture_output=True).returncode != 0
    # Switch the packaged service to the already-provisioned external TLS PG.
    original=(PRIVATE/'config/service.json').read_bytes()
    external=json.loads((Path('/var/lib/termterm-team-qualification')/'service.json').read_text())
    external['listen']='127.0.0.1:41851';external['database']['caPath']='/etc/termterm-team/postgres-ca-external.crt'
    import shutil
    shutil.copy2('/etc/postgresql/18/termterm/tls/ca.crt',PRIVATE/'config/postgres-ca-external.crt')
    (PRIVATE/'config/postgres-ca-external.crt').chmod(0o644)
    try:
        (PRIVATE/'config/service.json').write_text(json.dumps(external))
        run(base+['restart','team'])
        for _ in range(40):
            try:
                if get('/ready',context)[0] == 200:break
            except (OSError,ValueError,http.client.HTTPException):pass
            time.sleep(.25)
        else:raise AssertionError('Existing-PostgreSQL Compose connection failed')
        assert get('/ready',context)[1]['schemaVersion']==EXPECTED
    finally:
        (PRIVATE/'config/service.json').write_bytes(original)
    print('PASS: packaged Compose with dedicated/existing TLS PostgreSQL, HTTPS CA, limited roles, TCP non-TLS rejection, image load, resource limits, healthcheck and crash/restart.')
finally:
    # Preserve disposable volumes/config for repeatable qualification; stop only
    # these named project services, never unrelated SQL Workspace containers.
    subprocess.run(separate+['stop','team','https','postgres'],capture_output=True)
