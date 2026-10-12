BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE TABLE profile_versions (
 team_id uuid NOT NULL, account_id uuid NOT NULL, host_key text NOT NULL,
 revision bigint NOT NULL CHECK(revision>0), encrypted_username bytea NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,account_id,host_key,revision),
 FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id),
 CHECK(host_key='default' OR host_key::uuid IS NOT NULL)
);
CREATE TABLE profile_user_envelopes (
 team_id uuid NOT NULL, account_id uuid NOT NULL, host_key text NOT NULL, revision bigint NOT NULL,
 identity_version bigint NOT NULL, envelope bytea NOT NULL CHECK(octet_length(envelope) BETWEEN 80 AND 4096),
 PRIMARY KEY(team_id,account_id,host_key,revision,identity_version),
 FOREIGN KEY(team_id,account_id,host_key,revision) REFERENCES profile_versions ON DELETE CASCADE,
 FOREIGN KEY(team_id,account_id,identity_version) REFERENCES account_crypto_identities(team_id,account_id,version)
);
CREATE TABLE profile_recovery_envelopes (
 team_id uuid NOT NULL, account_id uuid NOT NULL, host_key text NOT NULL, revision bigint NOT NULL,
 recovery_key_id uuid NOT NULL, envelope bytea NOT NULL CHECK(octet_length(envelope) BETWEEN 80 AND 4096),
 PRIMARY KEY(team_id,account_id,host_key,revision,recovery_key_id),
 FOREIGN KEY(team_id,account_id,host_key,revision) REFERENCES profile_versions ON DELETE CASCADE,
 FOREIGN KEY(team_id,recovery_key_id) REFERENCES team_recovery_keys(team_id,id)
);
CREATE INDEX profile_recovery_key ON profile_recovery_envelopes(team_id,recovery_key_id,account_id,host_key,revision);
ALTER TABLE profile_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE profile_user_envelopes ENABLE ROW LEVEL SECURITY;
ALTER TABLE profile_recovery_envelopes ENABLE ROW LEVEL SECURITY;
ALTER FUNCTION api_profile_put(text,uuid,bigint,text,uuid,bytea) RENAME TO profile_put_internal;
CREATE FUNCTION api_profile_put(token text,host uuid,expected bigint,method_input text,credential uuid,username bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
BEGIN
 IF username IS NOT NULL THEN RAISE EXCEPTION 'profile_recovery_required' USING ERRCODE='22023'; END IF;
 RETURN profile_put_internal(token,host,expected,method_input,credential,NULL);
END $$;
CREATE FUNCTION api_profile_protected_put(token text,host uuid,expected bigint,method_input text,credential uuid,username bytea,identity_version_input bigint,key_id uuid,user_envelope bytea,recovery_envelope bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; h text:=coalesce(host::text,'default'); answer jsonb;
BEGIN
 s:=session_actor(token); PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 IF username IS NULL OR NOT EXISTS(SELECT FROM team_recovery_keys WHERE team_id=s.team_id AND id=key_id AND active)
 OR NOT EXISTS(SELECT FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND version=identity_version_input AND active)
 THEN RAISE EXCEPTION 'crypto_context_changed' USING ERRCODE='40001'; END IF;
 answer:=profile_put_internal(token,host,expected,method_input,credential,username);
 INSERT INTO profile_versions VALUES(s.team_id,s.account_id,h,expected+1,username,now());
 INSERT INTO profile_user_envelopes VALUES(s.team_id,s.account_id,h,expected+1,identity_version_input,user_envelope);
 INSERT INTO profile_recovery_envelopes VALUES(s.team_id,s.account_id,h,expected+1,key_id,recovery_envelope);
 DELETE FROM profile_versions WHERE team_id=s.team_id AND account_id=s.account_id AND host_key=h
 AND revision NOT IN(SELECT revision FROM profile_versions WHERE team_id=s.team_id AND account_id=s.account_id AND host_key=h ORDER BY revision DESC LIMIT 11);
 RETURN answer;
END $$;
ALTER FUNCTION api_profiles(text) RENAME TO profiles_internal;
CREATE FUNCTION api_profiles(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; rows jsonb; item jsonb; result jsonb:='[]'; env bytea;
BEGIN
 s:=session_actor(token); rows:=profiles_internal(token);
 FOR item IN SELECT value FROM jsonb_array_elements((rows->'hosts') || CASE WHEN rows->'default'='null' THEN '[]'::jsonb ELSE jsonb_build_array(rows->'default') END) LOOP
  SELECT e.envelope INTO env FROM profile_user_envelopes e JOIN account_crypto_identities i ON
   (i.team_id,i.account_id,i.version)=(e.team_id,e.account_id,e.identity_version) AND i.active
   WHERE e.team_id=s.team_id AND e.account_id=s.account_id AND e.host_key=coalesce(item->>'hostId','default') AND e.revision=(item->>'revision')::bigint;
  item:=item||jsonb_build_object('userEnvelope',encode(env,'hex'));
  IF item->>'hostId' IS NULL THEN rows:=jsonb_set(rows,'{default}',item); ELSE result:=result||jsonb_build_array(item); END IF;
 END LOOP;
 RETURN jsonb_set(rows,'{hosts}',result);
END $$;
ALTER FUNCTION api_recovery_manifest(text,uuid) RENAME TO recovery_manifest_internal;
CREATE FUNCTION api_recovery_manifest(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; m jsonb;
BEGIN
 s:=session_actor(token); m:=recovery_manifest_internal(token,operation);
 RETURN m||jsonb_build_object('profiles',coalesce((SELECT jsonb_agg(jsonb_build_object('hostKey',host_key,'revision',revision) ORDER BY host_key,revision)
 FROM profile_versions WHERE team_id=s.team_id AND account_id=(m->>'subjectId')::uuid),'[]'));
END $$;
CREATE FUNCTION api_recovery_profile(token text,operation uuid,host_input text,rev bigint,verification boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; op recovery_operations; answer jsonb;
BEGIN
 s:=session_actor(token);
 SELECT * INTO op FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND status='authorized';
 IF op.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF verification THEN
  IF op.subject_id<>s.account_id THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 ELSE
  PERFORM require_owner(s);
  IF op.actor_id<>s.account_id OR op.expires_at<=clock_timestamp() THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'recovery.profile_envelopes',op.subject_id,'authorized');
 END IF;
 SELECT jsonb_build_object('encryptedUsername',encode(v.encrypted_username,'hex'),'envelope',
 CASE WHEN verification THEN (SELECT encode(e.envelope,'hex') FROM profile_user_envelopes e JOIN account_crypto_identities i ON
 (i.team_id,i.account_id,i.version)=(e.team_id,e.account_id,e.identity_version) AND i.active
 WHERE (e.team_id,e.account_id,e.host_key,e.revision)=(v.team_id,v.account_id,v.host_key,v.revision))
 ELSE (SELECT encode(e.envelope,'hex') FROM profile_recovery_envelopes e WHERE
 (e.team_id,e.account_id,e.host_key,e.revision,e.recovery_key_id)=(v.team_id,v.account_id,v.host_key,v.revision,op.recovery_key_id)) END) INTO answer
 FROM profile_versions v WHERE (v.team_id,v.account_id,v.host_key,v.revision)=(s.team_id,op.subject_id,host_input,rev);
 IF answer IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 RETURN answer;
END $$;
CREATE FUNCTION api_recovery_profile_rewrap(token text,operation uuid,host_input text,rev bigint,identity_version_input bigint,sealed bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; op recovery_operations; old bytea;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 SELECT * INTO op FROM recovery_operations WHERE team_id=s.team_id AND id=operation AND actor_id=s.account_id AND status='authorized' AND expires_at>clock_timestamp();
 IF op.id IS NULL OR NOT EXISTS(SELECT FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=op.subject_id AND version=identity_version_input AND active)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 PERFORM api_recovery_profile(token,operation,host_input,rev,false);
 SELECT envelope INTO old FROM profile_user_envelopes WHERE (team_id,account_id,host_key,revision,identity_version)=(s.team_id,op.subject_id,host_input,rev,identity_version_input);
 -- A resumed client produces a fresh randomized envelope for the same key.
 -- Preserve the first accepted envelope; the recipient must still verify AEAD.
 INSERT INTO profile_user_envelopes VALUES(s.team_id,op.subject_id,host_input,rev,identity_version_input,sealed) ON CONFLICT DO NOTHING;
 RETURN '{"rewrapped":true}';
END $$;
ALTER FUNCTION api_recovery_complete(text,uuid) RENAME TO recovery_complete_internal;
CREATE FUNCTION api_recovery_complete(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token); PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 IF EXISTS(SELECT FROM profile_versions v WHERE v.team_id=s.team_id AND v.account_id=s.account_id AND NOT EXISTS(
 SELECT FROM profile_user_envelopes e JOIN account_crypto_identities i ON (i.team_id,i.account_id,i.version)=(e.team_id,e.account_id,e.identity_version) AND i.active
 WHERE (e.team_id,e.account_id,e.host_key,e.revision)=(v.team_id,v.account_id,v.host_key,v.revision)))
 THEN RAISE EXCEPTION 'recovery_incomplete' USING ERRCODE='23514'; END IF;
 RETURN recovery_complete_internal(token,operation);
END $$;
REVOKE ALL ON FUNCTION profile_put_internal(text,uuid,bigint,text,uuid,bytea),profiles_internal(text),recovery_manifest_internal(text,uuid),recovery_complete_internal(text,uuid),
api_profile_put(text,uuid,bigint,text,uuid,bytea),api_profile_protected_put(text,uuid,bigint,text,uuid,bytea,bigint,uuid,bytea,bytea),api_profiles(text),api_recovery_manifest(text,uuid),api_recovery_complete(text,uuid),api_recovery_profile(text,uuid,text,bigint,boolean),api_recovery_profile_rewrap(text,uuid,text,bigint,bigint,bytea) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(50);
COMMIT;
