BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE TABLE resource_restorations (
 team_id uuid NOT NULL, operation_id uuid NOT NULL, source_revision bigint NOT NULL,
 return_revision bigint NOT NULL, PRIMARY KEY(team_id,operation_id),
 FOREIGN KEY(team_id,operation_id) REFERENCES change_operations(team_id,id)
);
ALTER TABLE resource_restorations ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON resource_restorations FROM PUBLIC;
CREATE OR REPLACE FUNCTION api_resource_restore(token text, source_revision bigint, body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;r uuid:=(body->>'resourceId')::uuid; old resources;result jsonb;op change_operations;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT allowed(s.team_id,s.account_id,r,'edit') OR NOT allowed(s.team_id,s.account_id,r,'read')
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT * INTO op FROM change_operations WHERE team_id=s.team_id AND id=(body->>'operationId')::uuid;
 IF op.id IS NOT NULL THEN
  IF NOT EXISTS(SELECT FROM resource_restorations rr WHERE rr.team_id=s.team_id AND rr.operation_id=op.id AND rr.source_revision=api_resource_restore.source_revision)
  THEN RAISE EXCEPTION 'operation_reuse' USING ERRCODE='22023'; END IF;
  -- The regular save validates actor/device/request hash before replaying.
  RETURN api_resource_save(token,body) || (SELECT jsonb_build_object('returnRevision',rr.return_revision,'sourceRevision',rr.source_revision)
  FROM resource_restorations rr WHERE rr.team_id=s.team_id AND rr.operation_id=op.id);
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
 INSERT INTO resource_restorations VALUES(s.team_id,(body->>'operationId')::uuid,source_revision,old.revision);
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome)
 VALUES(s.team_id,s.account_id,'resource.restore',r,'succeeded');
 RETURN result || jsonb_build_object('returnRevision',old.revision,'sourceRevision',source_revision);
END $$;
INSERT INTO schema_migrations VALUES(18,now());
COMMIT;
