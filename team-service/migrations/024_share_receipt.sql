BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE OR REPLACE FUNCTION api_share_plan(token text,plan uuid,cancel boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p credential_share_plans;
BEGIN
 s:=session_actor(token);
 SELECT * INTO p FROM credential_share_plans WHERE team_id=s.team_id AND id=plan AND actor_id=s.account_id FOR UPDATE;
 IF p.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 IF cancel THEN
  IF p.status='applied' THEN RAISE EXCEPTION 'already_applied' USING ERRCODE='22023'; END IF;
  UPDATE credential_share_plans SET status='cancelled' WHERE team_id=s.team_id AND id=plan;
  RETURN '{"status":"cancelled"}';
 END IF;
 IF p.status='applied' THEN RETURN jsonb_build_object('status','applied','credentialId',p.credential_id,'revision',1); END IF;
 IF p.status<>'pending' OR p.expires_at<=now() OR NOT allowed(s.team_id,s.account_id,p.scope_id,'edit') THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='42501'; END IF;
 RETURN share_plan_json(p);
END $$;
INSERT INTO schema_migrations(version) VALUES(24);
COMMIT;
