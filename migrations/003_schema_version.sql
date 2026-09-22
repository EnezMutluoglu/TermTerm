BEGIN;
INSERT INTO termterm.schema_version(version) VALUES(2) ON CONFLICT DO NOTHING;
COMMIT;
