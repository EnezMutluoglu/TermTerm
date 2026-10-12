BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE OR REPLACE FUNCTION session_actor(token text, activation boolean DEFAULT false) RETURNS auth_sessions
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 SELECT x.* INTO s FROM auth_sessions x JOIN team_accounts a ON (a.team_id,a.id)=(x.team_id,x.account_id)
 WHERE x.access_hash=public.digest(token,'sha256') AND x.revoked_at IS NULL AND x.rotated_at IS NULL
 AND x.access_expires>clock_timestamp() AND x.auth_epoch=a.auth_epoch
 AND ((a.status='active' AND NOT x.activation_only) OR (activation AND a.status='pending' AND x.activation_only))
 FOR SHARE OF a;
 IF s.id IS NULL THEN RAISE EXCEPTION 'invalid_session' USING ERRCODE='28000'; END IF;
 RETURN s;
END $$;
CREATE OR REPLACE FUNCTION api_refresh(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; a team_accounts;
BEGIN
 SELECT * INTO s FROM auth_sessions WHERE refresh_hash=public.digest(token,'sha256');
 IF s.id IS NULL THEN RETURN '{"error":"invalid_session"}'; END IF;
 -- All mutations lock account before sessions: password reset/disable cannot
 -- deadlock against refresh, and a revoked account cannot gain a new session.
 SELECT * INTO a FROM team_accounts WHERE team_id=s.team_id AND id=s.account_id FOR UPDATE;
 SELECT * INTO s FROM auth_sessions WHERE refresh_hash=public.digest(token,'sha256') FOR UPDATE;
 IF s.rotated_at IS NOT NULL THEN
  UPDATE auth_sessions SET revoked_at=coalesce(revoked_at,clock_timestamp()) WHERE family_id=s.family_id;
  RETURN '{"error":"refresh_replayed"}';
 END IF;
 IF s.revoked_at IS NOT NULL OR s.refresh_expires<=clock_timestamp() OR s.auth_epoch<>a.auth_epoch OR a.status<>'active' OR s.activation_only
 THEN RETURN '{"error":"invalid_session"}'; END IF;
 UPDATE auth_sessions SET rotated_at=clock_timestamp() WHERE id=s.id;
 RETURN issue_session(s.team_id,s.account_id,s.device_id,false,s.family_id);
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(5,now());
COMMIT;
