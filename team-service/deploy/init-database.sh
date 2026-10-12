#!/bin/bash
set -euo pipefail
owner=$(cat /run/secrets/migration_password)
service=$(cat /run/secrets/service_password)
[[ "$owner" =~ ^[a-f0-9]{64}$ && "$service" =~ ^[a-f0-9]{64}$ ]] || exit 1
# No secrets in process arguments, psql echo or container output.
psql -X -q -v ON_ERROR_STOP=1 --username postgres --dbname termterm_team <<SQL
CREATE ROLE termterm_team_migrator LOGIN PASSWORD '$owner' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT;
CREATE ROLE termterm_team_service LOGIN PASSWORD '$service' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT;
ALTER DATABASE termterm_team OWNER TO termterm_team_migrator;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
REVOKE ALL ON DATABASE termterm_team FROM PUBLIC;
GRANT CONNECT ON DATABASE termterm_team TO termterm_team_migrator,termterm_team_service;
SQL
