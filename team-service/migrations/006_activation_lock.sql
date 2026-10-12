BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE OR REPLACE FUNCTION session_actor(token text, activation boolean DEFAULT false) RETURNS auth_sessions
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 -- Activation/password change needs an exclusive account lock from the start;
 -- two concurrent activations must not both upgrade a SHARE lock.
 IF activation THEN
  PERFORM a.id FROM team_accounts a JOIN auth_sessions x ON (a.team_id,a.id)=(x.team_id,x.account_id)
  WHERE x.access_hash=public.digest(token,'sha256') FOR UPDATE OF a;
 END IF;
 SELECT x.* INTO s FROM auth_sessions x JOIN team_accounts a ON (a.team_id,a.id)=(x.team_id,x.account_id)
 WHERE x.access_hash=public.digest(token,'sha256') AND x.revoked_at IS NULL AND x.rotated_at IS NULL
 AND x.access_expires>clock_timestamp() AND x.auth_epoch=a.auth_epoch
 AND ((a.status='active' AND NOT x.activation_only) OR (activation AND a.status='pending' AND x.activation_only))
 FOR SHARE OF a;
 IF s.id IS NULL THEN RAISE EXCEPTION 'invalid_session' USING ERRCODE='28000'; END IF;
 RETURN s;
END $$;
CREATE OR REPLACE FUNCTION api_recovery_rewrap(token text,operation uuid,credential uuid,rev bigint,identity_ver bigint,data_key bytea,secret_key bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; op recovery_operations;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 SELECT * INTO op FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND actor_id=s.account_id AND status='authorized' AND expires_at>clock_timestamp();
 IF op.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 -- Same order as user confirmation: subject account, then recovery operation.
 PERFORM 1 FROM team_accounts WHERE team_id=s.team_id AND id=op.subject_id AND status='active' FOR SHARE;
 IF NOT FOUND OR NOT EXISTS(SELECT FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=op.subject_id AND version=identity_ver AND active) THEN
  RAISE EXCEPTION 'crypto_context_changed' USING ERRCODE='40001'; END IF;
 SELECT * INTO op FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND actor_id=s.account_id AND status='authorized' AND expires_at>clock_timestamp() FOR UPDATE;
 IF op.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=credential AND owner_account_id=op.subject_id AND visibility='private' AND NOT deleted) THEN
  RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 INSERT INTO credential_key_envelopes(team_id,credential_id,revision,account_id,identity_version,purpose,envelope) VALUES
 (s.team_id,credential,rev,op.subject_id,identity_ver,'data',data_key),(s.team_id,credential,rev,op.subject_id,identity_ver,'secret',secret_key)
 ON CONFLICT(team_id,credential_id,revision,account_id,identity_version,purpose) WHERE account_id IS NOT NULL DO UPDATE SET envelope=excluded.envelope;
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'recovery.rewrap',op.subject_id,'succeeded');
 RETURN '{"rewrapped":true,"verificationRequired":true}';
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(6,now());
COMMIT;
