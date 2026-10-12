BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION api_profile_history(token text,host uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;h text:=coalesce(host::text,'default');
BEGIN
 s:=session_actor(token);
 IF host IS NOT NULL AND NOT connection_profile_allowed(s.team_id,s.account_id,host) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('hostId',host,'revision',a.revision,'method',a.method,'credentialId',a.credential_id,
 'encryptedUsername',encode(a.encrypted_username,'hex'),'createdAt',a.created_at,
 'userEnvelope',(SELECT encode(e.envelope,'hex') FROM profile_user_envelopes e JOIN account_crypto_identities i ON
 (i.team_id,i.account_id,i.version)=(e.team_id,e.account_id,e.identity_version) AND i.active
 WHERE (e.team_id,e.account_id,e.host_key,e.revision)=(a.team_id,a.account_id,a.host_key,a.revision))) ORDER BY a.revision DESC)
 FROM profile_username_archives a WHERE a.team_id=s.team_id AND a.account_id=s.account_id AND a.host_key=h),'[]');
END $$;
REVOKE ALL ON FUNCTION api_profile_history(text,uuid) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(52);
COMMIT;
