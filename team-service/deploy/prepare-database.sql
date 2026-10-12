-- FIRST INSTALL ONLY. Run interactively as DBA with psql -X -f this-file.
-- Fixed names match service.example.json. Do not use on an existing Team DB.
-- Password prompts use psql \password; no plaintext passwords in SQL/history.
\set ON_ERROR_STOP on
SET password_encryption = 'scram-sha-256';
BEGIN;
CREATE ROLE termterm_team_migrator LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT;
CREATE ROLE termterm_team_service LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT;
COMMIT;
\password termterm_team_migrator
\password termterm_team_service
CREATE DATABASE termterm_team OWNER termterm_team_migrator TEMPLATE template0 ENCODING 'UTF8';
REVOKE ALL ON DATABASE termterm_team FROM PUBLIC;
GRANT CONNECT ON DATABASE termterm_team TO termterm_team_migrator, termterm_team_service;
\connect termterm_team
CREATE EXTENSION IF NOT EXISTS pgcrypto;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
-- Next: manage.py migrate using the migrator, never the service account.
