BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Policy changes survive temporary plan cleanup; no ciphertext/secrets enter audit.
CREATE TABLE acl_audit_rules (
 audit_id bigint NOT NULL REFERENCES audit_events(id) ON DELETE CASCADE,
 permission text NOT NULL CHECK(permission IN ('read','connect','edit')),
 effect text NOT NULL CHECK(effect IN ('allow','deny','clear')),
 PRIMARY KEY(audit_id,permission)
);
ALTER TABLE acl_audit_rules ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON acl_audit_rules FROM PUBLIC;
CREATE OR REPLACE FUNCTION api_acl_finish(token text,plan uuid,cancel boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p acl_change_plans;rev bigint;event_id bigint;
BEGIN
 -- Same global ordering as resource writes: actor -> team lock -> account locks -> plan.
 s:=session_actor(token);PERFORM require_owner(s);
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 p:=acl_plan_actor(token,plan);
 IF cancel IS NULL THEN RAISE EXCEPTION 'invalid_action' USING ERRCODE='22023'; END IF;
 IF p.status='applied' AND NOT cancel THEN RETURN jsonb_build_object('status','applied','aclRevision',p.result_revision); END IF;
 IF p.status<>'pending' THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='22023'; END IF;
 IF cancel THEN
 UPDATE acl_change_plans SET status='cancelled' WHERE team_id=p.team_id AND id=plan;
 DELETE FROM acl_plan_keys WHERE team_id=p.team_id AND plan_id=plan;
 RETURN '{"status":"cancelled"}'; END IF;
 IF p.expires_at<=now() OR p.acl_revision<>(SELECT acl_revision FROM teams WHERE id=p.team_id)
 OR p.vault_revision<>(SELECT revision FROM vaults WHERE team_id=p.team_id AND id=p.vault_id)
 OR NOT EXISTS(SELECT FROM account_crypto_identities i JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id)
 WHERE i.team_id=p.team_id AND i.account_id=p.account_id AND i.version=p.target_identity AND i.active AND a.status='active')
 OR NOT EXISTS(SELECT FROM account_crypto_identities WHERE team_id=p.team_id AND account_id=p.actor_id AND version=p.actor_identity AND active)
 THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001'; END IF;
 IF EXISTS(SELECT FROM acl_plan_keys WHERE team_id=p.team_id AND plan_id=plan AND target_envelope IS NULL) THEN RAISE EXCEPTION 'keys_incomplete' USING ERRCODE='22023'; END IF;
 DELETE FROM resource_acl WHERE team_id=p.team_id AND resource_id=p.resource_id AND account_id=p.account_id
 AND permission IN (SELECT c->>'permission' FROM jsonb_array_elements(p.changes)c);
 INSERT INTO resource_acl SELECT p.team_id,p.resource_id,p.account_id,c->>'permission',c->>'effect' FROM jsonb_array_elements(p.changes)c WHERE c->>'effect'<>'clear';
 INSERT INTO resource_key_envelopes SELECT p.team_id,object_id,revision,p.account_id,p.target_identity,target_envelope FROM acl_plan_keys
 WHERE team_id=p.team_id AND plan_id=plan AND kind='resource' ON CONFLICT DO NOTHING;
 INSERT INTO vault_key_envelopes SELECT p.team_id,object_id,p.account_id,p.target_identity,target_envelope FROM acl_plan_keys
 WHERE team_id=p.team_id AND plan_id=plan AND kind='vault' ON CONFLICT DO NOTHING;
 -- Remove server copies of no-longer-authorized envelopes. Previously downloaded keys cannot be recalled.
 DELETE FROM resource_key_envelopes e USING resources r WHERE (e.team_id,e.resource_id)=(r.team_id,r.id)
 AND r.team_id=p.team_id AND r.vault_id=p.vault_id AND e.account_id=p.account_id
 AND NOT (allowed(p.team_id,p.account_id,r.id,'read') OR allowed(p.team_id,p.account_id,r.id,'connect') OR allowed(p.team_id,p.account_id,r.id,'edit'));
 IF NOT EXISTS(SELECT FROM resources r WHERE r.team_id=p.team_id AND r.vault_id=p.vault_id AND
 (allowed(p.team_id,p.account_id,r.id,'read') OR allowed(p.team_id,p.account_id,r.id,'connect') OR allowed(p.team_id,p.account_id,r.id,'edit'))) THEN
 DELETE FROM vault_key_envelopes WHERE team_id=p.team_id AND vault_id=p.vault_id AND account_id=p.account_id; END IF;
 UPDATE teams SET acl_revision=acl_revision+1 WHERE id=p.team_id RETURNING acl_revision INTO rev;
 UPDATE acl_change_plans SET status='applied',result_revision=rev WHERE team_id=p.team_id AND id=plan;
 DELETE FROM acl_plan_keys WHERE team_id=p.team_id AND plan_id=plan;
 INSERT INTO audit_events(team_id,actor_id,subject_id,action,resource_id,outcome) VALUES(p.team_id,p.actor_id,p.account_id,'acl.apply',p.resource_id,'succeeded') RETURNING id INTO event_id;
 INSERT INTO acl_audit_rules SELECT event_id,c->>'permission',c->>'effect' FROM jsonb_array_elements(p.changes)c;
 RETURN jsonb_build_object('status','applied','aclRevision',rev);
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(13,now());
COMMIT;