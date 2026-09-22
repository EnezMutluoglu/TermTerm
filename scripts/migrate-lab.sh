#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
for db in termterm_dev termterm_e2e; do
  # Objects belong to the migration role; application logins cannot bypass RLS.
  { echo 'SET ROLE termterm_migrator;'; cat "$root/migrations/001_sync.sql"; } | runuser -u postgres -- psql -X -p 55432 -d "$db" -v ON_ERROR_STOP=1 >/dev/null
  { echo 'SET ROLE termterm_migrator;'; cat "$root/migrations/002_shared_terminal.sql"; } | runuser -u postgres -- psql -X -p 55432 -d "$db" -v ON_ERROR_STOP=1 >/dev/null
  { echo 'SET ROLE termterm_migrator;'; cat "$root/migrations/003_schema_version.sql"; } | runuser -u postgres -- psql -X -p 55432 -d "$db" -v ON_ERROR_STOP=1 >/dev/null
  for role in termterm_app termterm_editor termterm_viewer; do
    runuser -u postgres -- psql -X -p 55432 -d "$db" -v ON_ERROR_STOP=1 -v app_role="$role" -f "$root/migrations/grant_app.sql" >/dev/null
  done
done
echo 'TermTerm lab migrations applied.'
