BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Keep a revisioned inherit tombstone: deleting the row would permit stale CAS writes.
ALTER TABLE user_host_profiles DROP CONSTRAINT user_host_profiles_method_check;
ALTER TABLE user_host_profiles ADD CONSTRAINT user_host_profiles_method_check CHECK(method IN ('prompt','credential','inherit'));
CREATE OR REPLACE FUNCTION api_profile_put(token text,host uuid,expected bigint,method_input text,credential uuid,username bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; old_rev bigint;
BEGIN
 s:=session_actor(token); PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 IF method_input NOT IN ('prompt','credential','inherit') OR method_input IS NULL OR (method_input='inherit' AND (host IS NULL OR username IS NOT NULL)) THEN RAISE EXCEPTION 'invalid_profile_method' USING ERRCODE='22023'; END IF;
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

INSERT INTO schema_migrations VALUES(15,now());
COMMIT;
