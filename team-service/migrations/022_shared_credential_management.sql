BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE OR REPLACE FUNCTION api_shared_credentials(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',c.id,'kind',c.kind,'revision',c.revision,'scopeId',sc.scope_id,'canRemove',sc.creator_id=s.account_id OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner'),
 'encryptedData',encode(v.encrypted_data,'hex'),'dataEnvelope',encode(e.envelope,'hex'),'identityVersion',e.identity_version,
 'permissions',ARRAY(SELECT p FROM unnest(ARRAY['connect','reveal','export'])p WHERE allowed(s.team_id,s.account_id,c.resource_id,p))) ORDER BY c.id)
 FROM credentials c JOIN shared_credential_scopes sc ON (sc.team_id,sc.credential_id)=(c.team_id,c.id)
 JOIN credential_versions v ON (v.team_id,v.credential_id,v.revision)=(c.team_id,c.id,c.revision)
 LEFT JOIN credential_key_envelopes e ON (e.team_id,e.credential_id,e.revision,e.account_id,e.purpose)=(c.team_id,c.id,c.revision,s.account_id,'data')
 AND e.identity_version=(SELECT version FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND active)
 WHERE c.team_id=s.team_id AND c.visibility='shared' AND NOT c.deleted AND allowed(s.team_id,s.account_id,c.resource_id,'read') AND allowed(s.team_id,s.account_id,sc.scope_id,'connect')),'[]');
END $$;

INSERT INTO schema_migrations(version) VALUES(22);
COMMIT;
