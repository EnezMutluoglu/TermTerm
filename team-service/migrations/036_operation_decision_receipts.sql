BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE OR REPLACE FUNCTION api_operation_receipt(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;op change_operations;
BEGIN
 s:=session_actor(token);
 IF operation IS NULL THEN RAISE EXCEPTION 'invalid_operation' USING ERRCODE='22023'; END IF;
 SELECT * INTO op FROM change_operations WHERE team_id=s.team_id AND id=operation AND account_id=s.account_id AND device_id=s.device_id;
 IF op.id IS NULL THEN RETURN jsonb_build_object('found',false); END IF;
 RETURN jsonb_build_object('found',true,'operationId',op.id,'status',op.status,'revision',op.result_revision,
  'conflictId',(SELECT id FROM conflicts WHERE team_id=s.team_id AND operation_id=op.id),
  'decisionStatus',(SELECT status FROM conflicts WHERE team_id=s.team_id AND operation_id=op.id));
END $$;
REVOKE ALL ON FUNCTION api_operation_receipt(text,uuid) FROM PUBLIC;
INSERT INTO schema_migrations VALUES(36,now());
COMMIT;
