"""Create a separate, repeatable WSL lab. Does not change the v1 Team/SQL labs.
Run: wsl -d Ubuntu-24.04 -u root -- python3 /path/to/scripts/team-v2-lab.py
Only the fixed database and dedicated roles below are managed.
"""
import json, os, pathlib, secrets, subprocess
ROOT = pathlib.Path(__file__).resolve().parents[1]
DB = 'termterm_team_v2_e2e'
OWNER = 'termterm_team_v2_lab_owner'
SERVICE = 'termterm_team_v2_lab_service'
def sql(text, database='postgres'):
    result = subprocess.run(['runuser','-u','postgres','--','psql','-X','-qAt','-v','ON_ERROR_STOP=1','-p','55432','-d',database], input=text, text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError('Lab SQL failed: '+result.stderr)
    return result.stdout.strip()
def main():
    # First-install helper must not bypass the backed-up upgrade workflow for
    # an existing lab either. User data is never rebuilt to gain a new schema.
    target=max(int(p.name.split('_')[0]) for p in (ROOT/'team-service/migrations').glob('[0-9]*.sql'))
    if sql("SELECT 1 FROM pg_database WHERE datname='%s'"%DB):
        if sql("SELECT to_regclass('termterm_team_v2.schema_migrations') IS NOT NULL",DB)=='t':
            current=int(sql('SELECT max(version) FROM termterm_team_v2.schema_migrations',DB))
            if current!=target:
                raise RuntimeError(f'Existing Team lab schema {current}, package {target}. Use deploy/upgrade.py plan/apply/confirm with a verified backup; this first-install helper cannot upgrade or downgrade it.')
    lab=ROOT/'.lab'; lab.mkdir(exist_ok=True)
    path=lab/'team-v2-service.json'
    if path.exists(): config=json.loads(path.read_text())
    else:
        config={'listen':'127.0.0.1:41841','database':{'host':'localhost','port':55432,'database':DB,'username':SERVICE,'password':secrets.token_hex(32),'caPath':'/etc/postgresql/18/termterm/tls/ca.crt'}}
        with path.open('x') as f: json.dump(config,f)
        os.chmod(path,0o600)
    pw=config['database']['password']; assert len(pw)==64 and all(x in '0123456789abcdef' for x in pw)
    for role,options in [(OWNER,'NOLOGIN'),(SERVICE,"LOGIN PASSWORD '"+pw+"'")]:
        sql("DO $$ BEGIN IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='%s') THEN CREATE ROLE %s %s NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT; END IF; END $$;"%(role,role,options))
    if not sql("SELECT 1 FROM pg_database WHERE datname='%s'"%DB): sql('CREATE DATABASE '+DB+' OWNER '+OWNER)
    sql('CREATE EXTENSION IF NOT EXISTS pgcrypto',DB)
    sql('REVOKE CREATE ON SCHEMA public FROM PUBLIC; REVOKE ALL ON DATABASE '+DB+' FROM PUBLIC; GRANT CONNECT ON DATABASE '+DB+' TO '+SERVICE,DB)
    for migration in sorted((ROOT/'team-service/migrations').glob('[0-9]*.sql')):
        exists=sql("SELECT to_regclass('termterm_team_v2.schema_migrations') IS NOT NULL",DB)=='t'
        version=int(migration.name.split('_')[0])
        if exists and sql('SELECT 1 FROM termterm_team_v2.schema_migrations WHERE version='+str(version),DB): continue
        sql('SET ROLE '+OWNER+';\n'+migration.read_text(),DB)
    grant=(ROOT/'team-service/migrations/grant_service.sql').read_text().replace(':"service_role"',SERVICE)
    sql('SET ROLE '+OWNER+';\n'+grant,DB)
    sql("ALTER ROLE "+SERVICE+" IN DATABASE "+DB+" SET statement_timeout='15s'; ALTER ROLE "+SERVICE+" IN DATABASE "+DB+" SET lock_timeout='5s'",DB)
    # Discover only the public CA, never copy a server/client private key.
    ssl_ca=sql('SHOW ssl_ca_file',DB)
    if ssl_ca and pathlib.Path(ssl_ca).exists(): config['database']['caPath']=ssl_ca
    path.write_text(json.dumps(config,indent=2)); os.chmod(path,0o600)
    print('Team v2 isolated lab ready: '+DB+'; private config in .lab/team-v2-service.json')
if __name__=='__main__': main()
