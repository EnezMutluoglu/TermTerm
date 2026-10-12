BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- The authorization is evaluated on the server for every new transport. A
-- read grant or a client supplied username/record is never connection authority.
CREATE FUNCTION api_connection_target(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 IF NOT allowed(s.team_id,s.account_id,r,'connect') OR NOT EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=r)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 -- Until the dependency-key plan is installed, fail closed rather than connect
 -- directly and silently omit a configured jump host.
 IF EXISTS(SELECT FROM host_hops WHERE team_id=s.team_id AND host_id=r)
 THEN RAISE EXCEPTION 'chain_requires_dependency_bundle' USING ERRCODE='22023'; END IF;
 RETURN shared_resource_json(s.team_id,s.account_id,r);
END $$;
REVOKE ALL ON FUNCTION api_connection_target(text,uuid) FROM PUBLIC;
INSERT INTO schema_migrations VALUES(16,now());
COMMIT;
