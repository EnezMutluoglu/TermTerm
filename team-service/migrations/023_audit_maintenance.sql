BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
ALTER TABLE delegation_audit_rules DROP CONSTRAINT delegation_audit_rules_audit_id_fkey;
ALTER TABLE delegation_audit_rules ADD CONSTRAINT delegation_audit_rules_audit_id_fkey FOREIGN KEY(audit_id) REFERENCES audit_events(id) ON DELETE CASCADE;
CREATE INDEX share_plan_expiry ON credential_share_plans(team_id,expires_at,id);
CREATE INDEX acl_plan_expiry ON acl_change_plans(team_id,expires_at,id);

-- The ordinary service role never receives a cross-team maintenance capability.
CREATE FUNCTION api_audit(token text,before_id bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; result jsonb;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 IF before_id IS NOT NULL AND before_id<=0 THEN RAISE EXCEPTION 'invalid_cursor' USING ERRCODE='22023'; END IF;
 WITH page AS (SELECT e.*,a.display_name FROM audit_events e LEFT JOIN team_accounts a ON (a.team_id,a.id)=(e.team_id,e.actor_id)
 WHERE e.team_id=s.team_id AND (before_id IS NULL OR e.id<before_id) ORDER BY e.id DESC LIMIT 101)
 SELECT jsonb_build_object('events',coalesce((SELECT jsonb_agg(jsonb_build_object('id',id::text,'actor',display_name,
 'action',action,'outcome',outcome,'serverAt',server_at,'resourceId',resource_id,'subjectId',subject_id) ORDER BY id DESC)
 FROM (SELECT * FROM page ORDER BY id DESC LIMIT 100)p),'[]'),
 'nextBefore',CASE WHEN (SELECT count(*) FROM page)>100 THEN (SELECT id::text FROM page ORDER BY id DESC OFFSET 99 LIMIT 1) END)
 INTO result;RETURN result;
END $$;

CREATE FUNCTION api_maintenance(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; days integer;n integer;a integer;k integer;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT audit_days INTO days FROM teams WHERE id=s.team_id;
 -- Bounded work; conflicts, operation receipts, pinned versions and recovery data stay intact.
 WITH expired AS(SELECT id FROM audit_events WHERE team_id=s.team_id AND server_at<now()-make_interval(days=>days)
 ORDER BY server_at,id LIMIT 1000 FOR UPDATE)
 DELETE FROM audit_events WHERE id IN(SELECT id FROM expired);GET DIAGNOSTICS n=ROW_COUNT;
 WITH expired AS(SELECT id FROM acl_change_plans WHERE team_id=s.team_id AND expires_at<now()-interval '1 day'
 ORDER BY expires_at,id LIMIT 100 FOR UPDATE)
 DELETE FROM acl_change_plans WHERE team_id=s.team_id AND id IN(SELECT id FROM expired);GET DIAGNOSTICS a=ROW_COUNT;
 -- Keep applied share receipts: replay cannot create a second independent shared identity.
 WITH expired AS(SELECT id FROM credential_share_plans WHERE team_id=s.team_id AND status<>'applied' AND expires_at<now()-interval '1 day'
 ORDER BY expires_at,id LIMIT 100 FOR UPDATE)
 DELETE FROM credential_share_plans WHERE team_id=s.team_id AND id IN(SELECT id FROM expired);GET DIAGNOSTICS k=ROW_COUNT;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'maintenance.prune','succeeded');
 RETURN jsonb_build_object('auditDeleted',n,'accessPlansDeleted',a,'sharePlansDeleted',k,'auditDays',days,'batchLimit',1000);
END $$;
REVOKE ALL ON FUNCTION api_audit(text,bigint),api_maintenance(text) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(23);
COMMIT;
