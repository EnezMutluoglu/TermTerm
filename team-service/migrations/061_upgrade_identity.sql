BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Installation identity is control metadata; no account or credential content.
CREATE TABLE service_installation (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
 id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
 created_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO service_installation(singleton) VALUES(true);
ALTER TABLE service_installation ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON service_installation FROM PUBLIC;
CREATE OR REPLACE FUNCTION api_version() RETURNS jsonb LANGUAGE sql SECURITY DEFINER
SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
 SELECT jsonb_build_object('apiVersion',1,'schemaVersion',(SELECT max(version) FROM schema_migrations),
 'installationId',(SELECT id FROM service_installation WHERE singleton),
 'databaseScope',encode(public.digest(current_database()||':'||(SELECT oid::text FROM pg_database WHERE datname=current_database()),'sha256'),'hex'))
$$;
REVOKE ALL ON FUNCTION api_version() FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(61);
COMMIT;
