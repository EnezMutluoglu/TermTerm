BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION api_recovery_manifest(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;op recovery_operations;
BEGIN
 s:=session_actor(token);
 SELECT * INTO op FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND (status='authorized' OR (status='completed' AND subject_id=s.account_id));
 IF op.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 IF s.account_id<>op.subject_id THEN
  PERFORM require_owner(s);
  IF op.actor_id<>s.account_id OR op.expires_at<=clock_timestamp() THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 END IF;
 IF (SELECT count(*) FROM credentials WHERE team_id=s.team_id AND owner_account_id=op.subject_id AND NOT deleted)>10000
 THEN RAISE EXCEPTION 'recovery_batch_too_large' USING ERRCODE='22023'; END IF;
 RETURN (SELECT jsonb_build_object('operationId',op.id,'subjectId',op.subject_id,'displayName',a.display_name,
 'identityVersion',i.version,'publicKey',encode(i.public_key,'hex'),'recoveryKeyId',op.recovery_key_id,
 'credentials',coalesce((SELECT jsonb_agg(id ORDER BY id) FROM credentials WHERE team_id=s.team_id AND owner_account_id=op.subject_id AND visibility='private' AND NOT deleted),'[]'))
 FROM account_crypto_identities i JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id)
 WHERE i.team_id=s.team_id AND i.account_id=op.subject_id AND i.active AND a.status='active');
END $$;
CREATE FUNCTION api_recovery_pending(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',o.id,'createdAt',o.created_at,'actor',a.display_name) ORDER BY o.created_at DESC)
 FROM recovery_operations o JOIN team_accounts a ON (a.team_id,a.id)=(o.team_id,o.actor_id)
 WHERE o.team_id=s.team_id AND o.subject_id=s.account_id AND o.status='authorized'),'[]');
END $$;
CREATE FUNCTION api_recovery_verify(token text,operation uuid,credential uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 IF NOT EXISTS(SELECT FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND subject_id=s.account_id AND status='authorized')
 OR NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=credential AND owner_account_id=s.account_id AND visibility='private' AND NOT deleted)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 RETURN (SELECT jsonb_build_object('credentialId',credential,'versions',jsonb_agg(jsonb_build_object('revision',v.revision,
 'encryptedData',encode(v.encrypted_data,'hex'),'encryptedSecret',encode(v.encrypted_secret,'hex'),
 'envelopes',(SELECT jsonb_agg(jsonb_build_object('purpose',e.purpose,'envelope',encode(e.envelope,'hex'))) FROM credential_key_envelopes e
 JOIN account_crypto_identities i ON (i.team_id,i.account_id,i.version)=(e.team_id,e.account_id,e.identity_version) AND i.active
 WHERE (e.team_id,e.credential_id,e.revision,e.account_id)=(v.team_id,v.credential_id,v.revision,s.account_id)))))
 FROM credential_versions v WHERE v.team_id=s.team_id AND v.credential_id=credential);
END $$;
CREATE FUNCTION api_recovery_cancel(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 UPDATE recovery_operations SET status='failed',completed_at=clock_timestamp() WHERE team_id=s.team_id AND id=operation AND actor_id=s.account_id AND status='authorized';
 IF NOT FOUND THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'recovery.cancel','succeeded');
 RETURN '{"cancelled":true}';
END $$;
CREATE OR REPLACE FUNCTION api_recovery_complete(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; op recovery_operations;
BEGIN
 -- The recovered USER confirms successful decryption on their desktop. A leader
 -- cannot declare ciphertext recovered just because envelopes were uploaded.
 s:=session_actor(token);
 SELECT * INTO op FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND subject_id=s.account_id AND status IN ('authorized','completed') FOR UPDATE;
 IF op.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF op.status='completed' THEN RETURN '{"completed":true}'; END IF;
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

REVOKE ALL ON FUNCTION api_recovery_manifest(text,uuid),api_recovery_pending(text),api_recovery_verify(text,uuid,uuid),api_recovery_cancel(text,uuid) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(26);
COMMIT;
