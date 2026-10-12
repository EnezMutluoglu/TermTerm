BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Recover a lost final response even after a desktop restart. Reopening the
-- same two files proves which completed operation is being inspected; it never
-- reactivates an old key or starts an inverse rotation.
ALTER FUNCTION api_rotation_begin(text,text,bytea,bytea,text) RENAME TO begin_unfinished_rotation;
CREATE FUNCTION api_rotation_begin(token text,password text,old_public bytea,new_public bytea,reason_input text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;r recovery_rotations;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 SELECT x.* INTO r FROM recovery_rotations x JOIN team_recovery_keys o ON (o.team_id,o.id)=(x.team_id,x.old_key_id)
 JOIN team_recovery_keys n ON (n.team_id,n.id)=(x.team_id,x.new_key_id)
 WHERE x.team_id=s.team_id AND x.actor_id=s.account_id AND x.status='completed' AND o.public_key=old_public AND n.public_key=new_public AND n.active;
 IF r.id IS NULL THEN RETURN begin_unfinished_rotation(token,password,old_public,new_public,reason_input); END IF;
 IF NOT reauthenticate(s,password) THEN RETURN '{"error":"invalid_credentials"}'; END IF;
 UPDATE recovery_rotations SET authorized_until=clock_timestamp()+interval '30 minutes' WHERE team_id=s.team_id AND id=r.id;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'recovery.rotation_receipt','succeeded');
 RETURN jsonb_build_object('operationId',r.id,'oldKeyId',r.old_key_id,'newKeyId',r.new_key_id,'status','completed');
END $$;
REVOKE ALL ON FUNCTION begin_unfinished_rotation(text,text,bytea,bytea,text),api_rotation_begin(text,text,bytea,bytea,text) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(47);
COMMIT;
