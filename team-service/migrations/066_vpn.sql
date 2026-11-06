BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- VPN configuration uses existing versioned credential envelopes. Personal login material is never shareable.
ALTER TABLE credential_versions DROP CONSTRAINT credential_versions_encrypted_secret_check;
ALTER TABLE credential_versions ADD CONSTRAINT credential_versions_encrypted_secret_check CHECK(octet_length(encrypted_secret) BETWEEN 40 AND 1048616);
ALTER TABLE credentials DROP CONSTRAINT credentials_kind_check;
ALTER TABLE credentials ADD CONSTRAINT credentials_kind_check CHECK(kind IN ('ssh_key','password','agent','hardware','proxy','totp','vpn','vpn_auth'));
CREATE FUNCTION vpn_kind_guard() RETURNS trigger LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
BEGIN
 IF (OLD.kind IN ('vpn','vpn_auth') OR NEW.kind IN ('vpn','vpn_auth')) AND OLD.kind<>NEW.kind THEN RAISE EXCEPTION 'credential_kind_conflict' USING ERRCODE='22023'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER vpn_kind_immutable BEFORE UPDATE OF kind ON credentials FOR EACH ROW EXECUTE FUNCTION vpn_kind_guard();
ALTER TABLE credential_share_recipients ADD COLUMN can_view boolean NOT NULL DEFAULT true, ADD COLUMN can_edit boolean NOT NULL DEFAULT false;
ALTER TABLE credential_update_recipients ADD COLUMN can_view boolean NOT NULL DEFAULT true, ADD COLUMN can_edit boolean NOT NULL DEFAULT false;
ALTER TABLE credential_share_recipients DROP CONSTRAINT credential_share_recipients_check;
ALTER TABLE credential_update_recipients DROP CONSTRAINT credential_update_recipients_check;
ALTER TABLE credential_share_recipients ADD CHECK(can_view OR can_connect OR can_edit OR can_reveal OR can_export);
ALTER TABLE credential_update_recipients ADD CHECK(can_view OR can_connect OR can_edit OR can_reveal OR can_export);

CREATE OR REPLACE FUNCTION share_plan_json(p credential_share_plans) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',p.id,'sourceId',p.source_id,'sourceRevision',p.source_revision,'credentialId',p.credential_id,
 'scopeId',p.scope_id,'expiresAt',p.expires_at,'recoveryPublicKey',encode(k.public_key,'hex'),
 'recipients',(SELECT jsonb_agg(jsonb_build_object('accountId',r.account_id,'identityVersion',r.identity_version,'publicKey',encode(i.public_key,'hex'),
 'displayName',a.display_name,'view',r.can_view,'edit',r.can_edit,'connect',r.can_connect,'reveal',r.can_reveal,'export',r.can_export) ORDER BY r.account_id)
 FROM credential_share_recipients r JOIN account_crypto_identities i ON (i.team_id,i.account_id,i.version)=(r.team_id,r.account_id,r.identity_version)
 JOIN team_accounts a ON (a.team_id,a.id)=(r.team_id,r.account_id) WHERE r.team_id=p.team_id AND r.plan_id=p.id))
 FROM team_recovery_keys k WHERE k.team_id=p.team_id AND k.id=p.recovery_id
$$;
CREATE OR REPLACE FUNCTION credential_update_json(p credential_update_plans) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',p.id,'credentialId',p.credential_id,'expectedRevision',p.expected_revision,'revision',p.expected_revision+1,'kind',(SELECT kind FROM credentials WHERE team_id=p.team_id AND id=p.credential_id),
 'sourceId',p.source_id,'sourceRevision',p.source_revision,'scopeId',p.scope_id,'expiresAt',p.expires_at,
 'recoveryPublicKey',encode(k.public_key,'hex'),'recipients',(SELECT jsonb_agg(jsonb_build_object('accountId',r.account_id,
 'identityVersion',r.identity_version,'publicKey',encode(i.public_key,'hex'),'displayName',a.display_name,
 'view',r.can_view,'edit',r.can_edit,'connect',r.can_connect,'reveal',r.can_reveal,'export',r.can_export) ORDER BY r.account_id)
 FROM credential_update_recipients r JOIN account_crypto_identities i ON(i.team_id,i.account_id,i.version)=(r.team_id,r.account_id,r.identity_version)
 JOIN team_accounts a ON(a.team_id,a.id)=(r.team_id,r.account_id) WHERE r.team_id=p.team_id AND r.plan_id=p.id))
 FROM team_recovery_keys k WHERE k.team_id=p.team_id AND k.id=p.recovery_id
$$;
CREATE OR REPLACE FUNCTION api_share_prepare(token text,source uuid,scope uuid,recipients jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p credential_share_plans;c credentials;r jsonb;k uuid;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 SELECT * INTO c FROM credentials WHERE team_id=s.team_id AND id=source AND visibility='private' AND owner_account_id=s.account_id AND NOT deleted;
 IF c.id IS NULL OR c.kind NOT IN ('ssh_key','password','totp','vpn') OR NOT allowed(s.team_id,s.account_id,scope,'edit')
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
  IF r - 'accountId' - 'connect' - 'reveal' - 'export' - 'view' - 'edit'<>'{}'::jsonb OR jsonb_typeof(r->'connect') IS DISTINCT FROM 'boolean'
  OR jsonb_typeof(r->'reveal') IS DISTINCT FROM 'boolean' OR jsonb_typeof(r->'export') IS DISTINCT FROM 'boolean'
  OR (r ? 'view' AND jsonb_typeof(r->'view')<>'boolean') OR (r ? 'edit' AND jsonb_typeof(r->'edit')<>'boolean')
  OR NOT (coalesce((r->>'view')::boolean,false) OR coalesce((r->>'edit')::boolean,false) OR (r->>'connect')::boolean OR (r->>'reveal')::boolean OR (r->>'export')::boolean)
  THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023'; END IF;
  IF c.kind<>'vpn' AND (coalesce((r->>'view')::boolean,true)=false OR coalesce((r->>'edit')::boolean,false)) THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023'; END IF;
  IF NOT allowed(s.team_id,(r->>'accountId')::uuid,scope,'connect') THEN RAISE EXCEPTION 'recipient_scope_denied' USING ERRCODE='42501'; END IF;
  INSERT INTO credential_share_recipients
  SELECT s.team_id,p.id,i.account_id,i.version,(r->>'connect')::boolean,(r->>'reveal')::boolean,(r->>'export')::boolean,coalesce((r->>'view')::boolean,true),coalesce((r->>'edit')::boolean,false)
  FROM account_crypto_identities i JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id)
  WHERE i.team_id=s.team_id AND i.account_id=(r->>'accountId')::uuid AND i.active AND a.status='active';
  IF NOT FOUND THEN RAISE EXCEPTION 'identity_required' USING ERRCODE='22023'; END IF;
 END LOOP;
 -- The source owner keeps control of its independent shared copy. Team owners
 -- receive shared keys, but never the original private credential or its keys.
 INSERT INTO credential_share_recipients
 SELECT s.team_id,p.id,i.account_id,i.version,true,true,true,true,(c.kind='vpn') FROM account_crypto_identities i JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id)
 WHERE i.team_id=s.team_id AND i.active AND a.status='active' AND (i.account_id=s.account_id OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=i.account_id AND role='owner'))
 ON CONFLICT(team_id,plan_id,account_id) DO UPDATE SET can_connect=true,can_reveal=true,can_export=true,can_view=true,can_edit=excluded.can_edit;
 RETURN share_plan_json(p);
END $$;
CREATE OR REPLACE FUNCTION api_shared_update_prepare(token text,c uuid,expected bigint,source uuid,recipients jsonb) RETURNS jsonb
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
  IF r-'accountId'-'connect'-'reveal'-'export'-'view'-'edit'<>'{}'::jsonb OR jsonb_typeof(r->'connect') IS DISTINCT FROM 'boolean'
  OR jsonb_typeof(r->'reveal') IS DISTINCT FROM 'boolean' OR jsonb_typeof(r->'export') IS DISTINCT FROM 'boolean'
  OR (r ? 'view' AND jsonb_typeof(r->'view')<>'boolean') OR (r ? 'edit' AND jsonb_typeof(r->'edit')<>'boolean')
  OR NOT(coalesce((r->>'view')::boolean,false) OR coalesce((r->>'edit')::boolean,false) OR (r->>'connect')::boolean OR (r->>'reveal')::boolean OR (r->>'export')::boolean)
  THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023';END IF;
  IF cr.kind<>'vpn' AND (coalesce((r->>'view')::boolean,true)=false OR coalesce((r->>'edit')::boolean,false)) THEN RAISE EXCEPTION 'invalid_recipients' USING ERRCODE='22023'; END IF;
  IF NOT allowed(s.team_id,(r->>'accountId')::uuid,sc.scope_id,'connect') THEN RAISE EXCEPTION 'recipient_scope_denied' USING ERRCODE='42501';END IF;
  -- A delegated manager cannot add secret/export rights or elevate their own rights.
  IF NOT privileged AND (((r->>'edit')::boolean AND NOT allowed(s.team_id,(r->>'accountId')::uuid,c,'edit')) OR (coalesce((r->>'view')::boolean,true) AND NOT allowed(s.team_id,(r->>'accountId')::uuid,c,'read')) OR ((r->>'reveal')::boolean AND NOT allowed(s.team_id,(r->>'accountId')::uuid,c,'reveal'))
  OR ((r->>'export')::boolean AND NOT allowed(s.team_id,(r->>'accountId')::uuid,c,'export'))
  OR ((r->>'accountId')::uuid=s.account_id AND (r->>'connect')::boolean AND NOT allowed(s.team_id,s.account_id,c,'connect')))
  THEN RAISE EXCEPTION 'delegation_scope_denied' USING ERRCODE='42501';END IF;
  INSERT INTO credential_update_recipients SELECT s.team_id,p.id,i.account_id,i.version,(r->>'connect')::boolean,(r->>'reveal')::boolean,(r->>'export')::boolean,coalesce((r->>'view')::boolean,true),coalesce((r->>'edit')::boolean,false)
  FROM account_crypto_identities i JOIN team_accounts a ON(a.team_id,a.id)=(i.team_id,i.account_id)
  WHERE i.team_id=s.team_id AND i.account_id=(r->>'accountId')::uuid AND i.active AND a.status='active';
  IF NOT FOUND THEN RAISE EXCEPTION 'identity_required' USING ERRCODE='22023';END IF;
 END LOOP;
 INSERT INTO credential_update_recipients SELECT s.team_id,p.id,i.account_id,i.version,true,true,true,true,(cr.kind='vpn')
 FROM account_crypto_identities i JOIN team_accounts a ON(a.team_id,a.id)=(i.team_id,i.account_id)
 WHERE i.team_id=s.team_id AND i.active AND a.status='active' AND allowed(s.team_id,a.id,sc.scope_id,'connect')
 AND(i.account_id=sc.creator_id OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=i.account_id AND role='owner'))
 ON CONFLICT(team_id,plan_id,account_id) DO UPDATE SET can_connect=true,can_reveal=true,can_export=true,can_view=true,can_edit=excluded.can_edit;
 RETURN credential_update_json(p);
END $$;
CREATE OR REPLACE FUNCTION api_share_finish(token text,plan uuid,body jsonb) RETURNS jsonb
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
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'read','allow' FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_view;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'connect','allow' FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_connect;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'reveal','allow' FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_reveal;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'export','allow' FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_export;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'edit','allow' FROM credential_share_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_edit AND EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=p.credential_id AND kind='vpn');
 UPDATE credential_share_plans SET status='applied',request_hash=request WHERE team_id=s.team_id AND id=p.id;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'credential.share',p.credential_id,'succeeded');
 RETURN jsonb_build_object('credentialId',p.credential_id,'revision',1);
END $$;
CREATE OR REPLACE FUNCTION api_shared_update_finish(token text,plan uuid,body jsonb) RETURNS jsonb
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
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'read','allow' FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_view;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'connect','allow' FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_connect;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'reveal','allow' FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_reveal;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'export','allow' FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_export;
 INSERT INTO resource_acl SELECT s.team_id,p.credential_id,account_id,'edit','allow' FROM credential_update_recipients WHERE team_id=s.team_id AND plan_id=p.id AND can_edit AND EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=p.credential_id AND kind='vpn');
 UPDATE credentials SET revision=rev WHERE team_id=s.team_id AND id=p.credential_id;
 UPDATE resources SET revision=rev,modified_at=now() WHERE team_id=s.team_id AND id=p.credential_id;
 DELETE FROM credential_versions WHERE team_id=s.team_id AND credential_id=p.credential_id AND revision<rev-10;
 UPDATE teams SET acl_revision=acl_revision+1 WHERE id=s.team_id;
 UPDATE credential_update_plans SET status='applied',request_hash=request WHERE team_id=s.team_id AND id=p.id;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'credential.shared_update',p.credential_id,'succeeded');
 RETURN jsonb_build_object('credentialId',p.credential_id,'revision',rev);
END $$;
CREATE OR REPLACE FUNCTION shared_update_allowed(t uuid,a uuid,c uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT FROM shared_credential_scopes sc JOIN credentials cr ON (cr.team_id,cr.id)=(sc.team_id,sc.credential_id)
 WHERE sc.team_id=t AND sc.credential_id=c AND NOT cr.deleted AND allowed(t,a,sc.scope_id,'connect') AND
 ((cr.kind='vpn' AND allowed(t,a,c,'edit')) OR (cr.kind<>'vpn' AND allowed(t,a,sc.scope_id,'edit') AND allowed(t,a,c,'reveal') AND
 (sc.creator_id=a OR EXISTS(SELECT FROM team_memberships WHERE team_id=t AND account_id=a AND role='owner') OR
 (EXISTS(SELECT FROM team_memberships WHERE team_id=t AND account_id=a AND role='manager') AND
 NOT EXISTS(SELECT p FROM unnest(ARRAY['connect','edit'])p WHERE NOT EXISTS(
 SELECT FROM acl_delegations d WHERE d.team_id=t AND d.account_id=a AND d.permission=p AND within_scope(t,sc.scope_id,d.resource_id))))))))
$$;
CREATE OR REPLACE FUNCTION api_shared_update_options(token text,c uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;scope uuid;
BEGIN
 s:=session_actor(token);IF NOT shared_update_allowed(s.team_id,s.account_id,c) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501';END IF;
 SELECT scope_id INTO scope FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c;
 RETURN jsonb_build_object('credentialId',c,'scopeId',scope,'revision',(SELECT revision FROM credentials WHERE team_id=s.team_id AND id=c),
 'recipients',coalesce((SELECT jsonb_agg(jsonb_build_object('accountId',a.id,'displayName',a.display_name,
 'view',allowed(s.team_id,a.id,c,'read'),'edit',allowed(s.team_id,a.id,c,'edit'),'connect',allowed(s.team_id,a.id,c,'connect'),'reveal',allowed(s.team_id,a.id,c,'reveal'),'export',allowed(s.team_id,a.id,c,'export')) ORDER BY a.id)
 FROM team_accounts a WHERE a.team_id=s.team_id AND a.status='active' AND allowed(s.team_id,a.id,scope,'connect')
 AND EXISTS(SELECT FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=a.id AND active)),'[]'));
END $$;
CREATE OR REPLACE FUNCTION api_shared_credentials(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',c.id,'kind',c.kind,'revision',c.revision,'scopeId',sc.scope_id,'canUpdate',shared_update_allowed(s.team_id,s.account_id,c.id),'canRemove',sc.creator_id=s.account_id OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner'),
 'encryptedData',encode(v.encrypted_data,'hex'),'dataEnvelope',encode(e.envelope,'hex'),'identityVersion',e.identity_version,
 'permissions',ARRAY(SELECT p FROM unnest(ARRAY['read','connect','edit','reveal','export'])p WHERE allowed(s.team_id,s.account_id,c.resource_id,p))) ORDER BY c.id)
 FROM credentials c JOIN shared_credential_scopes sc ON (sc.team_id,sc.credential_id)=(c.team_id,c.id)
 JOIN credential_versions v ON (v.team_id,v.credential_id,v.revision)=(c.team_id,c.id,c.revision)
 LEFT JOIN credential_key_envelopes e ON (e.team_id,e.credential_id,e.revision,e.account_id,e.purpose)=(c.team_id,c.id,c.revision,s.account_id,'data')
 AND e.identity_version=(SELECT version FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND active)
 WHERE c.team_id=s.team_id AND c.visibility='shared' AND NOT c.deleted AND allowed(s.team_id,s.account_id,c.resource_id,'read') AND allowed(s.team_id,s.account_id,sc.scope_id,'connect')),'[]');
END $$;
CREATE OR REPLACE FUNCTION api_shared_credential_get(token text,c uuid,purpose text,host uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;row credentials;scope uuid;answer jsonb;
BEGIN
 s:=session_actor(token);
 SELECT * INTO row FROM credentials WHERE team_id=s.team_id AND id=c AND visibility='shared' AND NOT deleted;
 SELECT scope_id INTO scope FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c;
 IF row.id IS NULL OR purpose NOT IN ('connect','reveal','export','otp','vpn','edit') OR purpose IS NULL OR NOT allowed(s.team_id,s.account_id,row.resource_id,CASE WHEN purpose IN ('otp','vpn') THEN 'connect' ELSE purpose END) OR NOT allowed(s.team_id,s.account_id,scope,'connect')
 OR (purpose IN ('vpn','edit') AND row.kind<>'vpn') OR (purpose IN ('connect','otp') AND row.kind IN ('vpn','vpn_auth')) OR (purpose='otp' AND row.kind<>'totp') OR (purpose='connect' AND row.kind='totp') OR (purpose='connect' AND (host IS NULL OR NOT allowed(s.team_id,s.account_id,host,'connect') OR NOT within_scope(s.team_id,host,scope)))
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT jsonb_build_object('id',row.id,'revision',row.revision,'kind',row.kind,'encryptedData',encode(v.encrypted_data,'hex'),
 'encryptedSecret',encode(v.encrypted_secret,'hex'),'envelopes',(SELECT jsonb_agg(jsonb_build_object('purpose',e.purpose,'envelope',encode(envelope,'hex'),'identityVersion',identity_version))
 FROM credential_key_envelopes e WHERE team_id=s.team_id AND credential_id=c AND revision=v.revision AND account_id=s.account_id)) INTO answer
 FROM credential_versions v WHERE v.team_id=s.team_id AND v.credential_id=c AND v.revision=row.revision;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'credential.'||purpose,row.resource_id,'succeeded');
 RETURN answer;
END $$;
CREATE OR REPLACE FUNCTION credential_usable(t uuid,a uuid,c uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT FROM credentials WHERE team_id=t AND id=c AND NOT deleted AND kind NOT IN ('totp','vpn','vpn_auth') AND
 ((visibility='private' AND owner_account_id=a) OR (visibility='shared' AND allowed(t,a,resource_id,'connect') AND EXISTS(
 SELECT FROM shared_credential_scopes sc WHERE sc.team_id=t AND sc.credential_id=c AND allowed(t,a,sc.scope_id,'connect')))))
$$;

-- Bindings are normalized control state; encrypted configuration and personal authentication stay separate.
CREATE TABLE vpn_bindings (
 team_id uuid NOT NULL,resource_id uuid NOT NULL,vpn_id uuid,revision bigint NOT NULL CHECK(revision>0),
 PRIMARY KEY(team_id,resource_id),FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id),
 FOREIGN KEY(team_id,vpn_id) REFERENCES credentials(team_id,id)
);
CREATE TABLE vpn_personal_bindings (
 team_id uuid NOT NULL,account_id uuid NOT NULL,resource_id uuid NOT NULL,vpn_id uuid,revision bigint NOT NULL CHECK(revision>0),
 PRIMARY KEY(team_id,account_id,resource_id),FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id),FOREIGN KEY(team_id,vpn_id) REFERENCES credentials(team_id,id)
);
ALTER TABLE vpn_personal_bindings ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON vpn_personal_bindings FROM PUBLIC;
CREATE TABLE vpn_auth_bindings (
 team_id uuid NOT NULL,account_id uuid NOT NULL,vpn_id uuid NOT NULL,credential_id uuid NOT NULL,
 PRIMARY KEY(team_id,account_id,vpn_id),FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,vpn_id) REFERENCES credentials(team_id,id),FOREIGN KEY(team_id,credential_id) REFERENCES credentials(team_id,id)
);
ALTER TABLE vpn_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE vpn_auth_bindings ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON vpn_bindings,vpn_auth_bindings FROM PUBLIC;
CREATE FUNCTION vpn_allowed(t uuid,a uuid,c uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT FROM credentials v WHERE v.team_id=t AND v.id=c AND v.kind='vpn' AND NOT v.deleted AND
 ((v.visibility='private' AND v.owner_account_id=a) OR (v.visibility='shared' AND allowed(t,a,v.resource_id,'connect') AND EXISTS(
 SELECT FROM shared_credential_scopes sc WHERE sc.team_id=t AND sc.credential_id=c AND allowed(t,a,sc.scope_id,'connect')))))
$$;
CREATE FUNCTION api_vpn_binding(token text,r uuid,expected bigint,c uuid,write boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;v vpn_bindings;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT allowed(s.team_id,s.account_id,r,CASE WHEN write THEN 'edit' ELSE 'read' END) OR NOT EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=r AND kind IN ('folder','host') AND NOT deleted)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT * INTO v FROM vpn_bindings WHERE team_id=s.team_id AND resource_id=r;
 IF write THEN
  IF expected IS NULL OR expected<>coalesce(v.revision,0) THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
  IF c IS NOT NULL AND (NOT vpn_allowed(s.team_id,s.account_id,c) OR NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=c AND visibility='shared') OR NOT EXISTS(SELECT FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c AND within_scope(s.team_id,r,scope_id)))
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  INSERT INTO vpn_bindings VALUES(s.team_id,r,c,expected+1) ON CONFLICT(team_id,resource_id) DO UPDATE SET vpn_id=excluded.vpn_id,revision=excluded.revision RETURNING * INTO v;
  UPDATE teams SET acl_revision=acl_revision+1 WHERE id=s.team_id;
  INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'vpn.binding',r,'succeeded');
 END IF;
 RETURN jsonb_build_object('vpnId',v.vpn_id,'revision',coalesce(v.revision,0));
END $$;
CREATE FUNCTION vpn_required(t uuid,a uuid,h uuid) RETURNS uuid LANGUAGE sql STABLE AS $$
 WITH RECURSIVE ancestry(id,depth) AS (
 SELECT h,0 UNION ALL SELECT coalesce(f.parent_id,x.folder_id),z.depth+1 FROM ancestry z
 LEFT JOIN folders f ON f.team_id=t AND f.id=z.id LEFT JOIN hosts x ON x.team_id=t AND x.id=z.id
 WHERE z.depth<64 AND coalesce(f.parent_id,x.folder_id) IS NOT NULL)
 SELECT b.vpn_id FROM ancestry z JOIN (
 SELECT resource_id,vpn_id,0 AS priority FROM vpn_personal_bindings WHERE team_id=t AND account_id=a
 UNION ALL SELECT resource_id,vpn_id,1 AS priority FROM vpn_bindings WHERE team_id=t) b ON b.resource_id=z.id
 WHERE b.vpn_id IS NOT NULL ORDER BY z.depth,b.priority LIMIT 1
$$;
CREATE FUNCTION api_vpn_required(token text,h uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;c uuid;
BEGIN
 s:=session_actor(token);
 IF NOT allowed(s.team_id,s.account_id,h,'connect') OR NOT EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=h) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 c:=vpn_required(s.team_id,s.account_id,h);
 IF c IS NOT NULL AND NOT vpn_allowed(s.team_id,s.account_id,c) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 RETURN jsonb_build_object('vpnId',c);
END $$;
CREATE FUNCTION api_vpn_required_for_plan(token text,r uuid,h uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p jsonb;c uuid;
BEGIN
 s:=session_actor(token);p:=api_connection_plan(token,r);
 -- Membership comes from the authorized plan, including the selected historical route.
 IF NOT EXISTS(SELECT FROM jsonb_array_elements(p->'steps')x WHERE (x->>'id')::uuid=h) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 c:=vpn_required(s.team_id,s.account_id,h);
 IF c IS NOT NULL AND NOT vpn_allowed(s.team_id,s.account_id,c) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 RETURN jsonb_build_object('vpnId',c);
END $$;
CREATE FUNCTION api_vpn_personal_binding(token text,r uuid,expected bigint,c uuid,write boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;v vpn_personal_bindings;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT allowed(s.team_id,s.account_id,r,'connect') OR NOT EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=r AND kind IN ('folder','host') AND NOT deleted)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT * INTO v FROM vpn_personal_bindings WHERE team_id=s.team_id AND account_id=s.account_id AND resource_id=r;
 IF write THEN
  IF expected IS NULL OR expected<>coalesce(v.revision,0) THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
  IF c IS NOT NULL AND NOT vpn_allowed(s.team_id,s.account_id,c) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  INSERT INTO vpn_personal_bindings VALUES(s.team_id,s.account_id,r,c,expected+1) ON CONFLICT(team_id,account_id,resource_id) DO UPDATE SET vpn_id=excluded.vpn_id,revision=excluded.revision RETURNING * INTO v;
 END IF;
 RETURN jsonb_build_object('vpnId',v.vpn_id,'revision',coalesce(v.revision,0));
END $$;
CREATE FUNCTION api_vpn_auth(token text,v uuid,c uuid,write boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 IF NOT vpn_allowed(s.team_id,s.account_id,v) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF write THEN
  IF NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=c AND kind='vpn_auth' AND visibility='private' AND owner_account_id=s.account_id AND NOT deleted) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  INSERT INTO vpn_auth_bindings VALUES(s.team_id,s.account_id,v,c) ON CONFLICT(team_id,account_id,vpn_id) DO UPDATE SET credential_id=excluded.credential_id;
 END IF;
 RETURN jsonb_build_object('credentialId',(SELECT credential_id FROM vpn_auth_bindings WHERE team_id=s.team_id AND account_id=s.account_id AND vpn_id=v));
END $$;
CREATE FUNCTION api_vpn_remove(token text,c uuid,expected bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;v credentials;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));SELECT * INTO v FROM credentials WHERE team_id=s.team_id AND id=c AND kind='vpn' AND NOT deleted FOR UPDATE;
 IF v.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 IF v.visibility='shared' THEN RETURN api_shared_credential_remove(token,c,expected); END IF;
 IF v.owner_account_id<>s.account_id THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF v.revision<>expected THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
 UPDATE credentials SET deleted=true WHERE team_id=s.team_id AND id=c;
 RETURN '{"status":"removed"}';
END $$;
CREATE FUNCTION api_vpn_authorize(token text,c uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN s:=session_actor(token);IF NOT vpn_allowed(s.team_id,s.account_id,c) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;RETURN jsonb_build_object('allowed',true,'visibility',(SELECT visibility FROM credentials WHERE team_id=s.team_id AND id=c),'revision',(SELECT revision FROM credentials WHERE team_id=s.team_id AND id=c),'permissions',CASE WHEN EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=c AND visibility='private' AND owner_account_id=s.account_id) THEN ARRAY['read','connect','edit','reveal','export'] ELSE ARRAY(SELECT p FROM unnest(ARRAY['read','connect','edit','reveal','export'])p WHERE allowed(s.team_id,s.account_id,c,p)) END);END $$;
CREATE FUNCTION api_vpn_legacy_connection(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p jsonb;
BEGIN s:=session_actor(token);p:=api_connection_plan(token,r);
 IF EXISTS(SELECT FROM jsonb_array_elements(p->'steps')x WHERE vpn_required(s.team_id,s.account_id,(x->>'id')::uuid) IS NOT NULL) THEN RAISE EXCEPTION 'vpn_client_required' USING ERRCODE='42501'; END IF;
 RETURN '{"allowed":true}';END $$;

-- Offline VPN mutations keep their own typed, durable receipts; resource decision delivery is never used.
CREATE TABLE vpn_create_authorities(team_id uuid NOT NULL,id uuid NOT NULL DEFAULT gen_random_uuid(),account_id uuid NOT NULL,device_id uuid NOT NULL,auth_epoch bigint NOT NULL,
 PRIMARY KEY(team_id,id),FOREIGN KEY(team_id,account_id,device_id) REFERENCES account_devices(team_id,account_id,id));
CREATE TABLE vpn_write_operations(team_id uuid NOT NULL,id uuid NOT NULL,actor_id uuid NOT NULL,device_id uuid NOT NULL,sequence bigint NOT NULL CHECK(sequence>0),vpn_id uuid NOT NULL,
 base_revision bigint NOT NULL CHECK(base_revision>=0),acl_revision bigint NOT NULL,recovery_id uuid,request_hash bytea NOT NULL CHECK(octet_length(request_hash)=32),body jsonb NOT NULL,
 status text NOT NULL CHECK(status IN ('prepared','applied','conflict','quarantined')),result_revision bigint,auth_id uuid,auth_revision bigint,created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,id),UNIQUE(team_id,actor_id,device_id,sequence),FOREIGN KEY(team_id,actor_id,device_id) REFERENCES account_devices(team_id,account_id,id));
ALTER TABLE vpn_create_authorities ENABLE ROW LEVEL SECURITY;
ALTER TABLE vpn_write_operations ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON vpn_create_authorities,vpn_write_operations FROM PUBLIC;
CREATE FUNCTION api_vpn_create_authority(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;c uuid;
BEGIN s:=session_actor(token);
 SELECT id INTO c FROM vpn_create_authorities WHERE team_id=s.team_id AND account_id=s.account_id AND device_id=s.device_id AND auth_epoch=s.auth_epoch;
 IF c IS NULL THEN INSERT INTO vpn_create_authorities(team_id,account_id,device_id,auth_epoch) VALUES(s.team_id,s.account_id,s.device_id,s.auth_epoch) RETURNING id INTO c;END IF;
 RETURN jsonb_build_object('authority',c);
END $$;
CREATE FUNCTION vpn_write_json(o vpn_write_operations) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('found',true,'operationId',o.id,'status',o.status,'revision',o.result_revision,'authId',o.auth_id,'authRevision',o.auth_revision)
$$;
CREATE FUNCTION api_vpn_write_receipt(token text,op uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;o vpn_write_operations;
BEGIN s:=session_actor(token);SELECT * INTO o FROM vpn_write_operations WHERE team_id=s.team_id AND id=op AND actor_id=s.account_id AND device_id=s.device_id;
 IF o.id IS NULL THEN RETURN '{"found":false}';END IF;RETURN vpn_write_json(o);END $$;
CREATE FUNCTION api_vpn_write_prepare(token text,p jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;o vpn_write_operations;c credentials;op uuid;v uuid;rev bigint;hash bytea:=public.digest(p::text,'sha256');k uuid;st text:='prepared';
BEGIN s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 IF NOT p ?& ARRAY['operationId','deviceId','sequence','vpnId','baseRevision','creationAuthority','config','auth','authId','authBase','sharedPlan'] OR jsonb_typeof(p->'config') NOT IN ('object','null') OR jsonb_typeof(p->'auth') NOT IN ('object','null')
 OR p-'operationId'-'deviceId'-'sequence'-'vpnId'-'baseRevision'-'creationAuthority'-'config'-'auth'-'authId'-'authBase'-'sharedPlan'<>'{}'::jsonb
 OR octet_length(p::text)>3145728 OR (p->>'deviceId')::uuid IS DISTINCT FROM s.device_id OR (p->>'sequence')::bigint IS NULL OR (p->>'sequence')::bigint<=0
 OR (p->'config'='null'::jsonb AND p->'auth'='null'::jsonb)
 THEN RAISE EXCEPTION 'invalid_vpn_operation' USING ERRCODE='22023';END IF;
 op:=(p->>'operationId')::uuid;v:=(p->>'vpnId')::uuid;rev:=(p->>'baseRevision')::bigint;
 IF op IS NULL OR v IS NULL OR rev IS NULL OR rev<0 THEN RAISE EXCEPTION 'invalid_vpn_operation' USING ERRCODE='22023';END IF;
 SELECT * INTO o FROM vpn_write_operations WHERE team_id=s.team_id AND id=op;
 IF o.id IS NOT NULL THEN
  IF o.actor_id<>s.account_id OR o.device_id<>s.device_id OR o.request_hash<>hash THEN RAISE EXCEPTION 'operation_mismatch' USING ERRCODE='22023';END IF;RETURN vpn_write_json(o);
 END IF;
 IF (SELECT count(*) FROM vpn_write_operations WHERE team_id=s.team_id AND actor_id=s.account_id AND status='prepared')>=100 THEN RAISE EXCEPTION 'too_many_operations' USING ERRCODE='22023';END IF;
 SELECT * INTO c FROM credentials WHERE team_id=s.team_id AND id=v;
 IF c.id IS NULL THEN
  IF rev<>0 OR p->'config'='null'::jsonb OR NOT EXISTS(SELECT FROM vpn_create_authorities WHERE team_id=s.team_id AND id=(p->>'creationAuthority')::uuid AND account_id=s.account_id AND device_id=s.device_id AND auth_epoch=s.auth_epoch)
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501';END IF;
 ELSE
  IF c.kind<>'vpn' OR c.deleted OR (c.visibility='private' AND c.owner_account_id<>s.account_id) OR (c.visibility='shared' AND ((p->'config'<>'null'::jsonb AND NOT shared_update_allowed(s.team_id,s.account_id,v)) OR (p->'auth'<>'null'::jsonb AND NOT vpn_allowed(s.team_id,s.account_id,v))))
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501';END IF;
  IF c.revision<>rev THEN st:='conflict';END IF;
 END IF;
 SELECT id INTO k FROM team_recovery_keys WHERE team_id=s.team_id AND active;
 IF k IS NULL THEN RAISE EXCEPTION 'recovery_key_required' USING ERRCODE='22023';END IF;
 IF p->'auth'<>'null'::jsonb AND ((p->>'authId') IS NULL OR (p->>'authBase') IS NULL OR coalesce((SELECT credential_id FROM vpn_auth_bindings WHERE team_id=s.team_id AND account_id=s.account_id AND vpn_id=v),(p->>'authId')::uuid)<>(p->>'authId')::uuid
 OR coalesce((SELECT revision FROM credentials WHERE team_id=s.team_id AND id=(p->>'authId')::uuid),0)<>(p->>'authBase')::bigint) THEN st:='conflict';END IF;
 INSERT INTO vpn_write_operations(team_id,id,actor_id,device_id,sequence,vpn_id,base_revision,acl_revision,recovery_id,request_hash,body,status)
 SELECT s.team_id,op,s.account_id,s.device_id,(p->>'sequence')::bigint,v,rev,t.acl_revision,k,hash,p,st FROM teams t WHERE t.id=s.team_id RETURNING * INTO o;
 RETURN vpn_write_json(o);
END $$;
CREATE FUNCTION vpn_private_payload(token text,c uuid,expected bigint,kind_input text,p jsonb) RETURNS jsonb
LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
BEGIN
 IF p-'expectedRevision'-'kind'-'encryptedData'-'encryptedSecret'-'identityVersion'-'recoveryKeyId'-'userData'-'userSecret'-'recoveryData'-'recoverySecret'<>'{}'::jsonb
 OR p->>'kind' IS DISTINCT FROM kind_input OR (p->>'expectedRevision')::bigint IS DISTINCT FROM expected
 THEN RAISE EXCEPTION 'invalid_vpn_payload' USING ERRCODE='22023';END IF;
 RETURN api_private_credential_put(token,c,expected,kind_input,decode(p->>'encryptedData','hex'),decode(p->>'encryptedSecret','hex'),(p->>'identityVersion')::bigint,(p->>'recoveryKeyId')::uuid,decode(p->>'userData','hex'),decode(p->>'userSecret','hex'),decode(p->>'recoveryData','hex'),decode(p->>'recoverySecret','hex'));
END $$;
CREATE FUNCTION api_vpn_write_apply(token text,op uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;o vpn_write_operations;c credentials;p jsonb;receipt jsonb;rev bigint;ar bigint;aid uuid;
BEGIN s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 SELECT * INTO o FROM vpn_write_operations WHERE team_id=s.team_id AND id=op AND actor_id=s.account_id AND device_id=s.device_id FOR UPDATE;
 IF o.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002';END IF;
 IF o.status<>'prepared' THEN RETURN vpn_write_json(o);END IF;
 p:=o.body;SELECT * INTO c FROM credentials WHERE team_id=s.team_id AND id=o.vpn_id;
 IF (c.id IS NOT NULL AND (c.deleted OR c.kind<>'vpn' OR (c.visibility='private' AND c.owner_account_id<>s.account_id)
 OR (c.visibility='shared' AND ((p->'config'<>'null'::jsonb AND NOT shared_update_allowed(s.team_id,s.account_id,c.id)) OR (p->'auth'<>'null'::jsonb AND NOT vpn_allowed(s.team_id,s.account_id,c.id))))))
 THEN UPDATE vpn_write_operations SET status='quarantined' WHERE team_id=s.team_id AND id=op RETURNING * INTO o;RETURN vpn_write_json(o);END IF;
 IF (p->'auth'<>'null'::jsonb AND (coalesce((SELECT credential_id FROM vpn_auth_bindings WHERE team_id=s.team_id AND account_id=s.account_id AND vpn_id=o.vpn_id),(p->>'authId')::uuid) IS DISTINCT FROM (p->>'authId')::uuid OR coalesce((SELECT revision FROM credentials WHERE team_id=s.team_id AND id=(p->>'authId')::uuid),0) IS DISTINCT FROM (p->>'authBase')::bigint))
 OR coalesce(c.revision,0)<>o.base_revision OR o.acl_revision<>(SELECT acl_revision FROM teams WHERE id=s.team_id) OR NOT EXISTS(SELECT FROM team_recovery_keys WHERE team_id=s.team_id AND id=o.recovery_id AND active)
 THEN UPDATE vpn_write_operations SET status='conflict' WHERE team_id=s.team_id AND id=op RETURNING * INTO o;RETURN vpn_write_json(o);END IF;
 BEGIN
  rev:=o.base_revision;
  IF p->'config'<>'null'::jsonb THEN
   IF c.visibility='shared' THEN
    IF NOT EXISTS(SELECT FROM credential_update_plans WHERE team_id=s.team_id AND id=(p->>'sharedPlan')::uuid AND actor_id=s.account_id AND credential_id=o.vpn_id AND expected_revision=o.base_revision) THEN RAISE EXCEPTION 'invalid_vpn_plan' USING ERRCODE='22023';END IF;
    receipt:=api_shared_update_finish(token,(p->>'sharedPlan')::uuid,p->'config');
   ELSE
    IF c.id IS NULL AND NOT EXISTS(SELECT FROM vpn_create_authorities WHERE team_id=s.team_id AND id=(p->>'creationAuthority')::uuid AND account_id=s.account_id AND device_id=s.device_id AND auth_epoch=s.auth_epoch) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501';END IF;
    receipt:=vpn_private_payload(token,o.vpn_id,o.base_revision,'vpn',p->'config');
   END IF;rev:=(receipt->>'revision')::bigint;
  END IF;
  IF p->'auth'<>'null'::jsonb THEN
   aid:=(p->>'authId')::uuid;
   receipt:=vpn_private_payload(token,aid,(p->>'authBase')::bigint,'vpn_auth',p->'auth');ar:=(receipt->>'revision')::bigint;
   PERFORM api_vpn_auth(token,o.vpn_id,aid,true);
  END IF;
  UPDATE vpn_write_operations SET status='applied',result_revision=rev,auth_id=aid,auth_revision=ar WHERE team_id=s.team_id AND id=op RETURNING * INTO o;
 EXCEPTION WHEN serialization_failure OR invalid_parameter_value THEN UPDATE vpn_write_operations SET status='conflict' WHERE team_id=s.team_id AND id=op RETURNING * INTO o;
 WHEN insufficient_privilege THEN UPDATE vpn_write_operations SET status='quarantined' WHERE team_id=s.team_id AND id=op RETURNING * INTO o;
 END;
 RETURN vpn_write_json(o);
END $$;

REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(66);
COMMIT;
