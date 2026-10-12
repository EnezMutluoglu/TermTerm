BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE OR REPLACE FUNCTION api_history_select(token text,r uuid,selected_revision bigint) RETURNS jsonb
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
 -- A user may release their own selection even after access is revoked.
 DELETE FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='temporary';
 IF selected_revision IS NOT NULL THEN
  INSERT INTO revision_pins VALUES(s.team_id,r,selected_revision,s.account_id,'temporary');
 END IF;
 -- Releasing/switching a selection must not retain old versions until the next edit.
 -- Other users' temporary selections and restore return points remain protected.
 DELETE FROM resource_versions rv USING resources x WHERE (x.team_id,x.id)=(rv.team_id,rv.resource_id)
 AND rv.team_id=s.team_id AND rv.resource_id=r AND rv.revision<x.revision-10
 AND NOT EXISTS(SELECT FROM revision_pins p WHERE (p.team_id,p.resource_id,p.revision)=(rv.team_id,rv.resource_id,rv.revision));
 RETURN jsonb_build_object('resourceId',r,'revision',selected_revision);
END $$;
INSERT INTO schema_migrations(version) VALUES(29);
COMMIT;
