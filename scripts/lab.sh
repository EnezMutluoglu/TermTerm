#!/usr/bin/env bash
set -euo pipefail
action=${1:-status}
out=${2:-/tmp/termterm-lab}
cluster=termterm
port=55432
certdir=/etc/postgresql/18/termterm/tls
case "$action" in
setup)
  command -v pg_createcluster >/dev/null
  if ! pg_lsclusters --no-header | awk '$1==18 && $2=="termterm" {found=1} END {exit !found}'; then
    pg_createcluster 18 "$cluster" --port "$port" -- --auth-local=peer --auth-host=scram-sha-256
  fi
  actual=$(pg_lsclusters --no-header | awk '$1==18 && $2=="termterm" {print $3}')
  test "$actual" = "$port" || { echo 'Existing termterm cluster uses a different port; refusing changes.'; exit 1; }
  mkdir -p "$certdir" "$out"
  chmod 700 "$certdir"
  if [ ! -f "$certdir/ca.crt" ]; then
    openssl req -x509 -newkey rsa:3072 -nodes -days 3650 -subj '/CN=TermTerm Development CA' -keyout "$certdir/ca.key" -out "$certdir/ca.crt" 2>/dev/null
    openssl req -newkey rsa:3072 -nodes -subj '/CN=localhost' -keyout "$certdir/server.key" -out "$certdir/server.csr" 2>/dev/null
    printf 'subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1\nextendedKeyUsage=serverAuth\n' > "$certdir/server.ext"
    openssl x509 -req -in "$certdir/server.csr" -CA "$certdir/ca.crt" -CAkey "$certdir/ca.key" -CAcreateserial -days 825 -extfile "$certdir/server.ext" -out "$certdir/server.crt" 2>/dev/null
    chmod 600 "$certdir/"*.key
  fi
  chown -R postgres:postgres "$certdir"
  pg_conftool 18 "$cluster" set listen_addresses localhost
  pg_conftool 18 "$cluster" set ssl on
  pg_conftool 18 "$cluster" set ssl_cert_file "$certdir/server.crt"
  pg_conftool 18 "$cluster" set ssl_key_file "$certdir/server.key"
  pg_conftool 18 "$cluster" set password_encryption scram-sha-256
  cat > /etc/postgresql/18/termterm/pg_hba.conf <<'HBA'
local all postgres peer
local all all peer
hostssl all all 127.0.0.1/32 scram-sha-256
hostssl all all ::1/128 scram-sha-256
HBA
  pg_ctlcluster 18 "$cluster" restart
  secrets=/var/lib/postgresql/termterm-lab-secrets.json
  if [ ! -f "$secrets" ]; then
    python3 - "$secrets" <<'PY'
import json,secrets,sys,os
with open(sys.argv[1], 'x') as f:
    json.dump({k:secrets.token_urlsafe(32) for k in ['termterm_migrator','termterm_app','termterm_editor','termterm_viewer']},f)
os.chmod(sys.argv[1],0o600)
PY
  fi
  for role in termterm_migrator termterm_app termterm_editor termterm_viewer; do
    password=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$secrets" "$role")
    # psql variables are quoted as SQL literals; no credentials printed.
    runuser -u postgres -- psql -X -p "$port" -v ON_ERROR_STOP=1 -v role="$role" -v pass="$password" >/dev/null <<'SQL'
SELECT format('CREATE ROLE %I LOGIN', :'role') WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname=:'role') \gexec
SELECT format('ALTER ROLE %I PASSWORD %L', :'role', :'pass') \gexec
SQL
  done
  for db in termterm_dev termterm_e2e; do
    runuser -u postgres -- psql -X -p "$port" -v ON_ERROR_STOP=1 -v db="$db" >/dev/null <<'SQL'
SELECT format('CREATE DATABASE %I OWNER termterm_migrator', :'db') WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname=:'db') \gexec
SQL
  done
  cp "$certdir/ca.crt" "$out/ca.crt"
  python3 - "$secrets" "$out/connection.json" <<'PY'
import json,sys,os
s=json.load(open(sys.argv[1])); json.dump({'host':'localhost','port':55432,'database':'termterm_dev','username':'termterm_app','password':s['termterm_app'],'schema':'termterm','tls':'verify-full','accounts':s},open(sys.argv[2],'w'),indent=2)
os.chmod(sys.argv[2],0o600)
PY
  echo 'TermTerm PostgreSQL 18 lab ready on localhost:55432 (TLS required).'
  ;;
start)
  if pg_ctlcluster 18 "$cluster" status >/dev/null 2>&1; then
    echo 'TermTerm PostgreSQL lab is already running.'
  else
    pg_ctlcluster 18 "$cluster" start
  fi
  ;;
stop)
  if pg_ctlcluster 18 "$cluster" status >/dev/null 2>&1; then
    pg_ctlcluster 18 "$cluster" stop
  else
    echo 'TermTerm PostgreSQL lab is already stopped.'
  fi
  ;;
status) pg_lsclusters ;;
backup)
  mkdir -p "$out"
  runuser -u postgres -- pg_dump -p "$port" -Fc termterm_dev > "$out/termterm_dev-$(date +%Y%m%d-%H%M%S).dump"
  echo 'Database backup complete.'
  ;;
*) echo 'Usage: lab.sh setup|start|stop|status|backup [output-dir]'; exit 2 ;;
esac
