"""Exercise the DBA's interactive psql recipe in a disposable networkless PostgreSQL."""
import os
from pathlib import Path
import pty
import re
import secrets
import select
import subprocess
import time
import uuid

root=Path(__file__).resolve().parents[1]
name='termterm-setup-check-'+uuid.uuid4().hex[:10]


def run(*args):
    return subprocess.check_output(args,stderr=subprocess.STDOUT,text=True).strip()


try:
    run('docker','run','-d','--name',name,'--network','none',
        '--tmpfs','/var/lib/postgresql:rw,size=512m','-e','POSTGRES_HOST_AUTH_METHOD=trust',
        'postgres:18-bookworm','-c','listen_addresses=')
    for _ in range(80):
        if subprocess.run(['docker','exec',name,'pg_isready','-U','postgres'],capture_output=True).returncode==0:break
        time.sleep(.5)
    else:raise AssertionError('Isolated PostgreSQL not ready')
    run('docker','cp',str(root/'team-service/deploy/prepare-database.sql'),name+':/tmp/prepare.sql')
    master,slave=pty.openpty()
    process=subprocess.Popen(['docker','exec','-it',name,'psql','-X','-U','postgres','-d','postgres',
                              '-f','/tmp/prepare.sql'],stdin=slave,stdout=slave,stderr=slave)
    os.close(slave)
    transcript=b'';consumed=0;prompts=0;passwords=[secrets.token_hex(16),secrets.token_hex(16)]
    deadline=time.monotonic()+30
    try:
        while time.monotonic()<deadline:
            if select.select([master],[],[],.1)[0]:
                try:chunk=os.read(master,4096)
                except OSError:break
                if not chunk:break
                transcript+=chunk
                while match:=re.search(rb'(?:Enter new password for user [^\r\n]+: |Enter it again: )',transcript[consumed:]):
                    assert prompts<4, 'Unexpected extra password prompt'
                    os.write(master,(passwords[prompts//2]+'\n').encode())
                    prompts+=1;consumed+=match.end()
            if process.poll() is not None:break
        else:raise AssertionError('Interactive password entry timed out')
        assert process.wait(timeout=5)==0 and prompts==4, 'DBA setup failed'
        assert all(value.encode() not in transcript for value in passwords), 'Password echoed'
    finally:
        os.close(master)
        if process.poll() is None:process.kill();process.wait()
    def sql(statement):
        return run('docker','exec',name,'psql','-X','-qAt','-U','postgres','-d','termterm_team','-c',statement)
    assert sql("SELECT count(*) FROM pg_roles WHERE rolname IN ('termterm_team_service','termterm_team_migrator') AND rolcanlogin AND NOT rolsuper AND NOT rolcreatedb AND NOT rolcreaterole AND NOT rolbypassrls AND NOT rolinherit")=='2'
    assert sql("SELECT count(*) FROM pg_authid WHERE rolname IN ('termterm_team_service','termterm_team_migrator') AND rolpassword LIKE 'SCRAM-SHA-256$%'")=='2'
    assert sql("SELECT pg_get_userbyid(datdba) FROM pg_database WHERE datname=current_database()")=='termterm_team_migrator'
    assert sql("SELECT count(*) FROM pg_extension WHERE extname='pgcrypto'")=='1'
    assert sql("SELECT has_database_privilege('termterm_team_service',current_database(),'CONNECT') AND NOT has_database_privilege('termterm_team_service',current_database(),'CREATE') AND NOT pg_has_role('termterm_team_service','termterm_team_migrator','MEMBER') AND NOT has_schema_privilege('termterm_team_service','public','CREATE')")=='t'
    assert sql("SELECT to_regnamespace('termterm_team_v2') IS NULL")=='t'
    # Rerun must stop before touching passwords or an existing database.
    repeated=subprocess.run(['docker','exec',name,'psql','-X','-U','postgres','-d','postgres','-f','/tmp/prepare.sql'],capture_output=True)
    assert repeated.returncode!=0
    print('PASS: interactive hidden passwords, SCRAM roles, ownership, pgcrypto, least privilege and safe rerun refusal.')
finally:
    subprocess.run(['docker','rm','-f',name],capture_output=True)
