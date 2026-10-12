BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE TABLE recovery_rotations (
 team_id uuid NOT NULL, id uuid NOT NULL DEFAULT gen_random_uuid(), actor_id uuid NOT NULL,
 old_key_id uuid NOT NULL, new_key_id uuid NOT NULL, reason text NOT NULL CHECK(length(reason) BETWEEN 8 AND 2000),
 status text NOT NULL DEFAULT 'running' CHECK(status IN ('running','completed')),
 authorized_until timestamptz NOT NULL, created_at timestamptz NOT NULL DEFAULT now(), completed_at timestamptz,
 PRIMARY KEY(team_id,id), FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,old_key_id) REFERENCES team_recovery_keys(team_id,id),
 FOREIGN KEY(team_id,new_key_id) REFERENCES team_recovery_keys(team_id,id), CHECK(old_key_id<>new_key_id)
);
CREATE UNIQUE INDEX rotation_running ON recovery_rotations(team_id) WHERE status='running';
CREATE INDEX rotation_actor ON recovery_rotations(team_id,actor_id,created_at DESC);
ALTER TABLE recovery_rotations ENABLE ROW LEVEL SECURITY;
CREATE FUNCTION rotation_actor(token text,job uuid) RETURNS recovery_rotations
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; r recovery_rotations;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 SELECT * INTO r FROM recovery_rotations WHERE team_id=s.team_id AND id=job AND actor_id=s.account_id;
 IF r.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF r.authorized_until<=clock_timestamp() THEN RAISE EXCEPTION 'reauthentication_required' USING ERRCODE='28000'; END IF;
 RETURN r;
END $$;
CREATE FUNCTION api_rotation_begin(token text,password text,old_public bytea,new_public bytea,reason_input text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;r recovery_rotations;old_id uuid;new_id uuid;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 IF NOT reauthenticate(s,password) THEN RETURN '{"error":"invalid_credentials"}'; END IF;
 IF octet_length(old_public)<>32 OR octet_length(new_public)<>32 OR old_public=new_public OR length(reason_input) NOT BETWEEN 8 AND 2000
 THEN RAISE EXCEPTION 'invalid_rotation' USING ERRCODE='22023'; END IF;
 -- Match the established shared-credential lock order. Private saves take 418.
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 SELECT id INTO old_id FROM team_recovery_keys WHERE team_id=s.team_id AND active AND public_key=old_public;
 IF old_id IS NULL THEN RAISE EXCEPTION 'recovery_key_mismatch' USING ERRCODE='40001'; END IF;
 SELECT * INTO r FROM recovery_rotations WHERE team_id=s.team_id AND status='running' FOR UPDATE;
 IF r.id IS NOT NULL THEN
  IF r.actor_id<>s.account_id OR r.old_key_id<>old_id OR NOT EXISTS(SELECT FROM team_recovery_keys WHERE team_id=s.team_id AND id=r.new_key_id AND public_key=new_public)
  THEN RAISE EXCEPTION 'rotation_in_progress' USING ERRCODE='40001'; END IF;
  UPDATE recovery_rotations SET authorized_until=clock_timestamp()+interval '30 minutes' WHERE team_id=s.team_id AND id=r.id;
 ELSE
  INSERT INTO team_recovery_keys(team_id,public_key,active) VALUES(s.team_id,new_public,false) RETURNING id INTO new_id;
  INSERT INTO recovery_rotations(team_id,actor_id,old_key_id,new_key_id,reason,authorized_until)
   VALUES(s.team_id,s.account_id,old_id,new_id,reason_input,clock_timestamp()+interval '30 minutes') RETURNING * INTO r;
 END IF;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'recovery.rotation_authorize','succeeded');
 RETURN jsonb_build_object('operationId',r.id,'oldKeyId',r.old_key_id,'newKeyId',r.new_key_id,'status',r.status);
END $$;
CREATE FUNCTION api_rotation_next(token text,job uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE r recovery_rotations;answer jsonb;remaining bigint;total bigint;
BEGIN
 r:=rotation_actor(token,job);
 SELECT count(*)*2 INTO total FROM credential_versions WHERE team_id=r.team_id;
 SELECT count(*) INTO remaining FROM credential_versions v CROSS JOIN (VALUES('data'),('secret')) p(purpose)
 WHERE v.team_id=r.team_id AND NOT EXISTS(SELECT FROM credential_key_envelopes e WHERE
 (e.team_id,e.credential_id,e.revision,e.recovery_key_id,e.purpose)=(v.team_id,v.credential_id,v.revision,r.new_key_id,p.purpose));
 SELECT jsonb_build_object('credentialId',v.credential_id,'revision',v.revision,'purpose',p.purpose,
 'ciphertext',encode(CASE p.purpose WHEN 'data' THEN v.encrypted_data ELSE v.encrypted_secret END,'hex'),
 'envelope',encode(e.envelope,'hex'),'sourceHash',encode(sha256(e.envelope),'hex')) INTO answer
 FROM credential_versions v CROSS JOIN (VALUES('data'),('secret')) p(purpose)
 LEFT JOIN credential_key_envelopes e ON (e.team_id,e.credential_id,e.revision,e.recovery_key_id,e.purpose)=(v.team_id,v.credential_id,v.revision,r.old_key_id,p.purpose)
 WHERE v.team_id=r.team_id AND NOT EXISTS(SELECT FROM credential_key_envelopes n WHERE
 (n.team_id,n.credential_id,n.revision,n.recovery_key_id,n.purpose)=(v.team_id,v.credential_id,v.revision,r.new_key_id,p.purpose))
 ORDER BY v.credential_id,v.revision,p.purpose LIMIT 1;
 RETURN jsonb_build_object('status',r.status,'total',total,'remaining',remaining,'item',answer);
END $$;
CREATE FUNCTION api_rotation_stage(token text,job uuid,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE r recovery_rotations;existing bytea;source bytea;sealed bytea:=decode(body->>'envelope','hex');
 c uuid:=(body->>'credentialId')::uuid;rev bigint:=(body->>'revision')::bigint;p text:=body->>'purpose';
BEGIN
 r:=rotation_actor(token,job);
 PERFORM pg_advisory_xact_lock(hashtextextended(r.team_id::text,418));
 IF r.status<>'running' OR p NOT IN ('data','secret') OR octet_length(sealed) NOT BETWEEN 80 AND 4096
 THEN RAISE EXCEPTION 'invalid_rotation' USING ERRCODE='22023'; END IF;
 SELECT envelope INTO source FROM credential_key_envelopes WHERE (team_id,credential_id,revision,recovery_key_id,purpose)=(r.team_id,c,rev,r.old_key_id,p);
 IF source IS NULL OR sha256(source) IS DISTINCT FROM decode(body->>'sourceHash','hex')
 THEN RAISE EXCEPTION 'rotation_source_changed' USING ERRCODE='40001'; END IF;
 SELECT envelope INTO existing FROM credential_key_envelopes WHERE (team_id,credential_id,revision,recovery_key_id,purpose)=(r.team_id,c,rev,r.new_key_id,p);
 IF existing IS NOT NULL AND existing<>sealed THEN RAISE EXCEPTION 'operation_mismatch' USING ERRCODE='40001'; END IF;
 IF existing IS NULL THEN INSERT INTO credential_key_envelopes(team_id,credential_id,revision,recovery_key_id,purpose,envelope) VALUES(r.team_id,c,rev,r.new_key_id,p,sealed); END IF;
 RETURN '{"staged":true}';
END $$;
CREATE FUNCTION api_rotation_finish(token text,job uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE r recovery_rotations;
BEGIN
 r:=rotation_actor(token,job);
 PERFORM pg_advisory_xact_lock(hashtextextended(r.team_id::text,419));
 PERFORM pg_advisory_xact_lock(hashtextextended(r.team_id::text,418));
 SELECT * INTO r FROM recovery_rotations WHERE team_id=r.team_id AND id=job FOR UPDATE;
 IF r.status='completed' THEN RETURN '{"completed":true}'; END IF;
 IF EXISTS(SELECT FROM credential_versions v CROSS JOIN (VALUES('data'),('secret')) p(purpose)
 WHERE v.team_id=r.team_id AND NOT EXISTS(SELECT FROM credential_key_envelopes e WHERE
 (e.team_id,e.credential_id,e.revision,e.recovery_key_id,e.purpose)=(v.team_id,v.credential_id,v.revision,r.new_key_id,p.purpose)))
 THEN RAISE EXCEPTION 'rotation_incomplete' USING ERRCODE='23514'; END IF;
 IF NOT EXISTS(SELECT FROM team_recovery_keys WHERE team_id=r.team_id AND id=r.old_key_id AND active)
 THEN RAISE EXCEPTION 'crypto_context_changed' USING ERRCODE='40001'; END IF;
 UPDATE team_recovery_keys SET active=false WHERE team_id=r.team_id AND id=r.old_key_id;
 UPDATE team_recovery_keys SET active=true WHERE team_id=r.team_id AND id=r.new_key_id;
 UPDATE recovery_rotations SET status='completed',completed_at=clock_timestamp() WHERE team_id=r.team_id AND id=r.id;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(r.team_id,r.actor_id,'recovery.rotation_complete','succeeded');
 RETURN '{"completed":true}';
END $$;
REVOKE ALL ON FUNCTION rotation_actor(text,uuid),api_rotation_begin(text,text,bytea,bytea,text),api_rotation_next(text,uuid),api_rotation_stage(text,uuid,jsonb),api_rotation_finish(text,uuid) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(46);
COMMIT;
