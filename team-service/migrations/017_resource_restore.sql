BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION api_resource_restore(token text, source_revision bigint, body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;r uuid:=(body->>'resourceId')::uuid; old resources;result jsonb;op change_operations;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT allowed(s.team_id,s.account_id,r,'edit') OR NOT allowed(s.team_id,s.account_id,r,'read')
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT * INTO op FROM change_operations WHERE team_id=s.team_id AND id=(body->>'operationId')::uuid;
 IF op.id IS NOT NULL THEN
  RETURN api_resource_save(token,body);
 END IF;
 SELECT * INTO old FROM resources WHERE team_id=s.team_id AND id=r FOR UPDATE;
 IF old.revision<>(body->>'expectedRevision')::bigint THEN RAISE EXCEPTION 'revision_changed' USING ERRCODE='40001'; END IF;
 IF source_revision IS NULL OR source_revision=old.revision OR NOT EXISTS(SELECT FROM resource_versions
 WHERE team_id=s.team_id AND resource_id=r AND revision=source_revision)
 THEN RAISE EXCEPTION 'history_unavailable' USING ERRCODE='22023'; END IF;
 -- Keep the previous current version before api_resource_save prunes history.
 DELETE FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='return_point';
 INSERT INTO revision_pins VALUES(s.team_id,r,old.revision,s.account_id,'return_point');
 result:=api_resource_save(token,body);
 IF result->>'status'<>'applied' THEN RAISE EXCEPTION 'restore_not_applied' USING ERRCODE='40001'; END IF;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome)
 VALUES(s.team_id,s.account_id,'resource.restore',r,'succeeded');
 RETURN result || jsonb_build_object('returnRevision',old.revision,'sourceRevision',source_revision);
END $$;
CREATE FUNCTION api_history_pins(token text,r uuid,release_return boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT allowed(s.team_id,s.account_id,r,'read') THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF release_return THEN
  DELETE FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='return_point';
  DELETE FROM resource_versions rv USING resources x WHERE (x.team_id,x.id)=(rv.team_id,rv.resource_id)
  AND rv.team_id=s.team_id AND rv.resource_id=r AND rv.revision<x.revision-10
  AND NOT EXISTS(SELECT FROM revision_pins p WHERE (p.team_id,p.resource_id,p.revision)=(rv.team_id,rv.resource_id,rv.revision));
 END IF;
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('revision',revision,'purpose',purpose))
 FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id),'[]');
END $$;
REVOKE ALL ON FUNCTION api_resource_restore(text,bigint,jsonb),api_history_pins(text,uuid,boolean) FROM PUBLIC;
INSERT INTO schema_migrations VALUES(17,now());
COMMIT;
