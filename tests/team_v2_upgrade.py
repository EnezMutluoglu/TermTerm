"""Real PostgreSQL TLS upgrade/backup/restore/crash-resume, isolated databases only."""
import hashlib, http.client, importlib.util, json, os
from pathlib import Path
import secrets, shutil, signal, ssl, subprocess, sys, time, uuid
ROOT=Path(__file__).resolve().parents[1]
PRIVATE=Path('/var/lib/termterm-team-upgrade-tests')
OWNER='termterm_team_upgrade_owner'; SERVICE='termterm_team_upgrade_service'
CA='/etc/postgresql/18/termterm/tls/ca.crt'

def run(args,**kw):
 p=subprocess.run(args,text=True,capture_output=True,**kw)
 if p.returncode: raise RuntimeError('Qualification failed: '+args[0]+' '+p.stderr[-400:])
 return p.stdout.strip()
def sql(q,db='postgres'):
 return run(['runuser','-u','postgres','--','psql','-X','-qAt','-v','ON_ERROR_STOP=1','-p','55432','-d',db],input=q)
def invoke(tool,args,env,success=True):
 p=subprocess.run(['python3',str(tool),*args],env=env,text=True,capture_output=True)
 assert (p.returncode==0)==success,(args[0],p.stderr[-400:])
 return p

def seed_team(db):
 """Synthetic encrypted payloads; exercise real account/resource/ACL relationships."""
 def lit(value):return 'NULL' if value is None else "'"+str(value).replace("'","''")+"'"
 def call(name,*args):
  return json.loads(sql('SET ROLE '+OWNER+'; SELECT termterm_team_v2.'+name+'('+','.join(map(lit,args))+')',db))
 b=call('bootstrap_team','Upgrade Üretim Benzeri Test','leader','Lider')
 def login(user,pw):return call('api_login',b['teamId'],user,pw,uuid.uuid4(),'Upgrade fixture')
 pw='Disposable-Upgrade-Password-123!'
 first=login('leader',b['temporaryPassword'])
 call('api_activate',first['accessToken'],pw,chr(92)+'x'+'ab'*32,chr(92)+'x'+'cd'*128)
 token=login('leader',pw)['accessToken']
 member=call('api_create_member',token,'operator','Operatör','operator')
 activation=login('operator',member['temporaryPassword'])
 call('api_activate',activation['accessToken'],pw,chr(92)+'x'+'de'*32,chr(92)+'x'+'ef'*128)
 vault=str(uuid.uuid4());host=str(uuid.uuid4())
 call('api_vault_create',token,vault,chr(92)+'x'+'aa'*100,1,chr(92)+'x'+'bb'*120)
 p=call('api_resource_recipients',token,vault,host,None)
 draft=dict(operationId=str(uuid.uuid4()),sequence=1,vaultId=vault,resourceId=host,expectedRevision=0,
  aclRevision=p['aclRevision'],writeCapability=p['writeCapability'],clientAt='2026-10-08T00:00:00Z',
  kind='host',parentId=None,deleted=False,encryptedName='ab'*80,encryptedAddress='bc'*80,
  encryptedUsername=None,protocol='ssh',port=22,usernamePolicy='personal',snapshot='cd'*100,
  envelopes=[dict(accountId=x['accountId'],identityVersion=x['identityVersion'],envelope='de'*120) for x in p['recipients']])
 call('api_resource_save',token,json.dumps(draft))
 sql('SET ROLE '+OWNER+'; INSERT INTO termterm_team_v2.resource_acl(team_id,resource_id,account_id,permission,effect) VALUES('+','.join(map(lit,[b['teamId'],host,member['accountId'],'export','deny']))+')',db)

 return {'teamId':b['teamId'],'leader':token,'member':login('operator',pw)['accessToken'],'password':pw}

def fixture_fingerprint(db):
 # Small disposable fixture only; never emit row content or credential hashes.
 tables=['teams','team_accounts','team_memberships','account_crypto_identities','vaults','resources',
         'hosts','resource_versions','resource_key_envelopes','resource_acl','change_operations']
 return {table:sql("SELECT md5(coalesce(string_agg(row_to_json(t)::text,E'\\n' ORDER BY row_to_json(t)::text),'')) FROM termterm_team_v2."+table+' t',db) for table in tables}

def main():
 assert os.geteuid()==0
 PRIVATE.mkdir(mode=0o700,parents=True,exist_ok=True)
 config=PRIVATE/'roles.json'
 if not config.exists():config.write_text(json.dumps({OWNER:secrets.token_hex(32),SERVICE:secrets.token_hex(32)}));config.chmod(0o600)
 passwords=json.loads(config.read_text())
 for role,pw in passwords.items():
  assert role in (OWNER,SERVICE) and all(c in '0123456789abcdef' for c in pw)
  sql(f"DO $$ BEGIN IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='{role}') THEN CREATE ROLE {role} LOGIN PASSWORD '{pw}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT; END IF; END $$")
 case=PRIVATE/uuid.uuid4().hex;case.mkdir(mode=0o700)
 base='termterm_team_upgrade_'+uuid.uuid4().hex[:8]
 def database(suffix):
  name=base+suffix;sql(f'CREATE DATABASE {name} OWNER {OWNER}')
  sql(f'REVOKE ALL ON DATABASE {name} FROM PUBLIC; GRANT CONNECT ON DATABASE {name} TO {OWNER}',name)
  return name
 source=database('');verify=database('_verify');resume_db=database('_resume');resume_verify=database('_resume_verify')
 sql(f'GRANT CONNECT ON DATABASE {source} TO {SERVICE}',source)
 passfile=case/'pgpass';passfile.write_text(f'localhost:55432:*:{OWNER}:{passwords[OWNER]}\n');passfile.chmod(0o600)
 env={**os.environ,'PGHOST':'localhost','PGPORT':'55432','PGDATABASE':source,'PGUSER':OWNER,'PGSSLROOTCERT':CA,'PGPASSFILE':str(passfile)}
 old=case/'old';shutil.copytree(ROOT/'team-service',old,ignore=shutil.ignore_patterns('target','.git'))
 (old/'migrations/064_proxy_restore.sql').unlink()
 g=old/'migrations/grant_service.sql';g.write_text('\n'.join(line for line in g.read_text().splitlines() if 'api_proxy_profile' not in line))
 contract=json.loads((old/'compatibility.json').read_text());contract['schemaVersion']=63;(old/'compatibility.json').write_text(json.dumps(contract))
 for name in (source,resume_db):
  invoke(old/'deploy/manage.py',['migrate','--service-role',SERVICE],{**env,'PGDATABASE':name})
  sql("SET ROLE "+OWNER+"; CREATE TABLE termterm_team_v2.upgrade_probe(id integer PRIMARY KEY, payload bytea NOT NULL); INSERT INTO termterm_team_v2.upgrade_probe VALUES(1,decode('00ffabcd','hex'));",name)
 seeded=seed_team(source)
 original_records=fixture_fingerprint(source)
 tool=ROOT/'team-service/deploy/upgrade.py';plan=case/'plan.json';state=case/'state'
 invoke(tool,['plan','--service-role',SERVICE,'--output',str(plan)],env)
 invoke(ROOT/'team-service/deploy/manage.py',['migrate','--service-role',SERVICE],env,False)
 data=json.loads(plan.read_text());assert data['sourceSchema']==63 and [x['version'] for x in data['pending']]==[64]
 before=sql('SELECT count(*) FROM termterm_team_v2.schema_migrations',source)
 apply=['apply','--plan',str(plan),'--state-directory',str(state),'--verification-database',verify,'--service-stopped']
 invoke(tool,apply[:-1],env,False)
 assert sql('SELECT count(*) FROM termterm_team_v2.schema_migrations',source)==before
 # Wrong identity must fail before backup/migration.
 bad=case/'bad.json';changed=json.loads(plan.read_text());changed['target']['oid']='0';bad.write_text(json.dumps(changed))
 invoke(tool,['apply','--plan',str(bad),'--state-directory',str(case/'bad-state'),'--verification-database',verify,'--service-stopped'],env,False)
 # A failed restore must never migrate the source or overwrite verification objects.
 sql('CREATE TABLE public.keep_me(id int); INSERT INTO public.keep_me VALUES(42)',verify)
 invoke(tool,apply,env,False)
 assert sql('SELECT max(version) FROM termterm_team_v2.schema_migrations',source)=='63'
 assert sql('SELECT id FROM public.keep_me',verify)=='42'
 # Only the test's known disposable table is removed, not a user database.
 sql('DROP TABLE public.keep_me',verify)
 invoke(tool,apply,env)
 result=json.loads((state/'state.json').read_text());assert result['phase']=='awaiting_service'
 backup=state/result['backup'];assert hashlib.sha256(backup.read_bytes()).hexdigest()==result['backupSha256']
 assert backup.stat().st_mode&0o077==0
 assert sql('SELECT max(version) FROM termterm_team_v2.schema_migrations',verify)=='63'
 assert sql("SELECT encode(payload,'hex') FROM termterm_team_v2.upgrade_probe",verify)=='00ffabcd'
 assert sql("SELECT encode(payload,'hex') FROM termterm_team_v2.upgrade_probe",source)=='00ffabcd'
 assert fixture_fingerprint(source)==original_records
 assert fixture_fingerprint(verify)==original_records
 invoke(tool,apply,env) # completed SQL replay does not duplicate any records
 assert sql('SELECT count(*) FROM termterm_team_v2.service_installation',source)=='1'
 assert fixture_fingerprint(source)==original_records
 print('PASS: 63→64 read-only plan, wrong-target/stopped checks, failed-restore protection, real private dump/restore, preserved ciphertext and replay',flush=True)
 # Interrupted migration resumes the same package; each revision+hash is atomic.
 future=case/'future';shutil.copytree(ROOT/'team-service',future,ignore=shutil.ignore_patterns('target','.git'))
 (future/'migrations/065_test_only.sql').write_text("BEGIN; CREATE TABLE termterm_team_v2.resume_probe(id int PRIMARY KEY); INSERT INTO termterm_team_v2.resume_probe VALUES(1); INSERT INTO termterm_team_v2.schema_migrations(version) VALUES(65); COMMIT;\n")
 (future/'migrations/066_test_only.sql').write_text("BEGIN; SELECT pg_sleep(5); INSERT INTO termterm_team_v2.resume_probe VALUES(2); INSERT INTO termterm_team_v2.schema_migrations(version) VALUES(66); COMMIT;\n")
 contract=json.loads((future/'compatibility.json').read_text());contract['schemaVersion']=66;(future/'compatibility.json').write_text(json.dumps(contract))
 ft=future/'deploy/upgrade.py';fe={**env,'PGDATABASE':resume_db};fp=case/'resume-plan.json';fs=case/'resume-state'
 invoke(ft,['plan','--service-role',SERVICE,'--output',str(fp)],fe)
 fa=['apply','--plan',str(fp),'--state-directory',str(fs),'--verification-database',resume_verify,'--service-stopped']
 child=subprocess.Popen(['python3',str(ft),*fa],env=fe,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
 try:
  for _ in range(240):
   if sql('SELECT max(version) FROM termterm_team_v2.schema_migrations',resume_db)=='65':break
   assert child.poll() is None,'Upgrade ended before interruption point';time.sleep(.1)
  else:raise AssertionError('Migration checkpoint not reached')
  os.killpg(child.pid,signal.SIGTERM);child.wait(timeout=5)
 finally:
  if child.poll() is None:os.killpg(child.pid,signal.SIGKILL);child.wait()
 # PostgreSQL may finish the currently executing statement before observing
 # the lost socket. Do not retry until that backend releases its session lock.
 for _ in range(100):
  if sql("SELECT pg_try_advisory_lock(hashtextextended(current_database()||':termterm-team-migration',0))",resume_db)=='t':break
  time.sleep(.1)
 else:raise AssertionError('Interrupted backend did not drain')
 assert sql('SELECT max(version) FROM termterm_team_v2.schema_migrations',resume_db)=='65'
 assert sql('SELECT count(*) FROM termterm_team_v2.migration_file_hashes WHERE version=65',resume_db)=='1'
 invoke(ft,fa,fe)
 assert sql('SELECT count(*) FROM termterm_team_v2.resume_probe',resume_db)=='2'
 assert sql('SELECT max(version) FROM termterm_team_v2.schema_migrations',resume_db)=='66'
 # Changed applied migrations and schema downgrades are refused.
 invoke(tool,['plan','--service-role',SERVICE,'--output',str(case/'downgrade.json')],fe,False)
 p=future/'migrations/065_test_only.sql';p.write_text(p.read_text()+'-- changed applied source\n')
 invoke(ft,['plan','--service-role',SERVICE,'--output',str(case/'changed.json')],fe,False)
 print('PASS: forced process interruption, atomic revision/checksum, bounded resume, no duplicate write, checksum and downgrade rejection',flush=True)
 # Use a real updated service behind Caddy for lock/readiness/certificate checks.
 sys.path.insert(0,str(ROOT/'team-service/deploy'));import upgrade
 service=Path('/root/termterm-team-v2-build/team-service/target/debug/termterm-team-service')
 cfg=case/'service.json';cfg.write_text(json.dumps({'listen':'127.0.0.1:41854','database':{'host':'localhost','port':55432,'database':source,'username':SERVICE,'password':passwords[SERVICE],'caPath':CA}}));cfg.chmod(0o600)
 caddy=case/'Caddyfile';caddy.write_text('{\n admin off\n auto_https disable_redirects\n}\nhttps://localhost:41855 {\n bind 127.0.0.1\n tls internal\n reverse_proxy 127.0.0.1:41854\n}\n')
 container='termterm-team-upgrade-qualification';process=subprocess.Popen([str(service),str(cfg)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 def request(path,context,body=None,token=None,version='0.4.1'):
  c=http.client.HTTPSConnection('localhost',41855,context=context,timeout=20)
  try:
   headers={'Content-Type':'application/json','X-TermTerm-Client-Version':version,'X-TermTerm-API-Version':'1'}
   if token:headers['Authorization']='Bearer '+token
   c.request('POST' if body is not None else 'GET',path,None if body is None else json.dumps(body),headers)
   r=c.getresponse();return r.status,json.loads(r.read())
  finally:c.close()
 def get(path,context):
  c=http.client.HTTPSConnection('localhost',41855,context=context,timeout=3)
  try:c.request('GET',path);r=c.getresponse();return r.status,json.loads(r.read())
  finally:c.close()
 try:
  run(['docker','run','-d','--name',container,'--network','host','-v',str(caddy)+':/etc/caddy/Caddyfile:ro','-v',str(case/'caddy-data')+':/data','caddy:2.10.2@sha256:c3d7ee5d2b11f9dc54f947f68a734c84e9c9666c92c88a7f30b9cba5da182adb'])
  ca=case/'caddy-data/caddy/pki/authorities/local/root.crt'
  for _ in range(120):
   try:
    context=ssl.create_default_context(cafile=str(ca))
    if get('/ready',context)[0]==200:break
   except (OSError,ValueError,http.client.HTTPException):pass
   time.sleep(.25)
  else:raise AssertionError('New HTTPS service not ready')
  with upgrade.Database(env) as lock:
   assert lock.sql('SELECT pg_try_advisory_lock('+upgrade.LOCK+')')=='t'
   status,body=get('/ready',context);assert status==503 and body['error']=='service_maintenance'
  assert get('/ready',context)[0]==200
  # Wrong DB identity/CA cannot mark an upgrade complete.
  original=(state/'state.json').read_text();tamper=json.loads(original);tamper['databaseVersion']['installationId']=str(uuid.uuid4());(state/'state.json').write_text(json.dumps(tamper))
  try:invoke(tool,['confirm','--state-directory',str(state),'--url','https://localhost:41855','--ca-file',str(ca)],env,False)
  finally:(state/'state.json').write_text(original)
  invoke(tool,['confirm','--state-directory',str(state),'--url','https://localhost:41855','--ca-file',CA],env,False)
  invoke(tool,['confirm','--state-directory',str(state),'--url','https://localhost:41855','--ca-file',str(ca)],env)
  assert json.loads((state/'state.json').read_text())['phase']=='ready'
  print('PASS: real maintenance request rejection/recovery, HTTPS CA and installed package/database-bound readiness confirmation',flush=True)

  # Team-scoped administrator choice; no frontend-supplied actor or team scope.
  status,p=request('/v1/client-policy',context,token=seeded['leader']);assert status==200 and p['revision']==0
  status,_=request('/v1/client-policy',context,{'expectedRevision':0,'version':'0.4.1'},seeded['member']);assert status==403
  assert request('/v1/client-policy',context,{'expectedRevision':0,'version':'999.0.0'},seeded['leader'])[0]==400
  choice={'expectedRevision':0,'version':'0.4.1'}
  assert request('/v1/client-policy',context,choice,seeded['leader'])[1]['revision']==1
  assert request('/v1/client-policy',context,choice,seeded['leader'])[1]['revision']==1
  assert request('/v1/client-policy',context,{'expectedRevision':9,'version':'0.4.1'},seeded['leader'])[0]==409
  assert sql("SELECT count(*) FROM termterm_team_v2.client_update_history WHERE team_id='"+seeded['teamId']+"'",source)=='1'
  assert sql("SELECT count(*) FROM termterm_team_v2.audit_events WHERE action='client_update.required'",source)=='1'
  other=seed_team(source)
  # Simulate a future package's selected policy in this isolated database. No
  # fake public release or installer is downloaded/published by this test.
  sql("SET ROLE "+OWNER+"; SELECT termterm_team_v2.api_client_policy_set('"+seeded['leader']+"',1,'0.4.2')",source)
  assert request('/v1/me',context,token=seeded['member'])[0]==426
  assert request('/v1/me',context,token=seeded['leader'])[0]==426
  assert request('/v1/me?teamId='+other['teamId'],context,token=seeded['member'])[0]==426
  assert request('/v1/me',context,token=other['member'])[0]==200
  assert request('/v1/me',context,token=seeded['member'],version='0.4.2')[0]==200
  assert request('/client-policy?teamId='+seeded['teamId'],context)[1]['minimumClientVersion']=='0.4.2'
  assert request('/client-policy?teamId='+other['teamId'],context)[1]['minimumClientVersion']=='0.4.1'
  login_body=dict(teamId=seeded['teamId'],username='operator',password=seeded['password'],deviceId=str(uuid.uuid4()),deviceName='Policy test')
  assert request('/v1/auth/login',context,login_body)[0]==426
  assert request('/v1/auth/login',context,login_body,version='0.4.2')[0]==200
  print('PASS: real HTTPS owner selection, member denial, stale revision, safe replay, audit, team isolation, login/runtime mandatory update, compatible client access',flush=True)

 finally:
  process.terminate();process.wait(timeout=10)
  subprocess.run(['docker','rm','-f',container],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 print('Private evidence retained under '+str(case))
if __name__=='__main__':main()
