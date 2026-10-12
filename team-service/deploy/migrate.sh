#!/usr/bin/env bash
# Explicit migration tool. PGHOST/PGPORT/PGDATABASE/PGUSER/PGSSLROOTCERT are supplied
# by the operator; use a mode-0600 PGPASSFILE, never a password command argument.
set -euo pipefail
export PGSSLMODE=verify-full
: "${PGHOST:?Set PGHOST}" "${PGDATABASE:?Set PGDATABASE}" "${PGUSER:?Set dedicated migration PGUSER}"
: "${PGSSLROOTCERT:?Set PGSSLROOTCERT}" "${PGPASSFILE:?Set PGPASSFILE}"
: "${TERMTERM_SERVICE_ROLE:?Set limited TERMTERM_SERVICE_ROLE}"
exec python3 "$(dirname "$0")/manage.py" migrate --service-role "$TERMTERM_SERVICE_ROLE"
