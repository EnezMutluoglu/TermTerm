BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION maintenance_prune(t uuid,actor uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE days integer;n integer;a integer;k integer;
BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended(t::text,419));
 SELECT audit_days INTO days FROM teams WHERE id=t;
 -- Bounded work; conflicts, operation receipts, pinned versions and recovery data stay intact.
 WITH expired AS(SELECT id FROM audit_events WHERE team_id=t AND server_at<now()-make_interval(days=>days)
 ORDER BY server_at,id LIMIT 1000 FOR UPDATE)
 DELETE FROM audit_events WHERE id IN(SELECT id FROM expired);GET DIAGNOSTICS n=ROW_COUNT;
 WITH expired AS(SELECT id FROM acl_change_plans WHERE team_id=t AND expires_at<now()-interval '1 day'
 ORDER BY expires_at,id LIMIT 100 FOR UPDATE)
 DELETE FROM acl_change_plans WHERE team_id=t AND id IN(SELECT id FROM expired);GET DIAGNOSTICS a=ROW_COUNT;
 -- Keep applied share receipts: replay cannot create a second independent shared identity.
 WITH expired AS(SELECT id FROM credential_share_plans WHERE team_id=t AND status<>'applied' AND expires_at<now()-interval '1 day'
 ORDER BY expires_at,id LIMIT 100 FOR UPDATE)
 DELETE FROM credential_share_plans WHERE team_id=t AND id IN(SELECT id FROM expired);GET DIAGNOSTICS k=ROW_COUNT;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(t,actor,'maintenance.prune','succeeded');
 RETURN jsonb_build_object('auditDeleted',n,'accessPlansDeleted',a,'sharePlansDeleted',k,'auditDays',days,'batchLimit',1000);
END $$;

CREATE OR REPLACE FUNCTION api_maintenance(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN s:=session_actor(token);PERFORM require_owner(s);RETURN maintenance_prune(s.team_id,s.account_id);END $$;
CREATE TABLE team_maintenance_state(team_id uuid PRIMARY KEY REFERENCES teams(id),next_due timestamptz NOT NULL DEFAULT now(),last_completed timestamptz);
CREATE INDEX team_maintenance_due ON team_maintenance_state(next_due,team_id);
ALTER TABLE team_maintenance_state ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON team_maintenance_state FROM PUBLIC;
-- No HTTP route: this narrowly bounded housekeeping capability belongs only to
-- the backend role. It cannot return records, change policy or remove live data.
CREATE FUNCTION maintenance_tick() RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE t uuid; result jsonb; more boolean;
BEGIN
 INSERT INTO team_maintenance_state(team_id) SELECT id FROM teams WHERE NOT EXISTS(SELECT FROM team_maintenance_state WHERE team_id=teams.id) ORDER BY id LIMIT 100 ON CONFLICT DO NOTHING;
 SELECT team_id INTO t FROM team_maintenance_state WHERE next_due<=now() ORDER BY next_due,team_id LIMIT 1 FOR UPDATE SKIP LOCKED;
 IF t IS NULL THEN RETURN '{"processed":false}'; END IF;
 result:=maintenance_prune(t,NULL);
 more:=(result->>'auditDeleted')::integer>=1000 OR (result->>'accessPlansDeleted')::integer>=100 OR (result->>'sharePlansDeleted')::integer>=100;
 UPDATE team_maintenance_state SET next_due=now()+CASE WHEN more THEN interval '1 minute' ELSE interval '24 hours' END,last_completed=CASE WHEN more THEN last_completed ELSE now() END WHERE team_id=t;
 RETURN jsonb_build_object('processed',true,'more',more);
END $$;
CREATE FUNCTION api_storage(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; total bigint;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 SELECT coalesce(sum(bytes),0) INTO total FROM (
 SELECT octet_length(encrypted_snapshot)::bigint bytes FROM resource_versions WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(encrypted_data)::bigint+octet_length(encrypted_secret) FROM credential_versions WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(envelope) FROM credential_key_envelopes WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(envelope) FROM resource_key_envelopes WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(envelope) FROM vault_key_envelopes WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(encrypted_name) FROM vaults WHERE team_id=s.team_id
 UNION ALL SELECT octet_length(encrypted_username) FROM profile_username_archives WHERE team_id=s.team_id
 )payloads;
 RETURN jsonb_build_object('encryptedPayloadBytes',total::text,'records',(SELECT count(*) FROM resources WHERE team_id=s.team_id AND NOT deleted),
 'versions',(SELECT count(*) FROM resource_versions WHERE team_id=s.team_id),'auditDays',(SELECT audit_days FROM teams WHERE id=s.team_id),
 'lastMaintenance',(SELECT last_completed FROM team_maintenance_state WHERE team_id=s.team_id),
 'nextMaintenance',(SELECT next_due FROM team_maintenance_state WHERE team_id=s.team_id));
END $$;
REVOKE ALL ON FUNCTION maintenance_prune(uuid,uuid),maintenance_tick(),api_storage(text) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(28);
COMMIT;
