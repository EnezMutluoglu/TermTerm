BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION guard_graph() RETURNS trigger LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE cyclic boolean;
BEGIN
 -- Serialize graph edits within a team so concurrent reciprocal inserts cannot escape detection.
 PERFORM pg_advisory_xact_lock(hashtextextended(NEW.team_id::text,417));
 IF TG_TABLE_NAME='folders' THEN
  WITH RECURSIVE parents(id,depth) AS (
   SELECT NEW.parent_id,1 UNION ALL
   SELECT f.parent_id,p.depth+1 FROM folders f JOIN parents p ON f.id=p.id WHERE f.team_id=NEW.team_id AND p.depth<=64
  ) SELECT coalesce(bool_or(id=NEW.id OR depth>64),false) INTO cyclic FROM parents;
 ELSE
  WITH RECURSIVE hops(id,path) AS (
   SELECT NEW.jump_host_id,ARRAY[NEW.host_id,NEW.jump_host_id] UNION ALL
   SELECT h.jump_host_id,p.path||h.jump_host_id FROM host_hops h JOIN hops p ON h.host_id=p.id
   WHERE h.team_id=NEW.team_id AND cardinality(p.path)<=17 AND NOT p.id=ANY(p.path[1:cardinality(p.path)-1])
  ) SELECT coalesce(bool_or(id=NEW.host_id OR cardinality(path)>17 OR id=ANY(path[1:cardinality(path)-1])),false) INTO cyclic FROM hops;
 END IF;
 IF cyclic THEN RAISE EXCEPTION 'graph_cycle_or_depth_limit' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER folder_graph BEFORE INSERT OR UPDATE ON folders FOR EACH ROW EXECUTE FUNCTION guard_graph();
CREATE TRIGGER host_graph BEFORE INSERT OR UPDATE ON host_hops FOR EACH ROW EXECUTE FUNCTION guard_graph();
CREATE FUNCTION resource_kind_guard() RETURNS trigger LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE k text;
BEGIN
 SELECT kind INTO k FROM resources WHERE team_id=NEW.team_id AND id=NEW.id;
 IF k IS DISTINCT FROM TG_ARGV[0] THEN RAISE EXCEPTION 'resource_kind_mismatch' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER folder_kind BEFORE INSERT OR UPDATE ON folders FOR EACH ROW EXECUTE FUNCTION resource_kind_guard('folder');
CREATE TRIGGER host_kind BEFORE INSERT OR UPDATE ON hosts FOR EACH ROW EXECUTE FUNCTION resource_kind_guard('host');
CREATE FUNCTION allowed(t uuid,a uuid,r uuid,p text) RETURNS boolean LANGUAGE sql STABLE AS $$
 WITH RECURSIVE ancestry(id,depth) AS (
  SELECT r,0 UNION ALL
  SELECT coalesce(f.parent_id,h.folder_id),x.depth+1 FROM ancestry x
  LEFT JOIN folders f ON f.team_id=t AND f.id=x.id LEFT JOIN hosts h ON h.team_id=t AND h.id=x.id
  WHERE x.depth<64 AND coalesce(f.parent_id,h.folder_id) IS NOT NULL
 ), rules AS (
  SELECT effect FROM resource_acl acl JOIN ancestry x ON x.id=acl.resource_id
  WHERE acl.team_id=t AND acl.account_id=a AND acl.permission=p
 ) SELECT EXISTS(SELECT FROM team_accounts WHERE team_id=t AND id=a AND status='active')
 AND EXISTS(SELECT FROM resources WHERE team_id=t AND id=r AND NOT deleted)
 AND (EXISTS(SELECT FROM team_memberships WHERE team_id=t AND account_id=a AND role='owner')
 OR (NOT EXISTS(SELECT FROM rules WHERE effect='deny') AND EXISTS(SELECT FROM rules WHERE effect='allow')))
$$;
CREATE FUNCTION credential_usable(t uuid,a uuid,c uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT FROM credentials WHERE team_id=t AND id=c AND NOT deleted AND
 ((visibility='private' AND owner_account_id=a) OR (visibility='shared' AND allowed(t,a,resource_id,'connect'))))
$$;
CREATE FUNCTION profile_guard() RETURNS trigger LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
BEGIN
 IF NEW.credential_id IS NOT NULL AND NOT credential_usable(NEW.team_id,NEW.account_id,NEW.credential_id) THEN
  RAISE EXCEPTION 'credential_not_permitted' USING ERRCODE='42501'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER defaults_ownership BEFORE INSERT OR UPDATE ON user_connection_defaults FOR EACH ROW EXECUTE FUNCTION profile_guard();
CREATE TRIGGER profile_ownership BEFORE INSERT OR UPDATE ON user_host_profiles FOR EACH ROW EXECUTE FUNCTION profile_guard();
CREATE FUNCTION reauthenticate(s auth_sessions,p text) RETURNS boolean LANGUAGE plpgsql AS $$
DECLARE k bytea:=public.digest(s.team_id::text||':'||s.account_id::text||':reauth','sha256'); rate auth_attempts; ok boolean;
BEGIN
 INSERT INTO auth_attempts VALUES(k,clock_timestamp(),0) ON CONFLICT DO NOTHING;
 SELECT * INTO rate FROM auth_attempts WHERE key_hash=k FOR UPDATE;
 IF rate.window_at<clock_timestamp()-interval '15 minutes' THEN
  UPDATE auth_attempts SET failures=0,window_at=clock_timestamp() WHERE key_hash=k;rate.failures:=0;
 END IF;
 IF rate.failures>=5 THEN RETURN false; END IF;
 SELECT valid_password(p) AND public.crypt(p,password_hash)=password_hash INTO ok FROM team_accounts WHERE team_id=s.team_id AND id=s.account_id;
 UPDATE auth_attempts SET failures=CASE WHEN coalesce(ok,false) THEN 0 ELSE failures+1 END WHERE key_hash=k;
 RETURN coalesce(ok,false);
END $$;
CREATE FUNCTION api_recovery_key_set(token text,p text,pub bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; k uuid;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 IF NOT reauthenticate(s,p) THEN RETURN '{"error":"invalid_credentials"}'; END IF;
 IF octet_length(pub)<>32 THEN RAISE EXCEPTION 'invalid_key' USING ERRCODE='22023'; END IF;
 -- Initial enrollment only. Rotation must rewrap every live version before retiring the old key.
 INSERT INTO team_recovery_keys(team_id,public_key) VALUES(s.team_id,pub) RETURNING id INTO k;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'recovery.enroll','succeeded');
 RETURN jsonb_build_object('keyId',k);
END $$;
CREATE FUNCTION api_crypto_context(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN jsonb_build_object('identity',(SELECT jsonb_build_object('version',version,'publicKey',encode(public_key,'hex')) FROM account_crypto_identities
 WHERE team_id=s.team_id AND account_id=s.account_id AND active),
 'recovery',(SELECT jsonb_build_object('keyId',id,'publicKey',encode(public_key,'hex')) FROM team_recovery_keys WHERE team_id=s.team_id AND active));
END $$;
CREATE FUNCTION api_private_credential_put(token text,c uuid,expected bigint,kind_input text,data bytea,secret bytea,
 identity_ver bigint,recovery_id uuid,user_data bytea,user_secret bytea,recovery_data bytea,recovery_secret bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; old credentials; rev bigint:=expected+1;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 SELECT * INTO old FROM credentials WHERE team_id=s.team_id AND id=c FOR UPDATE;
 IF old.id IS NOT NULL AND (old.visibility<>'private' OR old.owner_account_id<>s.account_id OR old.deleted) THEN
  RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF expected<0 OR (old.id IS NULL AND expected<>0) OR (old.id IS NOT NULL AND old.revision<>expected) THEN
  RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
 IF NOT EXISTS(SELECT FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND version=identity_ver AND active)
 OR NOT EXISTS(SELECT FROM team_recovery_keys WHERE team_id=s.team_id AND id=recovery_id AND active) THEN
  RAISE EXCEPTION 'crypto_context_changed' USING ERRCODE='40001'; END IF;
 IF old.id IS NULL THEN INSERT INTO credentials(team_id,id,visibility,owner_account_id,kind) VALUES(s.team_id,c,'private',s.account_id,kind_input);
 ELSE UPDATE credentials SET revision=rev,kind=kind_input WHERE team_id=s.team_id AND id=c; END IF;
 INSERT INTO credential_versions(team_id,credential_id,revision,encrypted_data,encrypted_secret,actor_id) VALUES(s.team_id,c,rev,data,secret,s.account_id);
 INSERT INTO credential_key_envelopes(team_id,credential_id,revision,account_id,identity_version,purpose,envelope) VALUES
 (s.team_id,c,rev,s.account_id,identity_ver,'data',user_data),(s.team_id,c,rev,s.account_id,identity_ver,'secret',user_secret);
 INSERT INTO credential_key_envelopes(team_id,credential_id,revision,recovery_key_id,purpose,envelope) VALUES
 (s.team_id,c,rev,recovery_id,'data',recovery_data),(s.team_id,c,rev,recovery_id,'secret',recovery_secret);
 DELETE FROM credential_versions WHERE team_id=s.team_id AND credential_id=c AND revision<rev-10;
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'credential.private_save',s.account_id,'succeeded');
 RETURN jsonb_build_object('credentialId',c,'revision',rev);
END $$;
CREATE FUNCTION api_private_credentials(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',c.id,'kind',c.kind,'revision',c.revision,
 'encryptedData',encode(v.encrypted_data,'hex'),'dataEnvelope',encode(e.envelope,'hex'),'identityVersion',e.identity_version) ORDER BY c.id)
 FROM credentials c JOIN credential_versions v ON (v.team_id,v.credential_id,v.revision)=(c.team_id,c.id,c.revision)
 LEFT JOIN credential_key_envelopes e ON (e.team_id,e.credential_id,e.revision,e.account_id,e.purpose)=(c.team_id,c.id,c.revision,s.account_id,'data')
 AND e.identity_version=(SELECT version FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND active)
 WHERE c.team_id=s.team_id AND c.owner_account_id=s.account_id AND c.visibility='private' AND NOT c.deleted),'[]');
END $$;
CREATE FUNCTION api_private_credential_get(token text,c uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; answer jsonb;
BEGIN
 s:=session_actor(token);
 SELECT jsonb_build_object('id',c1.id,'revision',c1.revision,'kind',c1.kind,'encryptedData',encode(v.encrypted_data,'hex'),
 'encryptedSecret',encode(v.encrypted_secret,'hex'),'envelopes',(SELECT jsonb_agg(jsonb_build_object('purpose',purpose,'envelope',encode(envelope,'hex'),'identityVersion',identity_version))
 FROM credential_key_envelopes WHERE team_id=s.team_id AND credential_id=c AND revision=v.revision AND account_id=s.account_id)) INTO answer
 FROM credentials c1 JOIN credential_versions v ON (v.team_id,v.credential_id,v.revision)=(c1.team_id,c1.id,c1.revision)
 WHERE c1.team_id=s.team_id AND c1.id=c AND c1.owner_account_id=s.account_id AND c1.visibility='private' AND NOT c1.deleted;
 IF answer IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 RETURN answer;
END $$;
CREATE FUNCTION api_profile_put(token text,host uuid,expected bigint,method_input text,credential uuid,username bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; old_rev bigint;
BEGIN
 s:=session_actor(token); PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 IF username IS NOT NULL AND octet_length(username) NOT BETWEEN 40 AND 4096 THEN RAISE EXCEPTION 'invalid_username' USING ERRCODE='22023'; END IF;
 IF host IS NOT NULL THEN
  IF NOT allowed(s.team_id,s.account_id,host,'connect') THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  IF username IS NOT NULL AND EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=host AND username_policy='shared_required')
  THEN RAISE EXCEPTION 'shared_username_locked' USING ERRCODE='42501'; END IF;
  SELECT revision INTO old_rev FROM user_host_profiles WHERE team_id=s.team_id AND account_id=s.account_id AND host_id=host FOR UPDATE;
 ELSE SELECT revision INTO old_rev FROM user_connection_defaults WHERE team_id=s.team_id AND account_id=s.account_id FOR UPDATE;
 END IF;
 IF expected<0 OR coalesce(old_rev,0)<>expected THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
 IF host IS NULL THEN
  INSERT INTO user_connection_defaults VALUES(s.team_id,s.account_id,credential,method_input,username,expected+1)
  ON CONFLICT(team_id,account_id) DO UPDATE SET credential_id=excluded.credential_id,method=excluded.method,encrypted_username=excluded.encrypted_username,revision=excluded.revision;
 ELSE
  INSERT INTO user_host_profiles VALUES(s.team_id,s.account_id,host,credential,method_input,username,expected+1)
  ON CONFLICT(team_id,account_id,host_id) DO UPDATE SET credential_id=excluded.credential_id,method=excluded.method,encrypted_username=excluded.encrypted_username,revision=excluded.revision;
 END IF;
 RETURN jsonb_build_object('revision',expected+1);
END $$;
CREATE FUNCTION api_profiles(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN jsonb_build_object('default',(SELECT jsonb_build_object('credentialId',credential_id,'method',method,'encryptedUsername',encode(encrypted_username,'hex'),'revision',revision)
 FROM user_connection_defaults WHERE team_id=s.team_id AND account_id=s.account_id),
 'hosts',coalesce((SELECT jsonb_agg(jsonb_build_object('hostId',host_id,'credentialId',credential_id,'method',method,'encryptedUsername',encode(encrypted_username,'hex'),'revision',revision))
 FROM user_host_profiles WHERE team_id=s.team_id AND account_id=s.account_id AND allowed(s.team_id,s.account_id,host_id,'connect')),'[]'));
END $$;
CREATE FUNCTION api_recovery_begin(token text,p text,subject uuid,key_id uuid,why text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; op uuid;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 IF NOT reauthenticate(s,p) THEN RETURN '{"error":"invalid_credentials"}'; END IF;
 IF NOT EXISTS(SELECT FROM team_accounts WHERE team_id=s.team_id AND id=subject AND status='active') THEN
  RAISE EXCEPTION 'activate_account_first' USING ERRCODE='22023'; END IF;
 INSERT INTO recovery_operations(team_id,actor_id,subject_id,recovery_key_id,reason,status,expires_at)
 VALUES(s.team_id,s.account_id,subject,key_id,why,'authorized',clock_timestamp()+interval '10 minutes') RETURNING id INTO op;
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'recovery.authorize',subject,'authorized');
 RETURN jsonb_build_object('operationId',op,'expiresInSeconds',600);
END $$;
CREATE FUNCTION api_recovery_credential(token text,operation uuid,credential uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; op recovery_operations; answer jsonb;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 SELECT * INTO op FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND actor_id=s.account_id AND status='authorized' AND expires_at>clock_timestamp();
 IF op.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=credential AND visibility='private' AND owner_account_id=op.subject_id AND NOT deleted)
 THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'recovery.read_envelopes',op.subject_id,'authorized');
 SELECT jsonb_build_object('credentialId',credential,'versions',jsonb_agg(jsonb_build_object('revision',v.revision,
 'encryptedData',encode(v.encrypted_data,'hex'),'encryptedSecret',encode(v.encrypted_secret,'hex'),
 'envelopes',(SELECT jsonb_agg(jsonb_build_object('purpose',purpose,'envelope',encode(envelope,'hex'))) FROM credential_key_envelopes e
 WHERE (e.team_id,e.credential_id,e.revision)=(v.team_id,v.credential_id,v.revision) AND e.recovery_key_id=op.recovery_key_id)))) INTO answer
 FROM credential_versions v WHERE v.team_id=s.team_id AND v.credential_id=credential;
 RETURN answer;
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(3,now());
COMMIT;
