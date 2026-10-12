BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
ALTER FUNCTION api_rotation_next(text,uuid) RENAME TO rotation_credentials_next;
ALTER FUNCTION api_rotation_stage(text,uuid,jsonb) RENAME TO rotation_credentials_stage;
ALTER FUNCTION api_rotation_finish(text,uuid) RENAME TO rotation_credentials_finish;
CREATE FUNCTION api_rotation_next(token text,job uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE r recovery_rotations; answer jsonb; item jsonb; total bigint; remaining bigint;
BEGIN
 r:=rotation_actor(token,job);answer:=rotation_credentials_next(token,job);
 SELECT count(*) INTO total FROM profile_versions WHERE team_id=r.team_id;
 SELECT count(*) INTO remaining FROM profile_versions v WHERE v.team_id=r.team_id AND NOT EXISTS(
 SELECT FROM profile_recovery_envelopes e WHERE (e.team_id,e.account_id,e.host_key,e.revision,e.recovery_key_id)=(v.team_id,v.account_id,v.host_key,v.revision,r.new_key_id));
 IF answer->'item'='null'::jsonb THEN
  SELECT jsonb_build_object('purpose','username','accountId',v.account_id,'hostKey',v.host_key,'revision',v.revision,
  'ciphertext',encode(v.encrypted_username,'hex'),'envelope',encode(e.envelope,'hex'),'sourceHash',encode(sha256(e.envelope),'hex')) INTO item
  FROM profile_versions v LEFT JOIN profile_recovery_envelopes e ON
  (e.team_id,e.account_id,e.host_key,e.revision,e.recovery_key_id)=(v.team_id,v.account_id,v.host_key,v.revision,r.old_key_id)
  WHERE v.team_id=r.team_id AND NOT EXISTS(SELECT FROM profile_recovery_envelopes n WHERE
  (n.team_id,n.account_id,n.host_key,n.revision,n.recovery_key_id)=(v.team_id,v.account_id,v.host_key,v.revision,r.new_key_id))
  ORDER BY v.account_id,v.host_key,v.revision LIMIT 1;
  answer:=jsonb_set(answer,'{item}',coalesce(item,'null'));
 END IF;
 RETURN answer||jsonb_build_object('total',(answer->>'total')::bigint+total,'remaining',(answer->>'remaining')::bigint+remaining);
END $$;
CREATE FUNCTION api_rotation_stage(token text,job uuid,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE r recovery_rotations; source bytea; old bytea; sealed bytea:=decode(body->>'envelope','hex');
 a uuid:=(body->>'accountId')::uuid;h text:=body->>'hostKey';rev bigint:=(body->>'revision')::bigint;
BEGIN
 IF body->>'purpose'<>'username' THEN RETURN rotation_credentials_stage(token,job,body); END IF;
 r:=rotation_actor(token,job);PERFORM pg_advisory_xact_lock(hashtextextended(r.team_id::text,418));
 IF r.status<>'running' OR sealed IS NULL OR octet_length(sealed) NOT BETWEEN 80 AND 4096 THEN RAISE EXCEPTION 'invalid_rotation' USING ERRCODE='22023'; END IF;
 SELECT envelope INTO source FROM profile_recovery_envelopes WHERE (team_id,account_id,host_key,revision,recovery_key_id)=(r.team_id,a,h,rev,r.old_key_id);
 IF source IS NULL OR sha256(source) IS DISTINCT FROM decode(body->>'sourceHash','hex') THEN RAISE EXCEPTION 'rotation_source_changed' USING ERRCODE='40001'; END IF;
 SELECT envelope INTO old FROM profile_recovery_envelopes WHERE (team_id,account_id,host_key,revision,recovery_key_id)=(r.team_id,a,h,rev,r.new_key_id);
 IF old IS NOT NULL AND old<>sealed THEN RAISE EXCEPTION 'operation_mismatch' USING ERRCODE='40001'; END IF;
 INSERT INTO profile_recovery_envelopes VALUES(r.team_id,a,h,rev,r.new_key_id,sealed) ON CONFLICT DO NOTHING;
 RETURN '{"staged":true}';
END $$;
CREATE FUNCTION api_rotation_finish(token text,job uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE r recovery_rotations;
BEGIN
 r:=rotation_actor(token,job);
 PERFORM pg_advisory_xact_lock(hashtextextended(r.team_id::text,419));
 PERFORM pg_advisory_xact_lock(hashtextextended(r.team_id::text,418));
 IF r.status='completed' THEN RETURN '{"completed":true}'; END IF;
 IF EXISTS(SELECT FROM profile_versions v WHERE v.team_id=r.team_id AND NOT EXISTS(
 SELECT FROM profile_recovery_envelopes e WHERE (e.team_id,e.account_id,e.host_key,e.revision,e.recovery_key_id)=(v.team_id,v.account_id,v.host_key,v.revision,r.new_key_id)))
 THEN RAISE EXCEPTION 'rotation_incomplete' USING ERRCODE='23514'; END IF;
 RETURN rotation_credentials_finish(token,job);
END $$;
REVOKE ALL ON FUNCTION rotation_credentials_next(text,uuid),rotation_credentials_stage(text,uuid,jsonb),rotation_credentials_finish(text,uuid),api_rotation_next(text,uuid),api_rotation_stage(text,uuid,jsonb),api_rotation_finish(text,uuid) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(51);
COMMIT;
