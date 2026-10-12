BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION api_chain_configuration(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 IF NOT EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=r) THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 RETURN jsonb_build_object('targetId',r,'revision',coalesce((SELECT revision FROM connection_configurations WHERE team_id=s.team_id AND host_id=r),0),
 'hops',coalesce((SELECT jsonb_agg(jump_host_id ORDER BY position) FROM host_hops WHERE team_id=s.team_id AND host_id=r),'[]'));
END $$;
REVOKE ALL ON FUNCTION api_chain_configuration(text,uuid) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(38);
COMMIT;
