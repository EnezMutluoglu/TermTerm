BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Selection belongs to the account. Other accounts always retain their own view.
CREATE FUNCTION api_history_select(token text,r uuid,selected_revision bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF selected_revision IS NOT NULL THEN
  IF NOT allowed(s.team_id,s.account_id,r,'read') OR NOT allowed(s.team_id,s.account_id,r,'connect')
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  IF NOT EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=r) OR NOT EXISTS(SELECT FROM resource_versions
  WHERE team_id=s.team_id AND resource_id=r AND revision=selected_revision)
  THEN RAISE EXCEPTION 'history_unavailable' USING ERRCODE='22023'; END IF;
  IF EXISTS(SELECT FROM host_hops WHERE team_id=s.team_id AND host_id=r)
  THEN RAISE EXCEPTION 'chain_requires_dependency_bundle' USING ERRCODE='22023'; END IF;
 END IF;
 -- Returning to current may clear this account's pin even after access is revoked.
 DELETE FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='temporary';
 IF selected_revision IS NOT NULL THEN
  INSERT INTO revision_pins VALUES(s.team_id,r,selected_revision,s.account_id,'temporary');
 END IF;
 RETURN jsonb_build_object('resourceId',r,'revision',selected_revision);
END $$;
CREATE FUNCTION api_history_selections(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('resourceId',resource_id,'revision',revision)) FROM revision_pins
 WHERE team_id=s.team_id AND account_id=s.account_id AND purpose='temporary'
 AND allowed(s.team_id,s.account_id,resource_id,'read') AND allowed(s.team_id,s.account_id,resource_id,'connect')),'[]');
END $$;
-- Preserve the live host authorization check; history never grants connection rights.
ALTER FUNCTION api_connection_target(text,uuid) RENAME TO connection_target_current;
CREATE FUNCTION api_connection_target(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE result jsonb;s auth_sessions;selected_revision bigint;
BEGIN
 result:=connection_target_current(token,r);s:=session_actor(token);
 SELECT revision INTO selected_revision FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='temporary';
 IF selected_revision IS NOT NULL THEN
  IF NOT allowed(s.team_id,s.account_id,r,'read') THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  result:=result || jsonb_build_object('temporaryRevision',selected_revision);
 END IF;
 RETURN result;
END $$;
-- Renaming a function retains grants. Explicitly remove grants from the internal function.
DO $$ DECLARE role_name text; BEGIN
 FOR role_name IN SELECT grantee::regrole::text FROM pg_proc p,aclexplode(coalesce(p.proacl,acldefault('f',p.proowner))) a
 WHERE p.oid='connection_target_current(text,uuid)'::regprocedure AND a.grantee<>p.proowner AND a.grantee<>0
 LOOP EXECUTE format('REVOKE ALL ON FUNCTION connection_target_current(text,uuid) FROM %s',role_name); END LOOP;
END $$;
REVOKE ALL ON FUNCTION connection_target_current(text,uuid),api_connection_target(text,uuid),api_history_select(text,uuid,bigint),api_history_selections(text) FROM PUBLIC;
INSERT INTO schema_migrations VALUES(19,now());
COMMIT;
