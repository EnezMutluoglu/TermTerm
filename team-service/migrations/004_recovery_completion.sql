BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION api_recovery_rewrap(token text,operation uuid,credential uuid,rev bigint,identity_ver bigint,data_key bytea,secret_key bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; op recovery_operations;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 SELECT * INTO op FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND actor_id=s.account_id AND status='authorized' AND expires_at>clock_timestamp() FOR UPDATE;
 IF op.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 PERFORM 1 FROM team_accounts WHERE team_id=s.team_id AND id=op.subject_id AND status='active' FOR SHARE;
 IF NOT FOUND OR NOT EXISTS(SELECT FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=op.subject_id AND version=identity_ver AND active) THEN
  RAISE EXCEPTION 'crypto_context_changed' USING ERRCODE='40001'; END IF;
 IF NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=credential AND owner_account_id=op.subject_id AND visibility='private' AND NOT deleted) THEN
  RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 INSERT INTO credential_key_envelopes(team_id,credential_id,revision,account_id,identity_version,purpose,envelope) VALUES
 (s.team_id,credential,rev,op.subject_id,identity_ver,'data',data_key),(s.team_id,credential,rev,op.subject_id,identity_ver,'secret',secret_key)
 ON CONFLICT(team_id,credential_id,revision,account_id,identity_version,purpose) WHERE account_id IS NOT NULL DO UPDATE SET envelope=excluded.envelope;
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'recovery.rewrap',op.subject_id,'succeeded');
 RETURN '{"rewrapped":true,"verificationRequired":true}';
END $$;
CREATE FUNCTION api_recovery_complete(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; op recovery_operations;
BEGIN
 -- The recovered USER confirms successful decryption on their desktop. A leader
 -- cannot declare ciphertext recovered just because envelopes were uploaded.
 s:=session_actor(token);
 SELECT * INTO op FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND subject_id=s.account_id AND status='authorized' FOR UPDATE;
 IF op.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF EXISTS(SELECT FROM credentials c JOIN credential_versions v ON (v.team_id,v.credential_id)=(c.team_id,c.id)
 WHERE c.team_id=s.team_id AND c.owner_account_id=s.account_id AND NOT c.deleted AND
 (SELECT count(DISTINCT e.purpose) FROM credential_key_envelopes e JOIN account_crypto_identities i ON
 (i.team_id,i.account_id,i.version)=(e.team_id,e.account_id,e.identity_version)
 WHERE (e.team_id,e.credential_id,e.revision,e.account_id)=(v.team_id,v.credential_id,v.revision,s.account_id) AND i.active)<>2)
 THEN RAISE EXCEPTION 'recovery_incomplete' USING ERRCODE='23514'; END IF;
 UPDATE recovery_operations SET status='completed',completed_at=clock_timestamp() WHERE team_id=s.team_id AND id=operation;
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'recovery.confirm',s.account_id,'succeeded');
 RETURN '{"completed":true}';
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(4,now());
COMMIT;
