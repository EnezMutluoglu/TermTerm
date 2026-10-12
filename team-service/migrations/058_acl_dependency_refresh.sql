BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
ALTER TABLE acl_change_plans ADD COLUMN route_hash bytea;
ALTER TABLE acl_plan_keys DROP CONSTRAINT acl_plan_keys_kind_check;
ALTER TABLE acl_plan_keys DROP CONSTRAINT acl_plan_keys_team_id_plan_id_kind_object_id_revision_key;
ALTER TABLE acl_plan_keys ADD COLUMN root_id uuid,ADD COLUMN source_dependency boolean NOT NULL DEFAULT false;
ALTER TABLE acl_plan_keys ADD CONSTRAINT acl_key_kind CHECK(kind IN ('resource','vault','dependency'));
ALTER TABLE acl_plan_keys ADD CONSTRAINT acl_key_route CHECK((kind='dependency' AND root_id IS NOT NULL) OR (kind<>'dependency' AND root_id IS NULL AND NOT source_dependency));
ALTER TABLE acl_plan_keys ADD FOREIGN KEY(team_id,root_id) REFERENCES hosts(team_id,id);
CREATE UNIQUE INDEX acl_key_object ON acl_plan_keys(team_id,plan_id,kind,object_id,revision) WHERE kind<>'dependency';
CREATE UNIQUE INDEX acl_key_dependency ON acl_plan_keys(team_id,plan_id,root_id,object_id,revision) WHERE kind='dependency';
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
 UPDATE acl_change_plans SET route_hash=move_route_hash(s.team_id) WHERE team_id=s.team_id AND id=plan;
 INSERT INTO acl_plan_keys(team_id,plan_id,ordinal,kind,object_id,revision,source_envelope,root_id,source_dependency)
 SELECT s.team_id,plan,total+row_number() OVER(ORDER BY h.host_id,h.position),'dependency',h.jump_host_id,j.revision,
 coalesce(e.envelope,d.envelope),h.host_id,e.envelope IS NULL
 FROM host_hops h JOIN resources root ON (root.team_id,root.id)=(h.team_id,h.host_id)
 JOIN resources j ON (j.team_id,j.id)=(h.team_id,h.jump_host_id) AND NOT j.deleted
 LEFT JOIN resource_key_envelopes e ON (e.team_id,e.resource_id,e.revision,e.account_id,e.identity_version)=(j.team_id,j.id,j.revision,s.account_id,av)
 LEFT JOIN connection_dependency_envelopes d ON (d.team_id,d.host_id,d.jump_host_id,d.resource_revision,d.account_id,d.identity_version)=(h.team_id,h.host_id,h.jump_host_id,j.revision,s.account_id,av)
 WHERE root.team_id=s.team_id AND root.vault_id=v AND NOT root.deleted AND within_scope(s.team_id,root.id,scope)
 AND allowed_proposed(s.team_id,target,root.id,'connect',scope,changes)
 AND NOT EXISTS(SELECT FROM connection_dependency_envelopes x WHERE (x.team_id,x.host_id,x.jump_host_id,x.resource_revision,x.account_id,x.identity_version)=(h.team_id,h.host_id,h.jump_host_id,j.revision,target,tv));
 SELECT count(*) INTO total FROM acl_plan_keys WHERE team_id=s.team_id AND plan_id=plan;
 IF total>100000 THEN RAISE EXCEPTION 'scope_too_large' USING ERRCODE='22023'; END IF;
 RETURN jsonb_build_object('id',plan,'affectedResources',affected,'requiredKeys',total,'changes',changes,'expiresInSeconds',1800);
END $$;
CREATE OR REPLACE FUNCTION api_acl_keys(token text,plan uuid,after_index bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE p acl_change_plans;items jsonb;
BEGIN
 p:=acl_plan_actor(token,plan);
 IF p.status<>'pending' OR p.expires_at<=now() OR after_index IS NULL OR after_index<0 THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='22023'; END IF;
 SELECT coalesce(jsonb_agg(jsonb_build_object('ordinal',ordinal,'kind',kind,'id',object_id,'revision',revision,'rootId',root_id,'sourceDependency',source_dependency,'envelope',encode(source_envelope,'hex'),'staged',target_envelope IS NOT NULL) ORDER BY ordinal),'[]')
 INTO items FROM (SELECT * FROM acl_plan_keys WHERE team_id=p.team_id AND plan_id=plan AND ordinal>after_index ORDER BY ordinal LIMIT 20)k;
 RETURN jsonb_build_object('keys',items,'nextAfter',CASE WHEN jsonb_array_length(items)>0 THEN items->(jsonb_array_length(items)-1)->'ordinal' END,
 'targetAccountId',p.account_id,'targetIdentity',p.target_identity,'publicKey',(SELECT encode(public_key,'hex') FROM account_crypto_identities WHERE team_id=p.team_id AND account_id=p.account_id AND version=p.target_identity));
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
 IF p.route_hash IS DISTINCT FROM move_route_hash(p.team_id) OR p.expires_at<=now() OR p.acl_revision<>(SELECT acl_revision FROM teams WHERE id=p.team_id)
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
 INSERT INTO connection_dependency_envelopes(team_id,host_id,jump_host_id,resource_revision,account_id,identity_version,envelope)
 SELECT p.team_id,root_id,object_id,revision,p.account_id,p.target_identity,target_envelope FROM acl_plan_keys
 WHERE team_id=p.team_id AND plan_id=plan AND kind='dependency'
 ON CONFLICT(team_id,host_id,jump_host_id,account_id) DO UPDATE SET resource_revision=excluded.resource_revision,identity_version=excluded.identity_version,envelope=excluded.envelope;
 DELETE FROM connection_dependency_envelopes WHERE team_id=p.team_id AND account_id=p.account_id AND NOT allowed(p.team_id,p.account_id,host_id,'connect');
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
INSERT INTO schema_migrations(version) VALUES(58);
COMMIT;
