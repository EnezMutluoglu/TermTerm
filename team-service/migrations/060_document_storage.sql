BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
ALTER FUNCTION api_storage(text) RENAME TO storage_v59;
CREATE FUNCTION api_storage(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; result jsonb; extra bigint;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);result:=storage_v59(token);
 SELECT coalesce(sum(bytes),0) INTO extra FROM (
 SELECT (octet_length(encrypted_name)+octet_length(encrypted_body))::bigint bytes FROM snippets WHERE team_id=s.team_id
 UNION ALL SELECT (octet_length(encrypted_name)+octet_length(encrypted_body))::bigint FROM workspaces WHERE team_id=s.team_id
 )payloads;
 RETURN result||jsonb_build_object('encryptedPayloadBytes',((result->>'encryptedPayloadBytes')::bigint+extra)::text);
END $$;
REVOKE ALL ON FUNCTION storage_v59(text),api_storage(text) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(60);
COMMIT;
