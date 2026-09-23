#!/usr/bin/env bash
# Separate Team lab. Does not alter personal databases, cluster settings or installed apps.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
database=${1:-termterm_team_dev}
case "$database" in termterm_team_dev|termterm_team_e2e) ;; *) echo 'Only isolated Team lab databases are allowed'; exit 2;; esac
port=55432
mkdir -p "$root/.lab"
python3 - "$root/.lab/team-connection.json" <<'PY'
import json,os,secrets,sys,subprocess
p=sys.argv[1]
if not os.path.exists(p):
    with open(p,'x') as f: json.dump(dict(label='Team Lab',host='localhost',port=55432,database='termterm_team_dev',username='termterm_team_app',password=secrets.token_hex(32),schema='termterm_team',caPath=os.path.join(os.path.dirname(p),'ca.crt'),tls='verify-full'),f)
    os.chmod(p,0o600)
password=json.load(open(p))['password']
assert all(c in '0123456789abcdef' for c in password)
sql="DO $$ BEGIN IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='termterm_team_app') THEN CREATE ROLE termterm_team_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS PASSWORD '%s'; END IF; END $$;"%password
subprocess.run(['runuser','-u','postgres','--','psql','-X','-v','ON_ERROR_STOP=1','-p','55432','-d','postgres'],input=sql,text=True,check=True,stdout=subprocess.DEVNULL)
PY
if ! runuser -u postgres -- psql -X -p "$port" -d postgres -Atc "SELECT 1 FROM pg_database WHERE datname='$database'" | grep -q 1; then
 runuser -u postgres -- createdb -p "$port" "$database"
fi
ready=$(runuser -u postgres -- psql -X -p "$port" -d "$database" -Atc "SELECT to_regclass('termterm_team.schema_version') IS NOT NULL")
if [ "$ready" = f ]; then
 runuser -u postgres -- psql -X -p "$port" -d "$database" -v ON_ERROR_STOP=1 -f "$root/migrations/team/001_team.sql" >/dev/null
fi
# Development RPC/policy migrations are repeatable; never drop the lab's records.
for migration in "$root"/migrations/team/0*.sql; do
 [[ "$migration" == */001_team.sql ]] && continue
 runuser -u postgres -- psql -X -p "$port" -d "$database" -v ON_ERROR_STOP=1 -f "$migration" >/dev/null
done
runuser -u postgres -- psql -X -p "$port" -d "$database" -v ON_ERROR_STOP=1 -v app_role=termterm_team_app -f "$root/migrations/team/grant_app.sql" >/dev/null
echo "Team lab ready: $database on localhost:$port (TLS; credentials in ignored .lab)."
