BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE INDEX delegation_account_scope ON acl_delegations(team_id,account_id,resource_id,permission);
CREATE TABLE delegation_audit_rules (
 audit_id bigint NOT NULL REFERENCES audit_events(id), permission text NOT NULL,
 PRIMARY KEY(audit_id,permission), CHECK(permission IN ('read','connect','edit'))
);
ALTER TABLE delegation_audit_rules ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON delegation_audit_rules FROM PUBLIC;
CREATE FUNCTION within_scope(t uuid,r uuid,scope uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 WITH RECURSIVE ancestors(id,depth) AS (
 SELECT r,0 UNION ALL SELECT resource_parent(t,id),depth+1 FROM ancestors WHERE id IS NOT NULL AND depth<64
 ) SELECT EXISTS(SELECT FROM ancestors WHERE id=scope)
$$;
CREATE FUNCTION require_acl_scope(s auth_sessions,target uuid,scope uuid,changes jsonb) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner') THEN RETURN; END IF;
 IF target=s.account_id OR NOT EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='manager')
 OR NOT EXISTS(SELECT FROM team_accounts a JOIN team_memberships m ON (m.team_id,m.account_id)=(a.team_id,a.id)
 WHERE a.team_id=s.team_id AND a.id=target AND a.status='active' AND m.role<>'owner')
 OR NOT allowed(s.team_id,s.account_id,scope,'read') THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF EXISTS(SELECT FROM jsonb_array_elements(changes)c WHERE
 NOT allowed(s.team_id,s.account_id,scope,c->>'permission') OR NOT EXISTS(
 SELECT FROM acl_delegations d WHERE d.team_id=s.team_id AND d.account_id=s.account_id
 AND d.permission=c->>'permission' AND within_scope(s.team_id,scope,d.resource_id)))
 THEN RAISE EXCEPTION 'delegation_scope_denied' USING ERRCODE='42501'; END IF;
 -- A folder grant must not bypass an explicit restriction on the manager's child.
 -- Even a no-op grant is checked: it also governs future inherited permissions.
 IF EXISTS(SELECT FROM resources r CROSS JOIN jsonb_array_elements(changes)c
 WHERE r.team_id=s.team_id AND NOT r.deleted AND within_scope(s.team_id,r.id,scope)
 AND (allowed(s.team_id,target,r.id,c->>'permission') IS DISTINCT FROM allowed_proposed(s.team_id,target,r.id,c->>'permission',scope,changes)
 OR (c->>'effect'<>'deny' AND allowed_proposed(s.team_id,target,r.id,c->>'permission',scope,changes)))
 AND (NOT allowed(s.team_id,s.account_id,r.id,'read') OR NOT allowed(s.team_id,s.account_id,r.id,c->>'permission')))
 THEN RAISE EXCEPTION 'delegation_child_denied' USING ERRCODE='42501'; END IF;
END $$;
CREATE FUNCTION api_delegations(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;rev bigint;items jsonb;own boolean;
BEGIN
 s:=session_actor(token);
 own:=EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner');
 SELECT acl_revision INTO rev FROM teams WHERE id=s.team_id;
 SELECT coalesce(jsonb_agg(jsonb_build_object('accountId',account_id,'resourceId',resource_id,'permissions',permissions) ORDER BY account_id,resource_id),'[]') INTO items
 FROM (SELECT d.account_id,d.resource_id,array_agg(d.permission ORDER BY d.permission) permissions
 FROM acl_delegations d JOIN resources r ON (r.team_id,r.id)=(d.team_id,d.resource_id)
 WHERE d.team_id=s.team_id AND NOT r.deleted AND (own OR d.account_id=s.account_id)
 GROUP BY d.account_id,d.resource_id)x;
 RETURN jsonb_build_object('aclRevision',rev,'delegations',items);
END $$;
CREATE FUNCTION api_delegation_set(token text,target uuid,scope uuid,permissions jsonb,expected bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;rev bigint;event_id bigint;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT acl_revision INTO rev FROM teams WHERE id=s.team_id;
 IF expected IS NULL OR expected<>rev THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001'; END IF;
 IF target=s.account_id OR NOT EXISTS(SELECT FROM team_accounts a JOIN team_memberships m ON (m.team_id,m.account_id)=(a.team_id,a.id)
 WHERE a.team_id=s.team_id AND a.id=target AND m.role='manager') THEN RAISE EXCEPTION 'invalid_member' USING ERRCODE='22023'; END IF;
 IF NOT EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=scope AND NOT deleted AND kind IN ('host','folder'))
 THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 IF jsonb_typeof(permissions) IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'invalid_rules' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(permissions)>3 OR EXISTS(SELECT FROM jsonb_array_elements(permissions)p WHERE jsonb_typeof(p)<>'string' OR p#>>'{}' NOT IN ('read','connect','edit'))
 OR (SELECT count(DISTINCT p) FROM jsonb_array_elements(permissions)p)<>jsonb_array_length(permissions)
 THEN RAISE EXCEPTION 'invalid_rules' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(permissions)>0 AND (NOT EXISTS(SELECT FROM team_accounts WHERE team_id=s.team_id AND id=target AND status='active')
 OR NOT allowed(s.team_id,target,scope,'read') OR EXISTS(SELECT FROM jsonb_array_elements_text(permissions)p WHERE NOT allowed(s.team_id,target,scope,p)))
 THEN RAISE EXCEPTION 'grant_access_first' USING ERRCODE='42501'; END IF;
 DELETE FROM acl_delegations WHERE team_id=s.team_id AND account_id=target AND resource_id=scope;
 INSERT INTO acl_delegations SELECT s.team_id,scope,target,p FROM jsonb_array_elements_text(permissions)p;
 UPDATE teams SET acl_revision=acl_revision+1 WHERE id=s.team_id RETURNING acl_revision INTO rev;
 INSERT INTO audit_events(team_id,actor_id,subject_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,target,'delegation.set',scope,'succeeded') RETURNING id INTO event_id;
 INSERT INTO delegation_audit_rules SELECT event_id,p FROM jsonb_array_elements_text(permissions)p;
 RETURN jsonb_build_object('aclRevision',rev,'permissions',permissions);
END $$;
CREATE OR REPLACE FUNCTION api_acl_prepare(token text,target uuid,scope uuid,changes jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;plan uuid;v uuid;av bigint;tv bigint;total bigint;affected bigint;
BEGIN
 s:=session_actor(token);
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND id IN (s.account_id,target) ORDER BY id FOR SHARE;
 IF target=s.account_id OR NOT EXISTS(SELECT FROM team_accounts a JOIN team_memberships m ON (m.team_id,m.account_id)=(a.team_id,a.id)
 WHERE a.team_id=s.team_id AND a.id=target AND a.status='active' AND m.role<>'owner') THEN RAISE EXCEPTION 'invalid_member' USING ERRCODE='22023'; END IF;
 IF jsonb_typeof(changes) IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'invalid_rules' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(changes) NOT BETWEEN 1 AND 3 OR EXISTS(SELECT FROM jsonb_array_elements(changes)c
 WHERE c->>'permission' IS NULL OR c->>'permission' NOT IN ('read','connect','edit') OR c->>'effect' IS NULL OR c->>'effect' NOT IN ('allow','deny','clear')
 OR c - 'permission' - 'effect' <> '{}'::jsonb)
 OR (SELECT count(DISTINCT c->>'permission') FROM jsonb_array_elements(changes)c)<>jsonb_array_length(changes)
 THEN RAISE EXCEPTION 'invalid_rules' USING ERRCODE='22023'; END IF;
 SELECT vault_id INTO v FROM resources WHERE team_id=s.team_id AND id=scope AND kind IN ('folder','host') AND NOT deleted;
 IF v IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 PERFORM require_acl_scope(s,target,scope,changes);
 SELECT version INTO av FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND active;
 SELECT version INTO tv FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=target AND active;
 IF av IS NULL OR tv IS NULL THEN RAISE EXCEPTION 'identity_required' USING ERRCODE='22023'; END IF;
 DELETE FROM acl_change_plans WHERE team_id=s.team_id AND actor_id=s.account_id AND expires_at<now()-interval '1 day';
 IF (SELECT count(*) FROM acl_change_plans WHERE team_id=s.team_id AND actor_id=s.account_id AND status='pending' AND expires_at>now())>=20
 THEN RAISE EXCEPTION 'too_many_plans' USING ERRCODE='22023'; END IF;
 SELECT count(*) INTO affected FROM resources r WHERE r.team_id=s.team_id AND r.vault_id=v AND NOT r.deleted
 AND EXISTS(SELECT FROM jsonb_array_elements(changes)c WHERE allowed(s.team_id,target,r.id,c->>'permission')
 IS DISTINCT FROM allowed_proposed(s.team_id,target,r.id,c->>'permission',scope,changes));
 INSERT INTO acl_change_plans(team_id,actor_id,account_id,resource_id,vault_id,acl_revision,vault_revision,actor_identity,target_identity,changes,affected_count)
 SELECT s.team_id,s.account_id,target,scope,v,t.acl_revision,x.revision,av,tv,changes,affected FROM teams t JOIN vaults x ON x.team_id=t.id AND x.id=v WHERE t.id=s.team_id RETURNING id INTO plan;
 INSERT INTO acl_plan_keys(team_id,plan_id,ordinal,kind,object_id,revision,source_envelope)
 SELECT s.team_id,plan,row_number() OVER(ORDER BY rv.resource_id,rv.revision),'resource',rv.resource_id,rv.revision,e.envelope
 FROM resources r JOIN resource_versions rv ON (rv.team_id,rv.resource_id)=(r.team_id,r.id)
 LEFT JOIN resource_key_envelopes e ON (e.team_id,e.resource_id,e.revision,e.account_id,e.identity_version)=(rv.team_id,rv.resource_id,rv.revision,s.account_id,av)
 WHERE r.team_id=s.team_id AND r.vault_id=v AND NOT r.deleted
 AND (EXISTS(SELECT FROM unnest(ARRAY['read','connect','edit'])p WHERE allowed_proposed(s.team_id,target,r.id,p,scope,changes)) OR (rv.revision=r.revision AND navigation_needed_proposed(s.team_id,target,r.id,scope,changes)))
 AND NOT EXISTS(SELECT FROM resource_key_envelopes x WHERE (x.team_id,x.resource_id,x.revision,x.account_id,x.identity_version)=(rv.team_id,rv.resource_id,rv.revision,target,tv));
 GET DIAGNOSTICS total=ROW_COUNT;
 IF total>100000 THEN RAISE EXCEPTION 'scope_too_large' USING ERRCODE='22023'; END IF;
 IF EXISTS(SELECT FROM resources r WHERE r.team_id=s.team_id AND r.vault_id=v AND NOT r.deleted
 AND EXISTS(SELECT FROM unnest(ARRAY['read','connect','edit'])p WHERE allowed_proposed(s.team_id,target,r.id,p,scope,changes)))
 AND NOT EXISTS(SELECT FROM vault_key_envelopes WHERE team_id=s.team_id AND vault_id=v AND account_id=target AND identity_version=tv) THEN
 INSERT INTO acl_plan_keys(team_id,plan_id,ordinal,kind,object_id,revision,source_envelope)
 VALUES(s.team_id,plan,total+1,'vault',v,0,(SELECT envelope FROM vault_key_envelopes WHERE team_id=s.team_id AND vault_id=v AND account_id=s.account_id AND identity_version=av));
 total:=total+1; END IF;
 RETURN jsonb_build_object('id',plan,'affectedResources',affected,'requiredKeys',total,'changes',changes,'expiresInSeconds',1800);
END $$;
CREATE OR REPLACE FUNCTION api_acl_finish(token text,plan uuid,cancel boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p acl_change_plans;rev bigint;event_id bigint;
BEGIN
 -- Same global ordering as resource writes: actor -> team lock -> account locks -> plan.
 s:=session_actor(token);
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
 AND NOT (allowed(p.team_id,p.account_id,r.id,'read') OR allowed(p.team_id,p.account_id,r.id,'connect') OR allowed(p.team_id,p.account_id,r.id,'edit') OR (e.revision=r.revision AND navigation_needed(p.team_id,p.account_id,r.id)));
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
CREATE OR REPLACE FUNCTION acl_plan_actor(token text,plan uuid) RETURNS acl_change_plans LANGUAGE plpgsql AS $$
DECLARE s auth_sessions;p acl_change_plans;
BEGIN
 s:=session_actor(token);
 SELECT * INTO p FROM acl_change_plans WHERE team_id=s.team_id AND id=plan AND actor_id=s.account_id FOR UPDATE;
 IF p.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 PERFORM require_acl_scope(s,p.account_id,p.resource_id,p.changes);
 RETURN p;
END $$;
CREATE OR REPLACE FUNCTION api_members(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;own boolean;
BEGIN
 s:=session_actor(token);
 own:=EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner');
 IF NOT own AND NOT EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='manager')
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('accountId',a.id,'username',a.username,'displayName',a.display_name,'status',a.status,'role',m.role) ORDER BY a.username)
 FROM team_accounts a JOIN team_memberships m ON (m.team_id,m.account_id)=(a.team_id,a.id)
 WHERE a.team_id=s.team_id AND (own OR a.status='active')),'[]');
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(20,now());
COMMIT;
