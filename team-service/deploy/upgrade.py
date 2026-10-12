#!/usr/bin/env python3
"""Explicit, resumable Team schema upgrade. Never installs a binary or downgrades data."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import ssl
import subprocess
import sys
import threading
import time
import urllib.request
import uuid
import fcntl
import manage

ROOT = Path(__file__).resolve().parents[1]
LOCK = "hashtextextended(current_database() || ':termterm-team-migration',0)"

def digest(path):
    with Path(path).open('rb') as f: return hashlib.file_digest(f,'sha256').hexdigest()

def encoded(value): return json.dumps(value,sort_keys=True,separators=(',',':')).encode()

def timestamp(): return time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime())

def check(ok,message):
    if not ok: raise ValueError(message)

class Database:
    """Single libpq session: advisory ownership survives individual SQL commits."""
    def __init__(self, env=None):
        self.env=env or manage.environment()
        self.p=subprocess.Popen(['psql','-X','-qAt','-v','ON_ERROR_STOP=1'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,text=True,bufsize=1,env=self.env)
        self.lines=queue.Queue()
        def pump():
            for line in self.p.stdout: self.lines.put(line.rstrip('\n'))
            self.lines.put(None)
        threading.Thread(target=pump,daemon=True).start()
    def sql(self,text,timeout=360):
        marker='tt_done_'+uuid.uuid4().hex
        try:
            self.p.stdin.write(text+'\n;\nSELECT '+manage.literal(marker)+';\n');self.p.stdin.flush()
            result=[];deadline=time.monotonic()+timeout
            while True:
                line=self.lines.get(timeout=max(.01,deadline-time.monotonic()))
                if line==marker:return '\n'.join(result).strip()
                if line is None:raise RuntimeError('Database operation failed; phase recorded, no automatic retry')
                result.append(line)
        except (queue.Empty,BrokenPipeError):
            self.close();raise RuntimeError('Database operation interrupted/timed out; inspect state before resuming')
    def value(self,sql): return json.loads(self.sql(sql))
    def close(self):
        if self.p.poll() is None:
            self.p.terminate()
            try:self.p.wait(timeout=5)
            except subprocess.TimeoutExpired:self.p.kill();self.p.wait()
        for stream in (self.p.stdin,self.p.stdout):
            if stream:stream.close()
    def __enter__(self):return self
    def __exit__(self,*_):self.close()

def bundle():
    files=manage.migration_files()
    contract=json.loads((ROOT/'compatibility.json').read_text())
    check(contract.get('formatVersion')==1 and contract.get('schemaVersion')==len(files) and contract.get('apiVersion')==1,
          'Package compatibility contract and migration sequence disagree')
    migrations=[{'version':i,'file':p.name,'sha256':hashlib.sha256(p.read_text(encoding='utf-8').encode()).hexdigest()} for i,p in enumerate(files,1)]
    data={'contract':contract,'migrations':migrations,'grantsSha256':hashlib.sha256((ROOT/'migrations/grant_service.sql').read_text().encode()).hexdigest()}
    data['tools']={p.name:hashlib.sha256(p.read_text(encoding='utf-8').encode()).hexdigest() for p in [ROOT/'deploy/manage.py',ROOT/'deploy/upgrade.py']}
    data['packageId']=hashlib.sha256(encoded(data)).hexdigest()
    return data

def inspect(db,role):
    check(re.fullmatch(r'[a-z_][a-z0-9_]{0,62}',role),'Invalid service role')
    check(db.sql("SELECT to_regclass('termterm_team_v2.schema_migrations') IS NOT NULL")=='t','Use manage.py migrate for first installation')
    b=bundle()
    target=db.value("""SELECT json_build_object('database',current_database(),'oid',oid::text,'migrationUser',current_user,
       'serverVersion',current_setting('server_version_num'),'isOwner',pg_has_role(current_user,datdba,'MEMBER'))
       FROM pg_database WHERE datname=current_database()""")
    check(target['isOwner'],'Migration login must own the dedicated Team database')
    target['databaseScope']=db.sql("SELECT encode(public.digest(current_database()||':'||(SELECT oid::text FROM pg_database WHERE datname=current_database()),'sha256'),'hex')")
    target.update(host=db.env['PGHOST'],port=db.env.get('PGPORT','5432'))
    check(target['migrationUser']!=role,'Migration and service accounts must differ')
    versions=db.value("SELECT coalesce(json_agg(version ORDER BY version),'[]') FROM termterm_team_v2.schema_migrations")
    check(versions and versions==list(range(1,max(versions)+1)),'Applied schema revisions have gaps')
    check(max(versions)<=b['contract']['schemaVersion'],'Database is newer; automatic downgrade is forbidden')
    check(max(versions)>=b['contract']['minimumUpgradeSchema'],'Source schema not supported')
    check(db.sql("SELECT to_regclass('termterm_team_v2.migration_file_hashes') IS NOT NULL")=='t',
          'Missing migration identities; explicitly verify/adopt installed source with manage.py migrate first')
    hashes=db.value("SELECT coalesce(json_object_agg(version::text,sha256),'{}') FROM termterm_team_v2.migration_file_hashes")
    check(len(hashes)==len(versions),'Incomplete or unexpected migration identities')
    for v in versions:check(hashes.get(str(v))==b['migrations'][v-1]['sha256'],'Applied migration checksum mismatch')
    check(db.sql("SELECT count(*) FROM pg_roles WHERE rolname="+manage.literal(role)+" AND NOT (rolsuper OR rolbypassrls OR rolcreatedb OR rolcreaterole)")=='1','Limited service role required')
    return {'formatVersion':1,'target':target,'serviceRole':role,'package':b,'sourceSchema':max(versions),
            'pending':[x for x in b['migrations'] if x['version']>max(versions)],
            'rollback':'Previous binary plus original verified DB backup; later writes are not merged automatically'}

def save_state(directory,state):
    temp=directory/('state-'+uuid.uuid4().hex+'.tmp')
    fd=os.open(temp,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
    with os.fdopen(fd,'wb') as f:f.write(encoded(state));f.flush();os.fsync(f.fileno())
    os.replace(temp,directory/'state.json')
    fd=os.open(directory,os.O_RDONLY)
    try:os.fsync(fd)
    finally:os.close(fd)

def phase(directory,state,name,**values):
    state.update(phase=name,updatedAt=timestamp(),**values);save_state(directory,state)
    print('Upgrade phase: '+name,flush=True)

def subprocess_checked(args,env,**kw):
    p=subprocess.run(args,env=env,stderr=subprocess.PIPE,**kw)
    if p.returncode:raise RuntimeError(args[0]+' failed; no database downgrade was attempted')

def inventory(db):
    tables=db.value("SELECT coalesce(json_agg(tablename ORDER BY tablename),'[]') FROM pg_tables WHERE schemaname='termterm_team_v2'")
    result={}
    for table in tables:
        name='"'+table.replace('"','""')+'"'
        result[table]=int(db.sql('SELECT count(*) FROM termterm_team_v2.'+name))
    return result

def restore_check(db,backup,verify_database):
    check(verify_database!=db.env['PGDATABASE'],'Verification database must be separate')
    env={**db.env,'PGDATABASE':verify_database}
    with Database(env) as verify:
        verify.sql("SET statement_timeout='30s'; SET lock_timeout='5s';")
        check(verify.sql("SELECT pg_has_role(current_user,datdba,'MEMBER') FROM pg_database WHERE datname=current_database()")=='t', 'Verification database must belong to migration login')
        check(verify.sql("""SELECT NOT EXISTS(SELECT FROM pg_database d,
            LATERAL aclexplode(coalesce(d.datacl,acldefault('d',d.datdba))) a
            WHERE d.datname=current_database() AND a.privilege_type='CONNECT' AND a.grantee<>d.datdba)""")=='t',
            'Verification database must deny CONNECT to PUBLIC and every non-owner; restore does not copy grants')
        # No DROP or --clean is used. Existing objects always require a new empty DB.
        count=verify.sql("""SELECT count(*) FROM pg_namespace WHERE nspname NOT IN ('public','information_schema') AND nspname NOT LIKE 'pg_%'""")
        check(count=='0','Verification database is not empty; nothing was removed')
        check(verify.sql("""SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
            WHERE n.nspname='public' AND NOT EXISTS(SELECT FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=c.oid AND d.deptype='e')""")=='0','Verification public schema contains objects')
        check(verify.sql("""SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
            WHERE n.nspname='public' AND NOT EXISTS(SELECT FROM pg_depend d WHERE d.classid='pg_proc'::regclass AND d.objid=p.oid AND d.deptype='e')""")=='0','Verification public schema contains routines')
    subprocess_checked(['pg_restore','--exit-on-error','--single-transaction','--no-owner','--no-privileges','--dbname',verify_database,str(backup)],env,stdout=subprocess.DEVNULL,timeout=1800)
    with Database(env) as verify:
        check(inventory(verify)==inventory(db),'Restored Team table counts differ; migration was not started')
    return {'database':verify_database,'checkedAt':timestamp(),'method':'real single-transaction restore + Team table counts'}

def apply(plan_path,directory,verification,stopped):
    check(stopped,'Stop ALL old/new Team service instances and pass --service-stopped')
    plan=json.loads(Path(plan_path).read_text())
    check(plan.get('formatVersion')==1,'Unsupported upgrade plan')
    directory=Path(directory).resolve()
    if not directory.exists():directory.mkdir(mode=0o700,parents=True)
    check(not directory.stat().st_mode&0o077,'State/backup directory must have mode 0700')
    with (directory/'operator.lock').open('a') as local_lock:
        fcntl.flock(local_lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        with Database() as db:
            check(db.sql('SELECT pg_try_advisory_lock('+LOCK+')')=='t','Another upgrade/request owns the database maintenance lock; retry after drain')
            current=inspect(db,plan['serviceRole'])
            check(current['package']==plan['package'] and current['target']==plan['target'],'Package or database identity changed; create a fresh plan')
            check(db.sql("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND usename="+manage.literal(plan['serviceRole']))=='0', 'Service database sessions remain; stop every instance')
            state_path=directory/'state.json'
            plan_hash=hashlib.sha256(encoded(plan)).hexdigest()
            if state_path.exists():
                state=json.loads(state_path.read_text());check(state['planHash']==plan_hash,'State belongs to a different plan')
                check(plan['sourceSchema']<=current['sourceSchema']<=plan['package']['contract']['schemaVersion'],'Unexpected resume schema')
            else:
                check(current['sourceSchema']==plan['sourceSchema'],'Schema changed since preview; create a fresh plan')
                state={'formatVersion':1,'planHash':plan_hash,'plan':plan,'startedAt':timestamp()}
                phase(directory,state,'prepared')
            try:
                if not state.get('restoreVerified'):
                    # Preserve interrupted attempts; never overwrite a previous backup.
                    attempt=uuid.uuid4().hex
                    backup=directory/('before-'+attempt+'.dump')
                    phase(directory,state,'backing_up')
                    db.sql('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;')
                    snapshot=db.sql('SELECT pg_export_snapshot();')
                    try:
                        fd=os.open(backup,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
                        with os.fdopen(fd,'wb') as out:
                            subprocess_checked(['pg_dump','--format=custom','--snapshot='+snapshot],db.env,stdout=out,timeout=1800)
                            out.flush();os.fsync(out.fileno())
                        phase(directory,state,'verifying_backup',backup=backup.name,backupSha256=digest(backup))
                        proof=restore_check(db,backup,verification)
                    finally:db.sql('ROLLBACK;')
                    phase(directory,state,'backup_verified',restoreVerified=proof)
                backup=directory/state['backup']
                check(backup.parent==directory and digest(backup)==state['backupSha256'],'Original backup missing or changed; cannot resume')
                phase(directory,state,'migrating')
                manage.migrate(plan['serviceRole'],execute=db.sql)
                after=inspect(db,plan['serviceRole'])
                check(not after['pending'],'Upgrade is incomplete')
                phase(directory,state,'awaiting_service',appliedSchema=after['sourceSchema'],databaseVersion=db.value('SELECT termterm_team_v2.api_version()'))
            except Exception:
                phase(directory,state,'failed',failure='Upgrade stopped; preserve backup/state, inspect and resume with the same package/plan')
                raise
    print('Schema verified. Install the matching service package, start it and run confirm. No stable release was published.')

def _confirm_locked(directory,url,ca):
    directory=Path(directory).resolve();state=json.loads((directory/'state.json').read_text())
    check(state['phase'] in ('awaiting_service','ready'),'Schema upgrade has not reached service validation')
    from urllib.parse import urlparse
    parsed=urlparse(url)
    check(parsed.scheme=='https' and parsed.hostname and not parsed.username and not parsed.password and not parsed.query and not parsed.fragment and parsed.path in ('','/'),'HTTPS service origin required')
    context=ssl.create_default_context(cafile=ca)
    # Do not allow redirecting a readiness check to another origin.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self,*args):return None
    opener=urllib.request.build_opener(urllib.request.HTTPSHandler(context=context),NoRedirect())
    with opener.open(url.rstrip('/')+'/ready',timeout=10) as response:
        raw=response.read(16385);check(len(raw)<=16384,'Readiness response too large');ready=json.loads(raw)
    expected=state['plan']['package']['contract']
    check(ready.get('schemaVersion')==expected['schemaVersion'] and ready.get('apiVersion')==expected['apiVersion'] and ready.get('compatibility')==expected and ready.get('installationId')==state['databaseVersion'].get('installationId') and ready.get('databaseScope')==state['plan']['target']['databaseScope'],'Running service does not match the planned package')
    phase(directory,state,'ready',serviceCheckedAt=timestamp(),serviceOrigin=url.rstrip('/'))
    print('HTTPS readiness/CA and package compatibility verified. Keep rollback backup and previous binary.')

def confirm(directory,url,ca):
    directory=Path(directory).resolve()
    check(directory.is_dir() and not directory.stat().st_mode&0o077,'Private state directory required')
    with (directory/'operator.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        _confirm_locked(directory,url,ca)


def main():
    p=argparse.ArgumentParser(description=__doc__);sub=p.add_subparsers(dest='command',required=True)
    plan=sub.add_parser('plan');plan.add_argument('--service-role',required=True);plan.add_argument('--output',required=True)
    a=sub.add_parser('apply');a.add_argument('--plan',required=True);a.add_argument('--state-directory',required=True);a.add_argument('--verification-database',required=True);a.add_argument('--service-stopped',action='store_true')
    c=sub.add_parser('confirm');c.add_argument('--state-directory',required=True);c.add_argument('--url',required=True);c.add_argument('--ca-file',required=True)
    args=p.parse_args()
    if args.command=='plan':
        with Database() as db:
            db.sql("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET LOCAL statement_timeout='20s';")
            result=inspect(db,args.service_role);db.sql('ROLLBACK;')
        manage.private_write(args.output,result)
        print('Read-only plan saved. Pending migrations: '+str(len(result['pending'])))
    elif args.command=='apply':apply(args.plan,args.state_directory,args.verification_database,args.service_stopped)
    else:confirm(args.state_directory,args.url,args.ca_file)

if __name__=='__main__':
    try:main()
    except (ValueError,OSError,RuntimeError,subprocess.TimeoutExpired) as e:
        # Do not include libpq/server SQL details, passwords or response bodies.
        print(str(e) if isinstance(e,(ValueError,RuntimeError)) else 'Upgrade I/O/connection failure; inspect saved phase and retain backup',file=sys.stderr)
        sys.exit(1)
