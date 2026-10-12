-- FIRST INSTALL ONLY; alternative to prepare-database.sql, never run both.
-- Run as DBA: psql -X -d postgres -f prepare-database-owner-template.sql
-- For socket-only HBA authentication add -v socket_auth=true (no passwords).
-- Requires PostgreSQL 18 and the server locale en_US.UTF-8.
\set ON_ERROR_STOP on
SET password_encryption = 'scram-sha-256';
BEGIN;
CREATE ROLE termterm_team NOLOGIN NOINHERIT;
CREATE ROLE termterm_team_owner LOGIN INHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS IN ROLE termterm_team;
CREATE ROLE termterm_team_app NOLOGIN INHERIT IN ROLE termterm_team;
CREATE ROLE termterm_team_developer NOLOGIN INHERIT IN ROLE termterm_team;
CREATE ROLE termterm_team_readonly NOLOGIN INHERIT IN ROLE termterm_team;
CREATE ROLE termterm_team_appuser LOGIN INHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS IN ROLE termterm_team_app;
COMMIT;
\if :{?socket_auth}
\if :socket_auth
\echo 'Socket authentication selected; LOGIN roles have no password.'
\else
\password termterm_team_owner
\password termterm_team_appuser
\endif
\else
\password termterm_team_owner
\password termterm_team_appuser
\endif

CREATE DATABASE termterm_team OWNER termterm_team_owner TEMPLATE template0
    ENCODING 'UTF8' LOCALE_PROVIDER libc LC_COLLATE 'en_US.UTF-8' LC_CTYPE 'en_US.UTF-8';
REVOKE ALL ON DATABASE termterm_team FROM PUBLIC;
GRANT CONNECT, TEMP ON DATABASE termterm_team TO termterm_team;
\connect termterm_team
CREATE EXTENSION IF NOT EXISTS pgcrypto;
ALTER SCHEMA public OWNER TO termterm_team_owner;
REVOKE CREATE, USAGE ON SCHEMA public FROM PUBLIC;
GRANT USAGE ON SCHEMA public TO termterm_team;

-- Team data lives in termterm_team_v2, owned by owner after migration.
-- No default TABLE/SEQUENCE grants to app/developer/readonly: they would
-- bypass Team's API permissions and make the limited service role invalid.
-- Next, connect as owner and run:
-- python3 deploy/manage.py migrate --service-role termterm_team_appuser
-- This grants only the explicitly allowed Team API functions to appuser.
