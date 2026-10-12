BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Shared copies have their own resource identity, ACL and immutable ciphertext.
-- Neither deleting nor changing this copy mutates the original private credential.
CREATE TABLE shared_credential_scopes (
 team_id uuid NOT NULL, credential_id uuid NOT NULL, scope_id uuid NOT NULL, creator_id uuid NOT NULL,
 PRIMARY KEY(team_id,credential_id), FOREIGN KEY(team_id,credential_id) REFERENCES credentials(team_id,id),
 FOREIGN KEY(team_id,scope_id) REFERENCES resources(team_id,id), FOREIGN KEY(team_id,creator_id) REFERENCES team_accounts(team_id,id)
);
CREATE INDEX shared_credentials_scope ON shared_credential_scopes(team_id,scope_id,credential_id);
CREATE TABLE credential_share_plans (
 team_id uuid NOT NULL, id uuid NOT NULL DEFAULT gen_random_uuid(), actor_id uuid NOT NULL,
 source_id uuid NOT NULL, source_revision bigint NOT NULL, scope_id uuid NOT NULL, scope_revision bigint NOT NULL,
 credential_id uuid NOT NULL DEFAULT gen_random_uuid(), acl_revision bigint NOT NULL, recovery_id uuid NOT NULL,
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','applied','cancelled')),
 expires_at timestamptz NOT NULL DEFAULT now()+interval '15 minutes', request_hash bytea,
 PRIMARY KEY(team_id,id), UNIQUE(team_id,credential_id), FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,source_id) REFERENCES credentials(team_id,id), FOREIGN KEY(team_id,scope_id) REFERENCES resources(team_id,id),
 FOREIGN KEY(team_id,recovery_id) REFERENCES team_recovery_keys(team_id,id)
);
CREATE INDEX credential_share_pending ON credential_share_plans(team_id,actor_id,expires_at) WHERE status='pending';
CREATE TABLE credential_share_recipients (
 team_id uuid NOT NULL, plan_id uuid NOT NULL, account_id uuid NOT NULL, identity_version bigint NOT NULL,
 can_connect boolean NOT NULL, can_reveal boolean NOT NULL, can_export boolean NOT NULL,
 PRIMARY KEY(team_id,plan_id,account_id), FOREIGN KEY(team_id,plan_id) REFERENCES credential_share_plans(team_id,id) ON DELETE CASCADE,
 FOREIGN KEY(team_id,account_id,identity_version) REFERENCES account_crypto_identities(team_id,account_id,version),
 CHECK(can_connect OR can_reveal OR can_export)
);
ALTER TABLE shared_credential_scopes ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_share_plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_share_recipients ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON shared_credential_scopes,credential_share_plans,credential_share_recipients FROM PUBLIC;

CREATE FUNCTION share_plan_json(p credential_share_plans) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',p.id,'sourceId',p.source_id,'sourceRevision',p.source_revision,'credentialId',p.credential_id,
 'scopeId',p.scope_id,'expiresAt',p.expires_at,'recoveryPublicKey',encode(k.public_key,'hex'),
 'recipients',(SELECT jsonb_agg(jsonb_build_object('accountId',r.account_id,'identityVersion',r.identity_version,'publicKey',encode(i.public_key,'hex'),
 'displayName',a.display_name,'connect',r.can_connect,'reveal',r.can_reveal,'export',r.can_export) ORDER BY r.account_id)
 FROM credential_share_recipients r JOIN account_crypto_identities i ON (i.team_id,i.account_id,i.version)=(r.team_id,r.account_id,r.identity_version)
 JOIN team_accounts a ON (a.team_id,a.id)=(r.team_id,r.account_id) WHERE r.team_id=p.team_id AND r.plan_id=p.id))
 FROM team_recovery_keys k WHERE k.team_id=p.team_id AND k.id=p.recovery_id
$$;
CREATE FUNCTION api_share_candidates(token text,scope uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 IF NOT allowed(s.team_id,s.account_id,scope,'edit') OR NOT EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=scope AND kind IN ('folder','host'))
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('accountId',a.id,'displayName',a.display_name,'username',a.username) ORDER BY a.username)
 FROM team_accounts a WHERE a.team_id=s.team_id AND a.status='active' AND allowed(s.team_id,a.id,scope,'connect')
 AND EXISTS(SELECT FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=a.id AND active)),'[]');
END $$;
CREATE FUNCTION api_share_prepare(token text,source uuid,scope uuid,recipients jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p credential_share_plans;c credentials;r jsonb;k uuid;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 SELECT * INTO c FROM credentials WHERE team_id=s.team_id AND id=source AND visibility='private' AND owner_account_id=s.account_id AND NOT deleted;
 IF c.id IS NULL OR c.kind NOT IN ('ssh_key','password') OR NOT allowed(s.team_id,s.account_id,scope,'edit')
 OR NOT EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=scope AND NOT deleted AND kind IN ('folder','host'))
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF jsonb_typeof(recipients) IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(recipients)>100 OR (SELECT count(DISTINCT x->>'accountId') FROM jsonb_array_elements(recipients)x)<>jsonb_array_length(recipients)
 THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023'; END IF;
 SELECT id INTO k FROM team_recovery_keys WHERE team_id=s.team_id AND active;
 IF k IS NULL THEN RAISE EXCEPTION 'recovery_key_required' USING ERRCODE='22023'; END IF;
 IF (SELECT count(*) FROM credential_share_plans WHERE team_id=s.team_id AND actor_id=s.account_id AND status='pending' AND expires_at>now())>=20
 THEN RAISE EXCEPTION 'too_many_plans' USING ERRCODE='22023'; END IF;
 INSERT INTO credential_share_plans(team_id,actor_id,source_id,source_revision,scope_id,scope_revision,acl_revision,recovery_id)
 SELECT s.team_id,s.account_id,source,c.revision,scope,x.revision,t.acl_revision,k FROM resources x JOIN teams t ON t.id=x.team_id
 WHERE x.team_id=s.team_id AND x.id=scope RETURNING * INTO p;
 FOR r IN SELECT * FROM jsonb_array_elements(recipients) LOOP
  IF r - 'accountId' - 'connect' - 'reveal' - 'export'<>'{}'::jsonb OR jsonb_typeof(r->'connect') IS DISTINCT FROM 'boolean'
  OR jsonb_typeof(r->'reveal') IS DISTINCT FROM 'boolean' OR jsonb_typeof(r->'export') IS DISTINCT FROM 'boolean'
  OR NOT ((r->>'connect')::boolean OR (r->>'reveal')::boolean OR (r->>'export')::boolean)
  THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023'; END IF;
  IF NOT allowed(s.team_id,(r->>'accountId')::uuid,scope,'connect') THEN RAISE EXCEPTION 'recipient_scope_denied' USING ERRCODE='42501'; END IF;
  INSERT INTO credential_share_recipients
  SELECT s.team_id,p.id,i.account_id,i.version,(r->>'connect')::boolean,(r->>'reveal')::boolean,(r->>'export')::boolean
  FROM account_crypto_identities i JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id)
  WHERE i.team_id=s.team_id AND i.account_id=(r->>'accountId')::uuid AND i.active AND a.status='active';
  IF NOT FOUND THEN RAISE EXCEPTION 'identity_required' USING ERRCODE='22023'; END IF;
 END LOOP;
 -- The source owner keeps control of its independent shared copy. Team owners
 -- receive shared keys, but never the original private credential or its keys.
 INSERT INTO credential_share_recipients
 SELECT s.team_id,p.id,i.account_id,i.version,true,true,true FROM account_crypto_identities i JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id)
 WHERE i.team_id=s.team_id AND i.active AND a.status='active' AND (i.account_id=s.account_id OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=i.account_id AND role='owner'))
 ON CONFLICT(team_id,plan_id,account_id) DO UPDATE SET can_connect=true,can_reveal=true,can_export=true;
 RETURN share_plan_json(p);
END $$;
CREATE FUNCTION api_share_plan(token text,plan uuid,cancel boolean) RETURNS jsonb
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
 IF p.status<>'pending' OR p.expires_at<=now() OR NOT allowed(s.team_id,s.account_id,p.scope_id,'edit') THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='42501'; END IF;
 RETURN share_plan_json(p);
END $$;
CREATE FUNCTION api_share_finish(token text,plan uuid,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p credential_share_plans;c credentials;e jsonb;v uuid;request bytea:=public.digest(body::text,'sha256');
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 SELECT * INTO p FROM credential_share_plans WHERE team_id=s.team_id AND id=plan AND actor_id=s.account_id FOR UPDATE;
 IF p.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 IF p.status='applied' THEN
  IF p.request_hash IS DISTINCT FROM request THEN RAISE EXCEPTION 'operation_mismatch' USING ERRCODE='22023'; END IF;
  RETURN jsonb_build_object('credentialId',p.credential_id,'revision',1);
 END IF;
 SELECT * INTO c FROM credentials WHERE team_id=s.team_id AND id=p.source_id AND visibility='private' AND owner_account_id=s.account_id AND NOT deleted;
 SELECT vault_id INTO v FROM resources WHERE team_id=s.team_id AND id=p.scope_id AND NOT deleted AND revision=p.scope_revision;
 IF p.status<>'pending' OR p.expires_at<=now() OR c.id IS NULL OR c.revision<>p.source_revision OR v IS NULL
 OR p.acl_revision<>(SELECT acl_revision FROM teams WHERE id=s.team_id) OR NOT allowed(s.team_id,s.account_id,p.scope_id,'edit')
 OR NOT EXISTS(SELECT FROM team_recovery_keys WHERE team_id=s.team_id AND id=p.recovery_id AND active)
 OR EXISTS(SELECT FROM credential_share_recipients r WHERE r.team_id=s.team_id AND r.plan_id=p.id AND NOT EXISTS(
 SELECT FROM account_crypto_identities i JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id)
 WHERE i.team_id=s.team_id AND i.account_id=r.account_id AND i.version=r.identity_version AND i.active AND a.status='active'))
 THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001'; END IF;
 IF body - 'encryptedData' - 'encryptedSecret' - 'recoveryData' - 'recoverySecret' - 'envelopes'<>'{}'::jsonb
 OR jsonb_typeof(body->'envelopes') IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'invalid_payload' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(body->'envelopes')<>(SELECT count(*) FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id)
 OR (SELECT count(DISTINCT x->>'accountId') FROM jsonb_array_elements(body->'envelopes')x)<>jsonb_array_length(body->'envelopes')
 THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023'; END IF;
 INSERT INTO resources(team_id,vault_id,id,kind) VALUES(s.team_id,v,p.credential_id,'credential');
 INSERT INTO credentials(team_id,id,visibility,resource_id,kind) VALUES(s.team_id,p.credential_id,'shared',p.credential_id,c.kind);
 INSERT INTO shared_credential_scopes VALUES(s.team_id,p.credential_id,p.scope_id,s.account_id);
 INSERT INTO credential_versions(team_id,credential_id,revision,encrypted_data,encrypted_secret,actor_id)
 VALUES(s.team_id,p.credential_id,1,decode(body->>'encryptedData','hex'),decode(body->>'encryptedSecret','hex'),s.account_id);
 FOR e IN SELECT * FROM jsonb_array_elements(body->'envelopes') LOOP
  IF e - 'accountId' - 'identityVersion' - 'data' - 'secret'<>'{}'::jsonb OR NOT EXISTS(SELECT FROM credential_share_recipients
  WHERE team_id=s.team_id AND plan_id=p.id AND account_id=(e->>'accountId')::uuid AND identity_version=(e->>'identityVersion')::bigint)
  THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023'; END IF;
  INSERT INTO credential_key_envelopes(team_id,credential_id,revision,account_id,identity_version,purpose,envelope) VALUES
  (s.team_id,p.credential_id,1,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,'data',decode(e->>'data','hex')),
  (s.team_id,p.credential_id,1,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,'secret',decode(e->>'secret','hex'));
 END LOOP;
 INSERT INTO credential_key_envelopes(team_id,credential_id,revision,recovery_key_id,purpose,envelope) VALUES
 (s.team_id,p.credential_id,1,p.recovery_id,'data',decode(body->>'recoveryData','hex')),
 (s.team_id,p.credential_id,1,p.recovery_id,'secret',decode(body->>'recoverySecret','hex'));
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'read','allow' FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'connect','allow' FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_connect;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'reveal','allow' FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_reveal;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'export','allow' FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_export;
 UPDATE credential_share_plans SET status='applied',request_hash=request WHERE team_id=s.team_id AND id=p.id;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'credential.share',p.credential_id,'succeeded');
 RETURN jsonb_build_object('credentialId',p.credential_id,'revision',1);
END $$;
CREATE FUNCTION api_shared_credentials(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',c.id,'kind',c.kind,'revision',c.revision,'scopeId',sc.scope_id,
 'encryptedData',encode(v.encrypted_data,'hex'),'dataEnvelope',encode(e.envelope,'hex'),'identityVersion',e.identity_version,
 'permissions',ARRAY(SELECT p FROM unnest(ARRAY['connect','reveal','export'])p WHERE allowed(s.team_id,s.account_id,c.resource_id,p))) ORDER BY c.id)
 FROM credentials c JOIN shared_credential_scopes sc ON (sc.team_id,sc.credential_id)=(c.team_id,c.id)
 JOIN credential_versions v ON (v.team_id,v.credential_id,v.revision)=(c.team_id,c.id,c.revision)
 LEFT JOIN credential_key_envelopes e ON (e.team_id,e.credential_id,e.revision,e.account_id,e.purpose)=(c.team_id,c.id,c.revision,s.account_id,'data')
 AND e.identity_version=(SELECT version FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND active)
 WHERE c.team_id=s.team_id AND c.visibility='shared' AND NOT c.deleted AND allowed(s.team_id,s.account_id,c.resource_id,'read') AND allowed(s.team_id,s.account_id,sc.scope_id,'connect')),'[]');
END $$;
CREATE FUNCTION api_shared_credential_get(token text,c uuid,purpose text,host uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;row credentials;scope uuid;answer jsonb;
BEGIN
 s:=session_actor(token);
 SELECT * INTO row FROM credentials WHERE team_id=s.team_id AND id=c AND visibility='shared' AND NOT deleted;
 SELECT scope_id INTO scope FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c;
 IF row.id IS NULL OR purpose NOT IN ('connect','reveal','export') OR purpose IS NULL OR NOT allowed(s.team_id,s.account_id,row.resource_id,purpose) OR NOT allowed(s.team_id,s.account_id,scope,'connect')
 OR (purpose='connect' AND (host IS NULL OR NOT allowed(s.team_id,s.account_id,host,'connect') OR NOT within_scope(s.team_id,host,scope)))
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT jsonb_build_object('id',row.id,'revision',row.revision,'kind',row.kind,'encryptedData',encode(v.encrypted_data,'hex'),
 'encryptedSecret',encode(v.encrypted_secret,'hex'),'envelopes',(SELECT jsonb_agg(jsonb_build_object('purpose',e.purpose,'envelope',encode(envelope,'hex'),'identityVersion',identity_version))
 FROM credential_key_envelopes e WHERE team_id=s.team_id AND credential_id=c AND revision=v.revision AND account_id=s.account_id)) INTO answer
 FROM credential_versions v WHERE v.team_id=s.team_id AND v.credential_id=c AND v.revision=row.revision;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'credential.'||purpose,row.resource_id,'succeeded');
 RETURN answer;
END $$;
CREATE FUNCTION api_shared_credential_remove(token text,c uuid,expected bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;row credentials;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO row FROM credentials WHERE team_id=s.team_id AND id=c AND visibility='shared' FOR UPDATE;
 IF row.id IS NULL OR NOT EXISTS(SELECT FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c
 AND (creator_id=s.account_id OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner')))
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF row.revision IS DISTINCT FROM expected THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
 IF row.deleted THEN RETURN '{"status":"removed"}'; END IF;
 UPDATE credentials SET deleted=true WHERE team_id=s.team_id AND id=c;
 UPDATE resources SET deleted=true,modified_at=now() WHERE team_id=s.team_id AND id=row.resource_id;
 DELETE FROM credential_key_envelopes WHERE team_id=s.team_id AND credential_id=c AND account_id IS NOT NULL;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'credential.unshare',row.resource_id,'succeeded');
 RETURN '{"status":"removed"}';
END $$;
CREATE OR REPLACE FUNCTION credential_usable(t uuid,a uuid,c uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT FROM credentials WHERE team_id=t AND id=c AND NOT deleted AND
 ((visibility='private' AND owner_account_id=a) OR (visibility='shared' AND allowed(t,a,resource_id,'connect') AND EXISTS(
 SELECT FROM shared_credential_scopes sc WHERE sc.team_id=t AND sc.credential_id=c AND allowed(t,a,sc.scope_id,'connect')))))
$$;
-- Both initial connections and ongoing transport permission checks validate the
-- selected credential. Removing a shared copy must close existing connections.
CREATE OR REPLACE FUNCTION connection_target_current(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;c uuid;m text;
BEGIN
 s:=session_actor(token);
 IF NOT allowed(s.team_id,s.account_id,r,'connect') OR NOT EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=r)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF EXISTS(SELECT FROM host_hops WHERE team_id=s.team_id AND host_id=r)
 THEN RAISE EXCEPTION 'chain_requires_dependency_bundle' USING ERRCODE='22023'; END IF;
 SELECT credential_id,method INTO c,m FROM user_host_profiles WHERE team_id=s.team_id AND account_id=s.account_id AND host_id=r;
 IF m IS NULL OR m='inherit' THEN SELECT credential_id,method INTO c,m FROM user_connection_defaults WHERE team_id=s.team_id AND account_id=s.account_id; END IF;
 IF c IS NOT NULL AND (NOT credential_usable(s.team_id,s.account_id,c) OR EXISTS(SELECT FROM shared_credential_scopes
 WHERE team_id=s.team_id AND credential_id=c AND NOT within_scope(s.team_id,r,scope_id))) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 RETURN shared_resource_json(s.team_id,s.account_id,r);
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(21,now());
COMMIT;
