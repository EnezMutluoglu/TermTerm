BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Only current folder names needed to reach a readable child are exposed.
-- No sibling records, counts, folder history, or editing rights are granted.
CREATE FUNCTION navigation_folders_proposed(t uuid,a uuid,scope uuid,changes jsonb) RETURNS TABLE(folder_id uuid)
LANGUAGE sql STABLE AS $$
 WITH RECURSIVE ancestors(id,depth) AS (
  SELECT resource_parent(t,r.id),1 FROM resources r
  WHERE r.team_id=t AND r.kind IN ('host','folder') AND NOT r.deleted
  AND allowed_proposed(t,a,r.id,'read',scope,changes)
  UNION
  SELECT resource_parent(t,x.id),x.depth+1 FROM ancestors x WHERE x.id IS NOT NULL AND x.depth<64
 ) SELECT DISTINCT f.id FROM ancestors x JOIN folders f ON f.team_id=t AND f.id=x.id
 JOIN resources r ON (r.team_id,r.id)=(f.team_id,f.id) WHERE NOT r.deleted
$$;
CREATE FUNCTION navigation_needed_proposed(t uuid,a uuid,r uuid,scope uuid,changes jsonb) RETURNS boolean
LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN EXISTS(SELECT FROM folders WHERE team_id=t AND id=r)
 THEN EXISTS(SELECT FROM navigation_folders_proposed(t,a,scope,changes)n WHERE n.folder_id=r) ELSE false END
$$;
CREATE FUNCTION navigation_needed(t uuid,a uuid,r uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT navigation_needed_proposed(t,a,r,NULL,'[]')
$$;
CREATE OR REPLACE FUNCTION shared_recipients(t uuid,r uuid) RETURNS TABLE(account_id uuid,identity_version bigint,public_key bytea)
LANGUAGE sql STABLE AS $$
 SELECT a.id,i.version,i.public_key FROM team_accounts a JOIN account_crypto_identities i
 ON (i.team_id,i.account_id)=(a.team_id,a.id) AND i.active
 WHERE a.team_id=t AND a.status='active' AND (allowed(t,a.id,r,'read') OR allowed(t,a.id,r,'connect') OR allowed(t,a.id,r,'edit') OR navigation_needed(t,a.id,r))
$$;
CREATE OR REPLACE FUNCTION api_resource_recipients(token text,v uuid,r uuid,parent uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; scope uuid; rev bigint; recipients jsonb;cap uuid;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT EXISTS(SELECT FROM vaults WHERE team_id=s.team_id AND id=v) THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 IF EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=r AND vault_id=v AND NOT deleted) THEN scope:=r;
 ELSE
  IF EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=r) THEN RAISE EXCEPTION 'invalid_resource' USING ERRCODE='22023'; END IF;
  scope:=parent;
  IF parent IS NOT NULL AND NOT EXISTS(SELECT FROM folders f JOIN resources x ON (x.team_id,x.id)=(f.team_id,f.id)
  WHERE f.team_id=s.team_id AND f.vault_id=v AND f.id=parent AND NOT x.deleted) THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 END IF;
 IF scope IS NULL THEN PERFORM require_owner(s);
 ELSE IF NOT allowed(s.team_id,s.account_id,scope,'edit') THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF; END IF;
 SELECT acl_revision INTO rev FROM teams WHERE id=s.team_id;
 SELECT coalesce(jsonb_agg(jsonb_build_object('accountId',a.id,'identityVersion',i.version,'publicKey',encode(i.public_key,'hex')) ORDER BY a.id),'[]') INTO recipients
 FROM team_accounts a JOIN account_crypto_identities i ON (i.team_id,i.account_id)=(a.team_id,a.id) AND i.active
 WHERE a.team_id=s.team_id AND a.status='active' AND
 ((scope IS NOT NULL AND (allowed(s.team_id,a.id,scope,'read') OR allowed(s.team_id,a.id,scope,'connect') OR allowed(s.team_id,a.id,scope,'edit') OR (scope=r AND navigation_needed(s.team_id,a.id,r))))
 OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=a.id AND role='owner'));
 INSERT INTO resource_edit_receipts(team_id,account_id,device_id,vault_id,resource_id,acl_revision)
 VALUES(s.team_id,s.account_id,s.device_id,v,r,rev) RETURNING id INTO cap;
 RETURN jsonb_build_object('aclRevision',rev,'recipients',recipients,'writeCapability',cap);
END $$;
CREATE OR REPLACE FUNCTION api_acl_prepare(token text,target uuid,scope uuid,changes jsonb) RETURNS jsonb
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
CREATE OR REPLACE FUNCTION api_resources(token text,v uuid,after_id uuid,page_size integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; result jsonb;count_rows integer;
BEGIN
 s:=session_actor(token);
 IF page_size IS NULL OR page_size NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_page_size' USING ERRCODE='22023'; END IF;
 WITH candidates AS (
  SELECT x.id,octet_length(rv.encrypted_snapshot)*2+coalesce(octet_length(f.encrypted_name),0)*2
   +(coalesce(octet_length(h.encrypted_name),0)+coalesce(octet_length(h.encrypted_address),0)+coalesce(octet_length(h.encrypted_username),0))*2+16384 AS bytes
  FROM resources x JOIN resource_versions rv ON (rv.team_id,rv.resource_id,rv.revision)=(x.team_id,x.id,x.revision)
  LEFT JOIN folders f ON (f.team_id,f.id)=(x.team_id,x.id) LEFT JOIN hosts h ON (h.team_id,h.id)=(x.team_id,x.id)
  WHERE x.team_id=s.team_id AND x.vault_id=v AND x.kind IN ('folder','host') AND NOT x.deleted
   AND (after_id IS NULL OR x.id>after_id) AND (allowed(s.team_id,s.account_id,x.id,'read') OR navigation_needed(s.team_id,s.account_id,x.id)) ORDER BY x.id LIMIT page_size
 ), budget AS (SELECT id,sum(bytes) OVER (ORDER BY id) AS bytes FROM candidates)
 SELECT coalesce(jsonb_agg(shared_resource_json(s.team_id,s.account_id,id) || jsonb_build_object('navigationOnly',NOT allowed(s.team_id,s.account_id,id,'read')) ORDER BY id),'[]'),count(*) INTO result,count_rows FROM budget WHERE bytes<=2097152;
 -- Always supply the last ID when nonempty; an empty next page ends iteration.
 -- This also works when the byte budget, rather than row limit, ends the page.
 RETURN jsonb_build_object('records',result,'nextAfter',CASE WHEN count_rows>0 THEN result->(count_rows-1)->>'id' ELSE NULL END);
END $$;
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
-- Old pending previews do not describe the newly required navigation key set.
UPDATE acl_change_plans SET status='cancelled' WHERE status='pending';
DELETE FROM acl_plan_keys k USING acl_change_plans p WHERE (k.team_id,k.plan_id)=(p.team_id,p.id) AND p.status='cancelled';
UPDATE teams SET acl_revision=acl_revision+1;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(14,now());
COMMIT;