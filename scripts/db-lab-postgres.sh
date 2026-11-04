#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source .lab/database/credentials.env
pg_ctlcluster 18 termterm start || test "$(pg_lsclusters -h | awk '$2=="termterm" {print $4}')" = online
if ! runuser -u postgres -- psql -p 55432 -Atqc "SELECT 1 FROM pg_roles WHERE rolname='termterm_sql_lab'" | grep -q 1; then
  printf "CREATE ROLE termterm_sql_lab LOGIN PASSWORD '%s';\n" "$PG_PASSWORD" | runuser -u postgres -- psql -p 55432 -v ON_ERROR_STOP=1 >/dev/null
  runuser -u postgres -- createdb -p 55432 -O termterm_sql_lab termterm_sql_lab
fi
echo 'PostgreSQL disposable SQL database ready'
