BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Audit uses an immutable sequence cursor; time index remains for retention.
CREATE INDEX audit_team_cursor ON audit_events(team_id,id DESC);
INSERT INTO schema_migrations(version) VALUES(25);
COMMIT;
