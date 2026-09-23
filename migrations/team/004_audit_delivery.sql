BEGIN;
SET search_path=termterm_team,public;
CREATE UNIQUE INDEX audit_operation_once ON audit_events(team_id,actor_id,(summary->>'operationId')) WHERE summary->>'operationId' IS NOT NULL;
CREATE OR REPLACE FUNCTION audit(t uuid, r uuid, a text, s jsonb DEFAULT '{}') RETURNS void LANGUAGE sql AS $$
 INSERT INTO audit_events(team_id,actor_id,device_id,record_id,action,summary) VALUES(t,actor(),nullif(current_setting('termterm.device',true),'')::uuid,r,a,s) ON CONFLICT DO NOTHING
$$;
REVOKE ALL ON FUNCTION audit(uuid,uuid,text,jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION audit(uuid,uuid,text,jsonb) TO termterm_team_executor;
COMMIT;
