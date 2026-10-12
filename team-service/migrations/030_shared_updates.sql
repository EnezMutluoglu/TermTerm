BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE TABLE credential_update_plans (
 team_id uuid NOT NULL,id uuid NOT NULL DEFAULT gen_random_uuid(),actor_id uuid NOT NULL,credential_id uuid NOT NULL,
 expected_revision bigint NOT NULL,source_id uuid,source_revision bigint,scope_id uuid NOT NULL,scope_revision bigint NOT NULL,
 acl_revision bigint NOT NULL,recovery_id uuid NOT NULL,status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','applied','cancelled')),
 expires_at timestamptz NOT NULL DEFAULT now()+interval '15 minutes',request_hash bytea,
 PRIMARY KEY(team_id,id),FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,credential_id) REFERENCES credentials(team_id,id),FOREIGN KEY(team_id,source_id) REFERENCES credentials(team_id,id),
 FOREIGN KEY(team_id,scope_id) REFERENCES resources(team_id,id),FOREIGN KEY(team_id,recovery_id) REFERENCES team_recovery_keys(team_id,id)
);
CREATE INDEX credential_updates_pending ON credential_update_plans(team_id,actor_id,expires_at) WHERE status='pending';
CREATE TABLE credential_update_recipients (
 team_id uuid NOT NULL,plan_id uuid NOT NULL,account_id uuid NOT NULL,identity_version bigint NOT NULL,
 can_connect boolean NOT NULL,can_reveal boolean NOT NULL,can_export boolean NOT NULL,
 PRIMARY KEY(team_id,plan_id,account_id),FOREIGN KEY(team_id,plan_id) REFERENCES credential_update_plans(team_id,id) ON DELETE CASCADE,
 FOREIGN KEY(team_id,account_id,identity_version) REFERENCES account_crypto_identities(team_id,account_id,version),
 CHECK(can_connect OR can_reveal OR can_export)
);
ALTER TABLE credential_update_plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_update_recipients ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON credential_update_plans,credential_update_recipients FROM PUBLIC;

CREATE FUNCTION shared_update_allowed(t uuid,a uuid,c uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT FROM shared_credential_scopes sc JOIN credentials cr ON (cr.team_id,cr.id)=(sc.team_id,sc.credential_id)
 WHERE sc.team_id=t AND sc.credential_id=c AND NOT cr.deleted AND allowed(t,a,sc.scope_id,'edit')
 AND allowed(t,a,sc.scope_id,'connect') AND allowed(t,a,c,'reveal') AND
 (sc.creator_id=a OR EXISTS(SELECT FROM team_memberships WHERE team_id=t AND account_id=a AND role='owner') OR
 (EXISTS(SELECT FROM team_memberships WHERE team_id=t AND account_id=a AND role='manager') AND
 NOT EXISTS(SELECT p FROM unnest(ARRAY['connect','edit'])p WHERE NOT EXISTS(
 SELECT FROM acl_delegations d WHERE d.team_id=t AND d.account_id=a AND d.permission=p AND within_scope(t,sc.scope_id,d.resource_id))))))
$$;
CREATE FUNCTION credential_update_json(p credential_update_plans) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',p.id,'credentialId',p.credential_id,'expectedRevision',p.expected_revision,'revision',p.expected_revision+1,
 'sourceId',p.source_id,'sourceRevision',p.source_revision,'scopeId',p.scope_id,'expiresAt',p.expires_at,
 'recoveryPublicKey',encode(k.public_key,'hex'),'recipients',(SELECT jsonb_agg(jsonb_build_object('accountId',r.account_id,
 'identityVersion',r.identity_version,'publicKey',encode(i.public_key,'hex'),'displayName',a.display_name,
 'connect',r.can_connect,'reveal',r.can_reveal,'export',r.can_export) ORDER BY r.account_id)
 FROM credential_update_recipients r JOIN account_crypto_identities i ON(i.team_id,i.account_id,i.version)=(r.team_id,r.account_id,r.identity_version)
 JOIN team_accounts a ON(a.team_id,a.id)=(r.team_id,r.account_id) WHERE r.team_id=p.team_id AND r.plan_id=p.id))
 FROM team_recovery_keys k WHERE k.team_id=p.team_id AND k.id=p.recovery_id
$$;
CREATE FUNCTION api_shared_update_options(token text,c uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;scope uuid;
BEGIN
 s:=session_actor(token);IF NOT shared_update_allowed(s.team_id,s.account_id,c) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501';END IF;
 SELECT scope_id INTO scope FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c;
 RETURN jsonb_build_object('credentialId',c,'scopeId',scope,'revision',(SELECT revision FROM credentials WHERE team_id=s.team_id AND id=c),
 'recipients',coalesce((SELECT jsonb_agg(jsonb_build_object('accountId',a.id,'displayName',a.display_name,
 'connect',allowed(s.team_id,a.id,c,'connect'),'reveal',allowed(s.team_id,a.id,c,'reveal'),'export',allowed(s.team_id,a.id,c,'export')) ORDER BY a.id)
 FROM team_accounts a WHERE a.team_id=s.team_id AND a.status='active' AND allowed(s.team_id,a.id,scope,'connect')
 AND EXISTS(SELECT FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=a.id AND active)),'[]'));
END $$;
CREATE FUNCTION api_shared_update_prepare(token text,c uuid,expected bigint,source uuid,recipients jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p credential_update_plans;cr credentials;sc shared_credential_scopes;r jsonb;privileged boolean;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 IF NOT shared_update_allowed(s.team_id,s.account_id,c) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501';END IF;
 SELECT * INTO cr FROM credentials WHERE team_id=s.team_id AND id=c;
 SELECT * INTO sc FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c;
 IF expected IS DISTINCT FROM cr.revision THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001';END IF;
 IF source IS NOT NULL AND NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=source AND visibility='private'
 AND owner_account_id=s.account_id AND kind=cr.kind AND NOT deleted) THEN RAISE EXCEPTION 'invalid_source' USING ERRCODE='42501';END IF;
 IF jsonb_typeof(recipients) IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023';END IF;
 IF jsonb_array_length(recipients)>100 OR (SELECT count(DISTINCT x->>'accountId') FROM jsonb_array_elements(recipients)x)<>jsonb_array_length(recipients)
 THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023';END IF;
 IF (SELECT count(*) FROM credential_update_plans WHERE team_id=s.team_id AND actor_id=s.account_id AND status='pending' AND expires_at>now())>=20
 THEN RAISE EXCEPTION 'too_many_plans' USING ERRCODE='22023';END IF;
 INSERT INTO credential_update_plans(team_id,actor_id,credential_id,expected_revision,source_id,source_revision,scope_id,scope_revision,acl_revision,recovery_id)
 SELECT s.team_id,s.account_id,c,expected,source,(SELECT revision FROM credentials WHERE team_id=s.team_id AND id=source),sc.scope_id,x.revision,t.acl_revision,k.id
 FROM resources x JOIN teams t ON t.id=x.team_id JOIN team_recovery_keys k ON k.team_id=t.id AND k.active WHERE x.team_id=s.team_id AND x.id=sc.scope_id AND NOT x.deleted RETURNING * INTO p;
 IF p.id IS NULL THEN RAISE EXCEPTION 'recovery_or_scope_unavailable' USING ERRCODE='22023';END IF;
 privileged:=sc.creator_id=s.account_id OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner');
 FOR r IN SELECT * FROM jsonb_array_elements(recipients) LOOP
  IF r-'accountId'-'connect'-'reveal'-'export'<>'{}'::jsonb OR jsonb_typeof(r->'connect') IS DISTINCT FROM 'boolean'
  OR jsonb_typeof(r->'reveal') IS DISTINCT FROM 'boolean' OR jsonb_typeof(r->'export') IS DISTINCT FROM 'boolean'
  OR NOT((r->>'connect')::boolean OR (r->>'reveal')::boolean OR (r->>'export')::boolean)
  THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023';END IF;
  IF NOT allowed(s.team_id,(r->>'accountId')::uuid,sc.scope_id,'connect') THEN RAISE EXCEPTION 'recipient_scope_denied' USING ERRCODE='42501';END IF;
  -- A delegated manager cannot add secret/export rights or elevate their own rights.
  IF NOT privileged AND (((r->>'reveal')::boolean AND NOT allowed(s.team_id,(r->>'accountId')::uuid,c,'reveal'))
  OR ((r->>'export')::boolean AND NOT allowed(s.team_id,(r->>'accountId')::uuid,c,'export'))
  OR ((r->>'accountId')::uuid=s.account_id AND (r->>'connect')::boolean AND NOT allowed(s.team_id,s.account_id,c,'connect')))
  THEN RAISE EXCEPTION 'delegation_scope_denied' USING ERRCODE='42501';END IF;
  INSERT INTO credential_update_recipients SELECT s.team_id,p.id,i.account_id,i.version,(r->>'connect')::boolean,(r->>'reveal')::boolean,(r->>'export')::boolean
  FROM account_crypto_identities i JOIN team_accounts a ON(a.team_id,a.id)=(i.team_id,i.account_id)
  WHERE i.team_id=s.team_id AND i.account_id=(r->>'accountId')::uuid AND i.active AND a.status='active';
  IF NOT FOUND THEN RAISE EXCEPTION 'identity_required' USING ERRCODE='22023';END IF;
 END LOOP;
 INSERT INTO credential_update_recipients SELECT s.team_id,p.id,i.account_id,i.version,true,true,true
 FROM account_crypto_identities i JOIN team_accounts a ON(a.team_id,a.id)=(i.team_id,i.account_id)
 WHERE i.team_id=s.team_id AND i.active AND a.status='active' AND allowed(s.team_id,a.id,sc.scope_id,'connect')
 AND(i.account_id=sc.creator_id OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=i.account_id AND role='owner'))
 ON CONFLICT(team_id,plan_id,account_id) DO UPDATE SET can_connect=true,can_reveal=true,can_export=true;
 RETURN credential_update_json(p);
END $$;
CREATE FUNCTION api_shared_update_plan(token text,plan uuid,cancel boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p credential_update_plans;
BEGIN
 s:=session_actor(token);SELECT * INTO p FROM credential_update_plans WHERE team_id=s.team_id AND id=plan AND actor_id=s.account_id FOR UPDATE;
 IF p.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002';END IF;
 IF cancel THEN
  IF p.status='applied' THEN RAISE EXCEPTION 'already_applied' USING ERRCODE='22023';END IF;
  UPDATE credential_update_plans SET status='cancelled' WHERE team_id=s.team_id AND id=plan;RETURN '{"status":"cancelled"}';
 END IF;
 IF p.status='applied' THEN RETURN jsonb_build_object('status','applied','credentialId',p.credential_id,'revision',p.expected_revision+1);END IF;
 IF p.status<>'pending' OR p.expires_at<=now() OR NOT shared_update_allowed(s.team_id,s.account_id,p.credential_id)
 THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='42501';END IF;
 RETURN credential_update_json(p);
END $$;
CREATE FUNCTION api_shared_update_finish(token text,plan uuid,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p credential_update_plans;e jsonb;rev bigint;request bytea:=public.digest(body::text,'sha256');
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 SELECT * INTO p FROM credential_update_plans WHERE team_id=s.team_id AND id=plan AND actor_id=s.account_id FOR UPDATE;
 IF p.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002';END IF;
 rev:=p.expected_revision+1;
 IF p.status='applied' THEN
  IF request IS DISTINCT FROM p.request_hash THEN RAISE EXCEPTION 'operation_mismatch' USING ERRCODE='22023';END IF;
  RETURN jsonb_build_object('credentialId',p.credential_id,'revision',rev);
 END IF;
 IF p.status<>'pending' OR p.expires_at<=now() OR NOT shared_update_allowed(s.team_id,s.account_id,p.credential_id)
 OR NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=p.credential_id AND revision=p.expected_revision AND NOT deleted)
 OR NOT EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=p.scope_id AND revision=p.scope_revision AND NOT deleted)
 OR p.acl_revision<>(SELECT acl_revision FROM teams WHERE id=s.team_id)
 OR NOT EXISTS(SELECT FROM team_recovery_keys WHERE team_id=s.team_id AND id=p.recovery_id AND active)
 OR (p.source_id IS NOT NULL AND NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=p.source_id AND revision=p.source_revision AND owner_account_id=s.account_id AND NOT deleted))
 OR EXISTS(SELECT FROM credential_update_recipients r WHERE r.team_id=s.team_id AND r.plan_id=p.id AND
 (NOT allowed(s.team_id,r.account_id,p.scope_id,'connect') OR NOT EXISTS(SELECT FROM account_crypto_identities i JOIN team_accounts a ON(a.team_id,a.id)=(i.team_id,i.account_id)
 WHERE i.team_id=s.team_id AND i.account_id=r.account_id AND i.version=r.identity_version AND i.active AND a.status='active')))
 THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001';END IF;
 IF body-'encryptedData'-'encryptedSecret'-'recoveryData'-'recoverySecret'-'envelopes'<>'{}'::jsonb OR jsonb_typeof(body->'envelopes') IS DISTINCT FROM 'array'
 THEN RAISE EXCEPTION 'invalid_payload' USING ERRCODE='22023';END IF;
 IF jsonb_array_length(body->'envelopes')<>(SELECT count(*) FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id)
 OR (SELECT count(DISTINCT x->>'accountId') FROM jsonb_array_elements(body->'envelopes')x)<>jsonb_array_length(body->'envelopes') THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023';END IF;
 INSERT INTO credential_versions(team_id,credential_id,revision,encrypted_data,encrypted_secret,actor_id)
 VALUES(s.team_id,p.credential_id,rev,decode(body->>'encryptedData','hex'),decode(body->>'encryptedSecret','hex'),s.account_id);
 FOR e IN SELECT * FROM jsonb_array_elements(body->'envelopes') LOOP
  IF e-'accountId'-'identityVersion'-'data'-'secret'<>'{}'::jsonb OR NOT EXISTS(SELECT FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id AND account_id=(e->>'accountId')::uuid AND identity_version=(e->>'identityVersion')::bigint)
  THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023';END IF;
  INSERT INTO credential_key_envelopes(team_id,credential_id,revision,account_id,identity_version,purpose,envelope) VALUES
  (s.team_id,p.credential_id,rev,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,'data',decode(e->>'data','hex')),
  (s.team_id,p.credential_id,rev,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,'secret',decode(e->>'secret','hex'));
 END LOOP;
 INSERT INTO credential_key_envelopes(team_id,credential_id,revision,recovery_key_id,purpose,envelope) VALUES
 (s.team_id,p.credential_id,rev,p.recovery_id,'data',decode(body->>'recoveryData','hex')),
 (s.team_id,p.credential_id,rev,p.recovery_id,'secret',decode(body->>'recoverySecret','hex'));
 DELETE FROM credential_key_envelopes k WHERE k.team_id=s.team_id AND k.credential_id=p.credential_id AND k.account_id IS NOT NULL
 AND NOT EXISTS(SELECT FROM credential_update_recipients r WHERE r.team_id=s.team_id AND r.plan_id=p.id AND r.account_id=k.account_id AND r.identity_version=k.identity_version);
 DELETE FROM resource_acl WHERE team_id=s.team_id AND resource_id=p.credential_id;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'read','allow' FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'connect','allow' FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_connect;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'reveal','allow' FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_reveal;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'export','allow' FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_export;
 UPDATE credentials SET revision=rev WHERE team_id=s.team_id AND id=p.credential_id;
 UPDATE resources SET revision=rev,modified_at=now() WHERE team_id=s.team_id AND id=p.credential_id;
 DELETE FROM credential_versions WHERE team_id=s.team_id AND credential_id=p.credential_id AND revision<rev-10;
 UPDATE teams SET acl_revision=acl_revision+1 WHERE id=s.team_id;
 UPDATE credential_update_plans SET status='applied',request_hash=request WHERE team_id=s.team_id AND id=p.id;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'credential.shared_update',p.credential_id,'succeeded');
 RETURN jsonb_build_object('credentialId',p.credential_id,'revision',rev);
END $$;
-- The wrapper adds an authoritative UI capability without exposing internal functions.
ALTER FUNCTION api_shared_credentials(text) RENAME TO shared_credentials_v29;
CREATE FUNCTION api_shared_credentials(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN s:=session_actor(token);RETURN coalesce((SELECT jsonb_agg(x||jsonb_build_object('canUpdate',shared_update_allowed(s.team_id,s.account_id,(x->>'id')::uuid))) FROM jsonb_array_elements(shared_credentials_v29(token))x),'[]');END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
-- A renamed function keeps prior service grants; remove them before exposing the wrapper.
DO $$ DECLARE n text; BEGIN FOR n IN SELECT DISTINCT grantee::regrole::text FROM pg_proc p,aclexplode(coalesce(p.proacl,acldefault('f',p.proowner))) a
 WHERE p.oid='shared_credentials_v29(text)'::regprocedure AND a.grantee<>p.proowner AND a.grantee<>0
 LOOP EXECUTE format('REVOKE ALL ON FUNCTION shared_credentials_v29(text) FROM %s',n);END LOOP;END $$;
INSERT INTO schema_migrations(version) VALUES(30);
COMMIT;
