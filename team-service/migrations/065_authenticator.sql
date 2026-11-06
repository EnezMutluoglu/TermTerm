BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- TOTP uses credential envelopes/recovery, with code-use separate from seed reveal/export.
ALTER TABLE credentials DROP CONSTRAINT credentials_kind_check;
ALTER TABLE credentials ADD CONSTRAINT credentials_kind_check CHECK(kind IN ('ssh_key','password','agent','hardware','proxy','totp'));
CREATE FUNCTION authenticator_kind_guard() RETURNS trigger LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
BEGIN
 IF (OLD.kind='totp')<>(NEW.kind='totp') THEN RAISE EXCEPTION 'credential_kind_conflict' USING ERRCODE='22023'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER authenticator_kind_immutable BEFORE UPDATE OF kind ON credentials FOR EACH ROW EXECUTE FUNCTION authenticator_kind_guard();
REVOKE ALL ON FUNCTION authenticator_kind_guard() FROM PUBLIC;
CREATE OR REPLACE FUNCTION api_share_prepare(token text,source uuid,scope uuid,recipients jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p credential_share_plans;c credentials;r jsonb;k uuid;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 SELECT * INTO c FROM credentials WHERE team_id=s.team_id AND id=source AND visibility='private' AND owner_account_id=s.account_id AND NOT deleted;
 IF c.id IS NULL OR c.kind NOT IN ('ssh_key','password','totp') OR NOT allowed(s.team_id,s.account_id,scope,'edit')
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
CREATE OR REPLACE FUNCTION api_shared_credential_get(token text,c uuid,purpose text,host uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;row credentials;scope uuid;answer jsonb;
BEGIN
 s:=session_actor(token);
 SELECT * INTO row FROM credentials WHERE team_id=s.team_id AND id=c AND visibility='shared' AND NOT deleted;
 SELECT scope_id INTO scope FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c;
 IF row.id IS NULL OR purpose NOT IN ('connect','reveal','export','otp') OR purpose IS NULL OR NOT allowed(s.team_id,s.account_id,row.resource_id,CASE WHEN purpose='otp' THEN 'connect' ELSE purpose END) OR NOT allowed(s.team_id,s.account_id,scope,'connect')
 OR (purpose='otp' AND row.kind<>'totp') OR (purpose='connect' AND row.kind='totp') OR (purpose='connect' AND (host IS NULL OR NOT allowed(s.team_id,s.account_id,host,'connect') OR NOT within_scope(s.team_id,host,scope)))
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT jsonb_build_object('id',row.id,'revision',row.revision,'kind',row.kind,'encryptedData',encode(v.encrypted_data,'hex'),
 'encryptedSecret',encode(v.encrypted_secret,'hex'),'envelopes',(SELECT jsonb_agg(jsonb_build_object('purpose',e.purpose,'envelope',encode(envelope,'hex'),'identityVersion',identity_version))
 FROM credential_key_envelopes e WHERE team_id=s.team_id AND credential_id=c AND revision=v.revision AND account_id=s.account_id)) INTO answer
 FROM credential_versions v WHERE v.team_id=s.team_id AND v.credential_id=c AND v.revision=row.revision;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'credential.'||purpose,row.resource_id,'succeeded');
 RETURN answer;
END $$;
CREATE OR REPLACE FUNCTION credential_usable(t uuid,a uuid,c uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT FROM credentials WHERE team_id=t AND id=c AND NOT deleted AND kind<>'totp' AND
 ((visibility='private' AND owner_account_id=a) OR (visibility='shared' AND allowed(t,a,resource_id,'connect') AND EXISTS(
 SELECT FROM shared_credential_scopes sc WHERE sc.team_id=t AND sc.credential_id=c AND allowed(t,a,sc.scope_id,'connect')))))
$$;
CREATE FUNCTION api_authenticator_remove(token text,c uuid,expected bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;row credentials;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 SELECT * INTO row FROM credentials WHERE team_id=s.team_id AND id=c AND kind='totp' AND visibility='private' AND owner_account_id=s.account_id FOR UPDATE;
 IF row.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF row.revision IS DISTINCT FROM expected THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
 UPDATE credentials SET deleted=true WHERE team_id=s.team_id AND id=c;
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'authenticator.remove',s.account_id,'succeeded');
 RETURN '{"status":"removed"}';
END $$;
REVOKE ALL ON FUNCTION api_authenticator_remove(text,uuid,bigint) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(65);
COMMIT;
