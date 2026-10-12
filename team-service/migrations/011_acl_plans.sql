BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Prepared keys remain private to the owner until ACL and envelopes commit together.
CREATE TABLE acl_change_plans (
 team_id uuid NOT NULL, id uuid NOT NULL DEFAULT gen_random_uuid(), actor_id uuid NOT NULL,
 account_id uuid NOT NULL, resource_id uuid NOT NULL, vault_id uuid NOT NULL,
 acl_revision bigint NOT NULL, vault_revision bigint NOT NULL, actor_identity bigint NOT NULL, target_identity bigint NOT NULL,
 changes jsonb NOT NULL, affected_count bigint NOT NULL DEFAULT 0,
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','applied','cancelled')),
 expires_at timestamptz NOT NULL DEFAULT now()+interval '30 minutes', result_revision bigint,
 PRIMARY KEY(team_id,id), FOREIGN KEY(team_id,actor_id,actor_identity) REFERENCES account_crypto_identities(team_id,account_id,version),
 FOREIGN KEY(team_id,account_id,target_identity) REFERENCES account_crypto_identities(team_id,account_id,version),
 FOREIGN KEY(team_id,vault_id,resource_id) REFERENCES resources(team_id,vault_id,id)
);
CREATE INDEX acl_plans_pending ON acl_change_plans(team_id,actor_id,expires_at) WHERE status='pending';
CREATE TABLE acl_plan_keys (
 team_id uuid NOT NULL, plan_id uuid NOT NULL, ordinal bigint NOT NULL,
 kind text NOT NULL CHECK(kind IN ('resource','vault')), object_id uuid NOT NULL, revision bigint NOT NULL,
 source_envelope bytea NOT NULL, target_envelope bytea CHECK(octet_length(target_envelope) BETWEEN 80 AND 4096),
 PRIMARY KEY(team_id,plan_id,ordinal), UNIQUE(team_id,plan_id,kind,object_id,revision),
 FOREIGN KEY(team_id,plan_id) REFERENCES acl_change_plans(team_id,id) ON DELETE CASCADE
);
ALTER TABLE acl_change_plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE acl_plan_keys ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON acl_change_plans,acl_plan_keys FROM PUBLIC;

-- Evaluate an owner-proposed patch without temporarily publishing any ACL rows.
CREATE FUNCTION allowed_proposed(t uuid,a uuid,r uuid,p text,scope uuid,changes jsonb) RETURNS boolean LANGUAGE sql STABLE AS $$
 WITH RECURSIVE ancestry(id,depth) AS (
  SELECT r,0 UNION ALL
  SELECT coalesce(f.parent_id,h.folder_id),x.depth+1 FROM ancestry x
  LEFT JOIN folders f ON f.team_id=t AND f.id=x.id LEFT JOIN hosts h ON h.team_id=t AND h.id=x.id
  WHERE x.depth<64 AND coalesce(f.parent_id,h.folder_id) IS NOT NULL
 ), rules AS (
  SELECT effect FROM resource_acl acl JOIN ancestry x ON x.id=acl.resource_id
  WHERE acl.team_id=t AND acl.account_id=a AND acl.permission=p
   AND NOT (acl.resource_id=scope AND EXISTS(SELECT FROM jsonb_array_elements(changes)c WHERE c->>'permission'=p))
  UNION ALL SELECT c->>'effect' FROM jsonb_array_elements(changes)c
  WHERE c->>'permission'=p AND c->>'effect'<>'clear' AND EXISTS(SELECT FROM ancestry WHERE id=scope)
 ) SELECT EXISTS(SELECT FROM team_accounts WHERE team_id=t AND id=a AND status='active')
 AND EXISTS(SELECT FROM resources WHERE team_id=t AND id=r AND NOT deleted)
 AND (EXISTS(SELECT FROM team_memberships WHERE team_id=t AND account_id=a AND role='owner')
 OR (NOT EXISTS(SELECT FROM rules WHERE effect='deny') AND EXISTS(SELECT FROM rules WHERE effect='allow')))
$$;
CREATE FUNCTION acl_plan_actor(token text,plan uuid) RETURNS acl_change_plans
LANGUAGE plpgsql AS $$
DECLARE s auth_sessions;p acl_change_plans;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 SELECT * INTO p FROM acl_change_plans WHERE team_id=s.team_id AND id=plan AND actor_id=s.account_id FOR UPDATE;
 IF p.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 RETURN p;
END $$;
CREATE FUNCTION api_acl_prepare(token text,target uuid,scope uuid,changes jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;plan uuid;v uuid;av bigint;tv bigint;total bigint;affected bigint;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
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
 AND EXISTS(SELECT FROM unnest(ARRAY['read','connect','edit'])p WHERE allowed_proposed(s.team_id,target,r.id,p,scope,changes))
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
CREATE FUNCTION api_acl_keys(token text,plan uuid,after_index bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE p acl_change_plans;items jsonb;
BEGIN
 p:=acl_plan_actor(token,plan);
 IF p.status<>'pending' OR p.expires_at<=now() OR after_index IS NULL OR after_index<0 THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='22023'; END IF;
 SELECT coalesce(jsonb_agg(jsonb_build_object('ordinal',ordinal,'kind',kind,'id',object_id,'revision',revision,'envelope',encode(source_envelope,'hex'),'staged',target_envelope IS NOT NULL) ORDER BY ordinal),'[]')
 INTO items FROM (SELECT * FROM acl_plan_keys WHERE team_id=p.team_id AND plan_id=plan AND ordinal>after_index ORDER BY ordinal LIMIT 20)k;
 RETURN jsonb_build_object('keys',items,'nextAfter',CASE WHEN jsonb_array_length(items)>0 THEN items->(jsonb_array_length(items)-1)->'ordinal' END,
 'targetAccountId',p.account_id,'targetIdentity',p.target_identity,'publicKey',(SELECT encode(public_key,'hex') FROM account_crypto_identities WHERE team_id=p.team_id AND account_id=p.account_id AND version=p.target_identity));
END $$;
CREATE FUNCTION api_acl_stage(token text,plan uuid,items jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE p acl_change_plans;e jsonb;
BEGIN
 p:=acl_plan_actor(token,plan);
 IF p.status<>'pending' OR p.expires_at<=now() OR jsonb_typeof(items) IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(items) NOT BETWEEN 1 AND 20 OR (SELECT count(DISTINCT (x->>'ordinal')::bigint) FROM jsonb_array_elements(items)x)<>jsonb_array_length(items)
 THEN RAISE EXCEPTION 'invalid_batch' USING ERRCODE='22023'; END IF;
 FOR e IN SELECT * FROM jsonb_array_elements(items) LOOP
 IF e->>'envelope' IS NULL OR e - 'ordinal' - 'envelope'<>'{}'::jsonb THEN RAISE EXCEPTION 'invalid_envelope' USING ERRCODE='22023'; END IF;
 UPDATE acl_plan_keys SET target_envelope=decode(e->>'envelope','hex') WHERE team_id=p.team_id AND plan_id=plan AND ordinal=(e->>'ordinal')::bigint;
 IF NOT FOUND THEN RAISE EXCEPTION 'invalid_key_index' USING ERRCODE='22023'; END IF;
 END LOOP;
 RETURN jsonb_build_object('staged',jsonb_array_length(items));
END $$;
CREATE FUNCTION api_acl_finish(token text,plan uuid,cancel boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p acl_change_plans;rev bigint;
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
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(p.team_id,p.actor_id,'acl.apply',p.resource_id,'succeeded');
 RETURN jsonb_build_object('status','applied','aclRevision',rev);
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(11,now());
COMMIT;
