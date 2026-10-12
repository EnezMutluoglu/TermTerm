BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
ALTER FUNCTION api_storage(text) RENAME TO storage_v52;
CREATE FUNCTION api_storage(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; result jsonb; extra bigint;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);result:=storage_v52(token);
 SELECT coalesce(sum(bytes),0) INTO extra FROM (
 SELECT octet_length(encrypted_username)::bigint bytes FROM profile_versions WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(envelope) FROM profile_user_envelopes WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(envelope) FROM profile_recovery_envelopes WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(encrypted_username) FROM user_connection_defaults WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(encrypted_username) FROM user_host_profiles WHERE team_id=s.team_id
 )payloads;
 RETURN result||jsonb_build_object('encryptedPayloadBytes',((result->>'encryptedPayloadBytes')::bigint+extra)::text);
END $$;
REVOKE ALL ON FUNCTION storage_v52(text),api_storage(text) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(53);
COMMIT;
