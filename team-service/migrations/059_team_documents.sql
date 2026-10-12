BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Typed payloads stay encrypted; membership and folder relations are normalized.
CREATE TABLE snippets (
 team_id uuid NOT NULL,vault_id uuid NOT NULL,id uuid NOT NULL,folder_id uuid,
 encrypted_name bytea NOT NULL CHECK(octet_length(encrypted_name) BETWEEN 40 AND 4096),
 encrypted_body bytea NOT NULL CHECK(octet_length(encrypted_body) BETWEEN 40 AND 65536),
 PRIMARY KEY(team_id,id),UNIQUE(team_id,vault_id,id),
 FOREIGN KEY(team_id,vault_id,id) REFERENCES resources(team_id,vault_id,id),
 FOREIGN KEY(team_id,vault_id,folder_id) REFERENCES folders(team_id,vault_id,id)
);
CREATE TABLE workspaces (LIKE snippets INCLUDING ALL);
ALTER TABLE workspaces ADD FOREIGN KEY(team_id,vault_id,id) REFERENCES resources(team_id,vault_id,id);
ALTER TABLE workspaces ADD FOREIGN KEY(team_id,vault_id,folder_id) REFERENCES folders(team_id,vault_id,id);
CREATE TABLE workspace_entries (
 team_id uuid NOT NULL,workspace_id uuid NOT NULL,position smallint NOT NULL CHECK(position BETWEEN 0 AND 15),host_id uuid NOT NULL,
 PRIMARY KEY(team_id,workspace_id,position),UNIQUE(team_id,workspace_id,host_id),
 FOREIGN KEY(team_id,workspace_id) REFERENCES workspaces(team_id,id),
 FOREIGN KEY(team_id,host_id) REFERENCES hosts(team_id,id)
);
CREATE INDEX snippet_folder ON snippets(team_id,vault_id,folder_id);
CREATE INDEX workspace_folder ON workspaces(team_id,vault_id,folder_id);
CREATE INDEX workspace_target ON workspace_entries(team_id,host_id);
ALTER TABLE snippets ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspaces ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_entries ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON snippets,workspaces,workspace_entries FROM PUBLIC;
CREATE TRIGGER snippet_kind BEFORE INSERT OR UPDATE ON snippets FOR EACH ROW EXECUTE FUNCTION resource_kind_guard('snippet');
CREATE TRIGGER workspace_kind BEFORE INSERT OR UPDATE ON workspaces FOR EACH ROW EXECUTE FUNCTION resource_kind_guard('workspace');
CREATE OR REPLACE FUNCTION resource_parent(t uuid,r uuid) RETURNS uuid LANGUAGE sql STABLE AS $$
 SELECT parent_id FROM folders WHERE team_id=t AND id=r
 UNION ALL SELECT folder_id FROM hosts WHERE team_id=t AND id=r
 UNION ALL SELECT folder_id FROM snippets WHERE team_id=t AND id=r
 UNION ALL SELECT folder_id FROM workspaces WHERE team_id=t AND id=r
$$;

CREATE OR REPLACE FUNCTION allowed(t uuid,a uuid,r uuid,p text) RETURNS boolean LANGUAGE sql STABLE AS $$
 WITH RECURSIVE ancestry(id,depth) AS (
  SELECT r,0 UNION ALL
  SELECT resource_parent(t,x.id),x.depth+1 FROM ancestry x
  WHERE x.depth<64 AND resource_parent(t,x.id) IS NOT NULL
 ), rules AS (
  SELECT effect FROM resource_acl acl JOIN ancestry x ON x.id=acl.resource_id
  WHERE acl.team_id=t AND acl.account_id=a AND acl.permission=p
 ) SELECT EXISTS(SELECT FROM team_accounts WHERE team_id=t AND id=a AND status='active')
 AND EXISTS(SELECT FROM resources WHERE team_id=t AND id=r AND NOT deleted)
 AND (EXISTS(SELECT FROM team_memberships WHERE team_id=t AND account_id=a AND role='owner')
 OR (NOT EXISTS(SELECT FROM rules WHERE effect='deny') AND EXISTS(SELECT FROM rules WHERE effect='allow')))
$$;
CREATE OR REPLACE FUNCTION allowed_proposed(t uuid,a uuid,r uuid,p text,scope uuid,changes jsonb) RETURNS boolean LANGUAGE sql STABLE AS $$
 WITH RECURSIVE ancestry(id,depth) AS (
  SELECT r,0 UNION ALL
  SELECT resource_parent(t,x.id),x.depth+1 FROM ancestry x
  WHERE x.depth<64 AND resource_parent(t,x.id) IS NOT NULL
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
CREATE OR REPLACE FUNCTION navigation_folders_proposed(t uuid,a uuid,scope uuid,changes jsonb) RETURNS TABLE(folder_id uuid)
LANGUAGE sql STABLE AS $$
 WITH RECURSIVE ancestors(id,depth) AS (
  SELECT resource_parent(t,r.id),1 FROM resources r
  WHERE r.team_id=t AND r.kind IN ('host','folder','snippet','workspace') AND NOT r.deleted
  AND allowed_proposed(t,a,r.id,'read',scope,changes)
  UNION
  SELECT resource_parent(t,x.id),x.depth+1 FROM ancestors x WHERE x.id IS NOT NULL AND x.depth<64
 ) SELECT DISTINCT f.id FROM ancestors x JOIN folders f ON f.team_id=t AND f.id=x.id
 JOIN resources r ON (r.team_id,r.id)=(f.team_id,f.id) WHERE NOT r.deleted
$$;
CREATE OR REPLACE FUNCTION api_resources(token text,v uuid,after_id uuid,page_size integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; result jsonb;count_rows integer;
BEGIN
 s:=session_actor(token);
 IF page_size IS NULL OR page_size NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_page_size' USING ERRCODE='22023'; END IF;
 WITH candidates AS (
  SELECT x.id,octet_length(rv.encrypted_snapshot)*4+coalesce(octet_length(f.encrypted_name),0)*2
   +(coalesce(octet_length(h.encrypted_name),0)+coalesce(octet_length(h.encrypted_address),0)+coalesce(octet_length(h.encrypted_username),0))*2+16384 AS bytes
  FROM resources x JOIN resource_versions rv ON (rv.team_id,rv.resource_id,rv.revision)=(x.team_id,x.id,x.revision)
  LEFT JOIN folders f ON (f.team_id,f.id)=(x.team_id,x.id) LEFT JOIN hosts h ON (h.team_id,h.id)=(x.team_id,x.id)
  WHERE x.team_id=s.team_id AND x.vault_id=v AND x.kind IN ('folder','host','snippet','workspace') AND NOT x.deleted
   AND (after_id IS NULL OR x.id>after_id) AND (allowed(s.team_id,s.account_id,x.id,'read') OR navigation_needed(s.team_id,s.account_id,x.id)) ORDER BY x.id LIMIT page_size
 ), budget AS (SELECT id,sum(bytes) OVER (ORDER BY id) AS bytes FROM candidates)
 SELECT coalesce(jsonb_agg(shared_resource_json(s.team_id,s.account_id,id) || jsonb_build_object('navigationOnly',NOT allowed(s.team_id,s.account_id,id,'read')) ORDER BY id),'[]'),count(*) INTO result,count_rows FROM budget WHERE bytes<=2097152;
 -- Always supply the last ID when nonempty; an empty next page ends iteration.
 -- This also works when the byte budget, rather than row limit, ends the page.
 RETURN jsonb_build_object('records',result,'nextAfter',CASE WHEN count_rows>0 THEN result->(count_rows-1)->>'id' ELSE NULL END);
END $$;
CREATE OR REPLACE FUNCTION api_delegation_set(token text,target uuid,scope uuid,permissions jsonb,expected bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;rev bigint;event_id bigint;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT acl_revision INTO rev FROM teams WHERE id=s.team_id;
 IF expected IS NULL OR expected<>rev THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001'; END IF;
 IF target=s.account_id OR NOT EXISTS(SELECT FROM team_accounts a JOIN team_memberships m ON (m.team_id,m.account_id)=(a.team_id,a.id)
 WHERE a.team_id=s.team_id AND a.id=target AND m.role='manager') THEN RAISE EXCEPTION 'invalid_member' USING ERRCODE='22023'; END IF;
 IF NOT EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=scope AND NOT deleted AND kind IN ('host','folder','snippet','workspace'))
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
 SELECT vault_id INTO v FROM resources WHERE team_id=s.team_id AND id=scope AND kind IN ('folder','host','snippet','workspace') AND NOT deleted;
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
CREATE OR REPLACE FUNCTION api_move_prepare(token text,r uuid,d uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p resource_move_plans;x resources;av bigint;n bigint;recipient_rows jsonb;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO x FROM resources WHERE team_id=s.team_id AND id=r AND NOT deleted AND kind IN ('folder','host','snippet','workspace');
 IF x.id IS NULL OR (d IS NOT NULL AND NOT EXISTS(SELECT FROM folders f JOIN resources rr ON (rr.team_id,rr.id)=(f.team_id,f.id) WHERE f.team_id=s.team_id AND f.vault_id=x.vault_id AND f.id=d AND NOT rr.deleted))
 THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 IF r=d OR (d IS NOT NULL AND within_scope(s.team_id,d,r)) THEN RAISE EXCEPTION 'folder_cycle' USING ERRCODE='23514'; END IF;
 IF resource_parent(s.team_id,r) IS NOT DISTINCT FROM d THEN RAISE EXCEPTION 'same_parent' USING ERRCODE='22023'; END IF;
 PERFORM require_move_scope(s,r,d);
 SELECT version INTO av FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND active;
 IF (SELECT count(*) FROM resources WHERE team_id=s.team_id AND vault_id=x.vault_id AND within_scope(s.team_id,id,r))
 * (SELECT count(*) FROM team_accounts WHERE team_id=s.team_id AND status='active')>100000 THEN RAISE EXCEPTION 'scope_too_large' USING ERRCODE='22023'; END IF;
 DELETE FROM resource_move_plans WHERE team_id=s.team_id AND actor_id=s.account_id AND expires_at<now()-interval '1 day';
 IF (SELECT count(*) FROM resource_move_plans WHERE team_id=s.team_id AND actor_id=s.account_id AND status='pending' AND expires_at>now())>=20 THEN RAISE EXCEPTION 'too_many_plans' USING ERRCODE='22023'; END IF;
 INSERT INTO resource_move_plans(team_id,actor_id,vault_id,resource_id,destination_id,source_revision,acl_revision,vault_revision,identity_hash,route_hash)
 SELECT s.team_id,s.account_id,x.vault_id,r,d,x.revision,t.acl_revision,v.revision,move_identity_hash(s.team_id),move_route_hash(s.team_id)
 FROM teams t JOIN vaults v ON v.team_id=t.id AND v.id=x.vault_id WHERE t.id=s.team_id RETURNING * INTO p;
 INSERT INTO move_access_changes
 SELECT s.team_id,p.id,z.id,a.id,
 ARRAY(SELECT permission FROM unnest(ARRAY['read','connect','edit','reveal','export','manage'])permission WHERE allowed(s.team_id,a.id,z.id,permission)),
 ARRAY(SELECT permission FROM unnest(ARRAY['read','connect','edit','reveal','export','manage'])permission WHERE allowed_moved(s.team_id,a.id,z.id,permission,r,d))
 FROM resources z CROSS JOIN team_accounts a WHERE z.team_id=s.team_id AND z.vault_id=x.vault_id AND NOT z.deleted AND within_scope(s.team_id,z.id,r)
 AND a.team_id=s.team_id AND a.status='active';
 DELETE FROM move_access_changes WHERE team_id=s.team_id AND plan_id=p.id AND before_permissions=after_permissions;
 -- Managers cannot redistribute secret/export/management rights through a move.
 IF NOT EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner') AND EXISTS(
 SELECT FROM move_access_changes c CROSS JOIN unnest(ARRAY['reveal','export','manage'])permission WHERE c.team_id=s.team_id AND c.plan_id=p.id
 AND (permission=ANY(c.before_permissions)) IS DISTINCT FROM (permission=ANY(c.after_permissions))) THEN RAISE EXCEPTION 'delegation_scope_denied' USING ERRCODE='42501'; END IF;
 INSERT INTO move_plan_keys(team_id,plan_id,ordinal,kind,object_id,revision,account_id,identity_version,source_envelope)
 SELECT s.team_id,p.id,row_number() OVER(ORDER BY v.resource_id,v.revision,i.account_id),'resource',v.resource_id,v.revision,i.account_id,i.version,e.envelope
 FROM resources z JOIN resource_versions v ON (v.team_id,v.resource_id)=(z.team_id,z.id)
 JOIN account_crypto_identities i ON i.team_id=z.team_id AND i.active JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id) AND a.status='active'
 LEFT JOIN resource_key_envelopes e ON (e.team_id,e.resource_id,e.revision,e.account_id,e.identity_version)=(v.team_id,v.resource_id,v.revision,s.account_id,av)
 WHERE z.team_id=s.team_id AND z.vault_id=x.vault_id AND NOT z.deleted AND within_scope(s.team_id,z.id,r)
 AND EXISTS(SELECT permission FROM unnest(ARRAY['read','connect','edit'])permission WHERE allowed_moved(s.team_id,i.account_id,z.id,permission,r,d))
 AND NOT EXISTS(SELECT FROM resource_key_envelopes k WHERE (k.team_id,k.resource_id,k.revision,k.account_id,k.identity_version)=(v.team_id,v.resource_id,v.revision,i.account_id,i.version));
 GET DIAGNOSTICS n=ROW_COUNT;
 INSERT INTO move_plan_keys(team_id,plan_id,ordinal,kind,object_id,revision,account_id,identity_version,source_envelope)
 SELECT s.team_id,p.id,n+row_number() OVER(ORDER BY i.account_id),'vault',x.vault_id,0,i.account_id,i.version,e.envelope
 FROM account_crypto_identities i JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id) AND a.status='active'
 LEFT JOIN vault_key_envelopes e ON (e.team_id,e.vault_id,e.account_id,e.identity_version)=(s.team_id,x.vault_id,s.account_id,av)
 WHERE i.team_id=s.team_id AND i.active AND EXISTS(SELECT FROM resources z WHERE z.team_id=s.team_id AND z.vault_id=x.vault_id AND
 EXISTS(SELECT permission FROM unnest(ARRAY['read','connect','edit'])permission WHERE allowed_moved(s.team_id,i.account_id,z.id,permission,r,d)))
 AND NOT EXISTS(SELECT FROM vault_key_envelopes k WHERE (k.team_id,k.vault_id,k.account_id,k.identity_version)=(s.team_id,x.vault_id,i.account_id,i.version));
 SELECT coalesce(max(ordinal),0) INTO n FROM move_plan_keys WHERE team_id=s.team_id AND plan_id=p.id;
 -- Rebuild affected route envelopes, including a moved host used by another root.
 INSERT INTO move_plan_keys(team_id,plan_id,ordinal,kind,object_id,revision,root_id,account_id,identity_version,source_envelope,fresh_source)
 SELECT s.team_id,p.id,n+row_number() OVER(ORDER BY h.host_id,h.jump_host_id,i.account_id),'dependency',h.jump_host_id,z.revision+CASE WHEN h.jump_host_id=r THEN 1 ELSE 0 END,h.host_id,i.account_id,i.version,e.envelope,h.jump_host_id=r
 FROM host_hops h JOIN resources z ON (z.team_id,z.id)=(h.team_id,h.jump_host_id)
 JOIN account_crypto_identities i ON i.team_id=h.team_id AND i.active JOIN team_accounts a ON (a.team_id,a.id)=(i.team_id,i.account_id) AND a.status='active'
 LEFT JOIN resource_key_envelopes e ON (e.team_id,e.resource_id,e.revision,e.account_id,e.identity_version)=(z.team_id,z.id,z.revision,s.account_id,av)
 WHERE h.team_id=s.team_id AND (within_scope(s.team_id,h.host_id,r) OR h.jump_host_id=r)
 AND allowed_moved(s.team_id,i.account_id,h.host_id,'connect',r,d);
 IF EXISTS(SELECT FROM move_plan_keys WHERE team_id=s.team_id AND plan_id=p.id AND source_envelope IS NULL AND NOT fresh_source)
 THEN RAISE EXCEPTION 'source_keys_unavailable' USING ERRCODE='42501'; END IF;
 IF (SELECT count(*) FROM move_plan_keys WHERE team_id=s.team_id AND plan_id=p.id)>100000 THEN RAISE EXCEPTION 'scope_too_large' USING ERRCODE='22023'; END IF;
 SELECT coalesce(jsonb_agg(jsonb_build_object('accountId',i.account_id,'identityVersion',i.version,'publicKey',encode(i.public_key,'hex')) ORDER BY i.account_id),'[]') INTO recipient_rows
 FROM account_crypto_identities i WHERE i.team_id=s.team_id AND i.active AND EXISTS(SELECT permission FROM unnest(ARRAY['read','connect','edit'])permission WHERE allowed_moved(s.team_id,i.account_id,r,permission,r,d));
 RETURN jsonb_build_object('planId',p.id,'vaultId',x.vault_id,'destinationId',d,'resource',shared_resource_json(s.team_id,s.account_id,r),
 'aclRevision',p.acl_revision,'recipients',recipient_rows,'requiredKeys',(SELECT count(*) FROM move_plan_keys WHERE team_id=s.team_id AND plan_id=p.id),
 'affectedAccess',(SELECT count(*) FROM move_access_changes WHERE team_id=s.team_id AND plan_id=p.id),'expiresInSeconds',900);
END $$;
CREATE OR REPLACE FUNCTION api_offline_decision_submit(token text,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;cap resource_edit_receipts;op change_operations;op_id uuid:=(body->>'operationId')::uuid;
 r uuid:=(body->>'resourceId')::uuid;v uuid:=(body->>'vaultId')::uuid;parent uuid:=(body->>'parentId')::uuid;
 expected bigint:=(body->>'expectedRevision')::bigint;seq bigint:=(body->>'sequence')::bigint;
 snapshot bytea:=decode(body->>'snapshot','hex');envelopes jsonb:=body->'envelopes';
 request_hash bytea:=public.digest(body::text,'sha256');decision uuid;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=r)
 AND NOT EXISTS(SELECT FROM creation_decisions WHERE team_id=s.team_id AND operation_id=op_id)
 THEN RETURN existing_offline_decision_submit(token,body); END IF;
 IF op_id IS NULL OR r IS NULL OR v IS NULL OR expected IS NULL OR expected<0 OR expected=9223372036854775807
 OR seq IS NULL OR seq<=0 OR snapshot IS NULL OR octet_length(snapshot) NOT BETWEEN 40 AND 131072
 OR (body->>'clientAt')::timestamptz IS NULL OR body->>'kind' IS NULL OR body->>'kind' NOT IN ('host','folder','snippet','workspace')
 THEN RAISE EXCEPTION 'invalid_operation' USING ERRCODE='22023'; END IF;
 SELECT * INTO op FROM change_operations WHERE team_id=s.team_id AND id=op_id;
 IF op.id IS NOT NULL THEN
  IF op.account_id<>s.account_id OR op.device_id<>s.device_id OR op.request_hash<>request_hash
  THEN RAISE EXCEPTION 'operation_reuse' USING ERRCODE='22023'; END IF;
  RETURN jsonb_build_object('operationId',op.id,'status',op.status,'revision',op.result_revision,
   'conflictId',(SELECT id FROM creation_decisions WHERE team_id=s.team_id AND operation_id=op.id));
 END IF;
 SELECT * INTO cap FROM resource_edit_receipts WHERE team_id=s.team_id AND id=(body->>'writeCapability')::uuid
 AND account_id=s.account_id AND device_id=s.device_id AND resource_id=r AND vault_id=v;
 IF cap.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF EXISTS(SELECT FROM offline_creation_claims c JOIN offline_creation_scopes p ON(p.team_id,p.id)=(c.team_id,c.scope_id)
 WHERE c.team_id=s.team_id AND c.receipt_id=cap.id AND p.parent_id IS DISTINCT FROM parent)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 IF (body->>'aclRevision')::bigint IS DISTINCT FROM (SELECT acl_revision FROM teams WHERE id=s.team_id)
 THEN RAISE EXCEPTION 'acl_changed' USING ERRCODE='40001'; END IF;
 IF jsonb_typeof(envelopes) IS DISTINCT FROM 'array' OR jsonb_array_length(envelopes) NOT BETWEEN 1 AND 1000
 THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(envelopes)<>(SELECT count(*) FROM offline_decision_recipients(s.team_id,r))
 OR EXISTS(SELECT FROM jsonb_array_elements(envelopes)e WHERE NOT EXISTS(SELECT FROM offline_decision_recipients(s.team_id,r) current
 WHERE current.account_id=(e->>'accountId')::uuid AND current.identity_version=(e->>'identityVersion')::bigint))
 OR EXISTS(SELECT FROM jsonb_array_elements(envelopes)e GROUP BY e->>'accountId' HAVING count(*)>1)
 THEN RAISE EXCEPTION 'recipients_changed' USING ERRCODE='40001'; END IF;
 INSERT INTO change_operations(team_id,id,account_id,device_id,sequence,resource_id,expected_revision,request_hash,status,client_at)
 VALUES(s.team_id,op_id,s.account_id,s.device_id,seq,NULL,expected,request_hash,'conflict',(body->>'clientAt')::timestamptz);
 INSERT INTO creation_decisions(team_id,operation_id,resource_id,vault_id,parent_id,encrypted_candidate)
 VALUES(s.team_id,op_id,r,v,parent,snapshot) RETURNING id INTO decision;
 INSERT INTO creation_decision_envelopes SELECT s.team_id,decision,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,decode(e->>'envelope','hex') FROM jsonb_array_elements(envelopes)e;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'offline.creation_received','succeeded');
 RETURN jsonb_build_object('operationId',op_id,'status','conflict','conflictId',decision);
END $$;
CREATE OR REPLACE FUNCTION shared_resource_json(t uuid,a uuid,r uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',x.id,'vaultId',x.vault_id,'kind',x.kind,'revision',x.revision,'parentId',resource_parent(t,r),
 'deleted',x.deleted,'encryptedName',encode(coalesce(f.encrypted_name,h.encrypted_name,sn.encrypted_name,ws.encrypted_name),'hex'),'encryptedAddress',encode(h.encrypted_address,'hex'),
 'encryptedUsername',encode(h.encrypted_username,'hex'),'protocol',h.protocol,'port',h.port,'usernamePolicy',h.username_policy,
 'encryptedDocument',encode(coalesce(sn.encrypted_body,ws.encrypted_body),'hex'),'workspaceHostIds',CASE WHEN ws.id IS NOT NULL THEN (SELECT coalesce(jsonb_agg(we.host_id ORDER BY we.position),'[]') FROM workspace_entries we WHERE we.team_id=t AND we.workspace_id=r) ELSE NULL END,'snapshot',encode(rv.encrypted_snapshot,'hex'),'envelope',encode(e.envelope,'hex'),'identityVersion',e.identity_version,
 'permissions',(SELECT coalesce(jsonb_agg(p),'[]') FROM unnest(ARRAY['read','connect','edit','reveal','export','manage'])p WHERE allowed(t,a,r,p)))
 FROM resources x LEFT JOIN folders f ON (f.team_id,f.id)=(x.team_id,x.id) LEFT JOIN hosts h ON (h.team_id,h.id)=(x.team_id,x.id)
 LEFT JOIN snippets sn ON (sn.team_id,sn.id)=(x.team_id,x.id) LEFT JOIN workspaces ws ON (ws.team_id,ws.id)=(x.team_id,x.id)
 JOIN resource_versions rv ON (rv.team_id,rv.resource_id,rv.revision)=(x.team_id,x.id,x.revision)
 LEFT JOIN resource_key_envelopes e ON (e.team_id,e.resource_id,e.revision,e.account_id)=(x.team_id,x.id,x.revision,a)
 AND e.identity_version=(SELECT version FROM account_crypto_identities WHERE team_id=t AND account_id=a AND active)
 WHERE x.team_id=t AND x.id=r
$$;
CREATE OR REPLACE FUNCTION api_resource_save(token text,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; old resources; op change_operations; op_id uuid:=(body->>'operationId')::uuid;
 v uuid:=(body->>'vaultId')::uuid;r uuid:=(body->>'resourceId')::uuid;parent uuid:=(body->>'parentId')::uuid;
 expected bigint:=(body->>'expectedRevision')::bigint;seq bigint:=(body->>'sequence')::bigint;
 kind_input text:=body->>'kind';is_deleted boolean:=(body->>'deleted')::boolean;
 request_hash bytea:=public.digest(body::text,'sha256');rev bigint;vault_rev bigint;conflict_id uuid;reason text;has_edit boolean;
 snapshot bytea:=decode(body->>'snapshot','hex');envelopes jsonb:=body->'envelopes';
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF op_id IS NULL OR v IS NULL OR r IS NULL OR expected IS NULL OR expected<0 OR expected=9223372036854775807
 OR seq IS NULL OR seq<=0 OR is_deleted IS NULL OR kind_input IS NULL OR kind_input NOT IN ('folder','host','snippet','workspace')
 OR snapshot IS NULL OR octet_length(snapshot) NOT BETWEEN 40 AND 131072 OR (body->>'clientAt')::timestamptz IS NULL
 OR (body->>'aclRevision')::bigint IS NULL THEN RAISE EXCEPTION 'invalid_operation' USING ERRCODE='22023'; END IF;
 -- Pin account activation/revocation while checking and publishing recipients.
 -- Account activation/reset also locks this row before changing crypto identities.
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 SELECT * INTO op FROM change_operations WHERE team_id=s.team_id AND id=op_id;
 IF op.id IS NOT NULL THEN
  IF op.account_id<>s.account_id OR op.device_id<>s.device_id OR op.request_hash<>request_hash THEN RAISE EXCEPTION 'operation_reuse' USING ERRCODE='22023'; END IF;
  RETURN jsonb_build_object('operationId',op_id,'status',op.status,'revision',op.result_revision,
   'conflictId',(SELECT id FROM conflicts WHERE team_id=s.team_id AND operation_id=op_id));
 END IF;
 IF NOT EXISTS(SELECT FROM vaults WHERE team_id=s.team_id AND id=v) THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 SELECT * INTO old FROM resources WHERE team_id=s.team_id AND id=r FOR UPDATE;
 IF old.id IS NOT NULL AND (old.vault_id<>v OR old.kind<>kind_input) THEN RAISE EXCEPTION 'immutable_resource_type' USING ERRCODE='22023'; END IF;
 IF old.id IS NULL THEN
  IF expected<>0 OR is_deleted THEN RAISE EXCEPTION 'invalid_new_resource' USING ERRCODE='22023'; END IF;
  IF parent IS NULL THEN PERFORM require_owner(s);
  ELSIF NOT allowed(s.team_id,s.account_id,parent,'edit') THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  has_edit:=true;
 ELSE
  has_edit:=allowed(s.team_id,s.account_id,r,'edit');
  IF NOT has_edit THEN reason:='permission'; ELSIF old.revision<>expected THEN reason:='revision'; END IF;
 END IF;
 IF reason IS NOT NULL THEN
  IF reason='permission' AND NOT EXISTS(SELECT FROM resource_edit_receipts WHERE team_id=s.team_id AND resource_id=r AND vault_id=v AND account_id=s.account_id
   AND device_id=s.device_id AND id=(body->>'writeCapability')::uuid)
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  -- Keep the encrypted candidate without changing the record. This receipt is
  -- available to the author; it does not disclose current server data/revision.
  INSERT INTO change_operations(team_id,id,account_id,device_id,sequence,resource_id,expected_revision,request_hash,status,client_at)
  VALUES(s.team_id,op_id,s.account_id,s.device_id,seq,r,expected,request_hash,'conflict',(body->>'clientAt')::timestamptz);
  INSERT INTO conflicts(team_id,operation_id,resource_id,encrypted_candidate,reason) VALUES(s.team_id,op_id,r,snapshot,reason) RETURNING id INTO conflict_id;
  IF jsonb_typeof(envelopes) IS DISTINCT FROM 'array' OR jsonb_array_length(envelopes)>1000 THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023'; END IF;
  INSERT INTO conflict_key_envelopes(team_id,conflict_id,account_id,identity_version,envelope)
  SELECT s.team_id,conflict_id,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,decode(e->>'envelope','hex') FROM jsonb_array_elements(envelopes)e;
  RETURN jsonb_build_object('operationId',op_id,'status','conflict','conflictId',conflict_id);
 END IF;
 IF (body->>'aclRevision')::bigint<>(SELECT acl_revision FROM teams WHERE id=s.team_id) THEN RAISE EXCEPTION 'acl_changed' USING ERRCODE='40001'; END IF;
 IF old.id IS NOT NULL AND resource_parent(s.team_id,r) IS DISTINCT FROM parent THEN
  RAISE EXCEPTION 'move_requires_online_plan' USING ERRCODE='42501'; END IF;
 IF parent IS NOT NULL AND NOT EXISTS(SELECT FROM folders f JOIN resources x ON (x.team_id,x.id)=(f.team_id,f.id)
 WHERE f.team_id=s.team_id AND f.id=parent AND f.vault_id=v AND NOT x.deleted) THEN RAISE EXCEPTION 'invalid_parent' USING ERRCODE='23503'; END IF;
 IF is_deleted AND (EXISTS(SELECT FROM folders f JOIN resources x ON (x.team_id,x.id)=(f.team_id,f.id) WHERE f.team_id=s.team_id AND f.parent_id=r AND NOT x.deleted)
 OR EXISTS(SELECT FROM hosts h JOIN resources x ON (x.team_id,x.id)=(h.team_id,h.id) WHERE h.team_id=s.team_id AND h.folder_id=r AND NOT x.deleted)
 OR EXISTS(SELECT FROM snippets z JOIN resources q ON (q.team_id,q.id)=(z.team_id,z.id) WHERE z.team_id=s.team_id AND z.folder_id=r AND NOT q.deleted)
 OR EXISTS(SELECT FROM workspaces z JOIN resources q ON (q.team_id,q.id)=(z.team_id,z.id) WHERE z.team_id=s.team_id AND z.folder_id=r AND NOT q.deleted)
 OR EXISTS(SELECT FROM workspace_entries z JOIN resources q ON (q.team_id,q.id)=(z.team_id,z.workspace_id) WHERE z.team_id=s.team_id AND z.host_id=r AND NOT q.deleted)
 OR EXISTS(SELECT FROM host_hops WHERE team_id=s.team_id AND jump_host_id=r)
 OR EXISTS(SELECT FROM resource_links WHERE team_id=s.team_id AND target_id=r)) THEN RAISE EXCEPTION 'resource_in_use' USING ERRCODE='23503'; END IF;
 -- New clients refresh every dependent route with the same new resource key.
 -- Older clients omit this field; their stale envelopes remain unusable.
 IF body->'dependencyEnvelopes' IS NOT NULL AND body->'dependencyEnvelopes'<>'null'::jsonb THEN
  IF jsonb_typeof(body->'dependencyEnvelopes')<>'array' OR jsonb_array_length(body->'dependencyEnvelopes')>15000
  OR jsonb_array_length(body->'dependencyEnvelopes')<>jsonb_array_length(resource_dependency_recipients(s.team_id,r))
  OR EXISTS((SELECT x->>'rootId',x->>'accountId',x->>'identityVersion' FROM jsonb_array_elements(resource_dependency_recipients(s.team_id,r))x
   EXCEPT SELECT x->>'rootId',x->>'accountId',x->>'identityVersion' FROM jsonb_array_elements(body->'dependencyEnvelopes')x)
   UNION ALL (SELECT x->>'rootId',x->>'accountId',x->>'identityVersion' FROM jsonb_array_elements(body->'dependencyEnvelopes')x
   EXCEPT SELECT x->>'rootId',x->>'accountId',x->>'identityVersion' FROM jsonb_array_elements(resource_dependency_recipients(s.team_id,r))x))
  THEN RAISE EXCEPTION 'dependency_recipients_changed' USING ERRCODE='40001'; END IF;
 END IF;
 rev:=expected+1;
 INSERT INTO resources(team_id,vault_id,id,kind,revision) VALUES(s.team_id,v,r,kind_input,rev)
 ON CONFLICT(team_id,id) DO UPDATE SET revision=rev,modified_at=clock_timestamp();
 -- Writes to control columns are typed, never inferred from ciphertext or a generic document.
 IF kind_input='folder' THEN
  IF octet_length(decode(body->>'encryptedName','hex')) NOT BETWEEN 40 AND 4096 THEN RAISE EXCEPTION 'invalid_name' USING ERRCODE='22023'; END IF;
  INSERT INTO folders VALUES(s.team_id,v,r,parent,decode(body->>'encryptedName','hex'))
  ON CONFLICT(team_id,id) DO UPDATE SET encrypted_name=excluded.encrypted_name;
 ELSIF kind_input='host' THEN
  IF octet_length(decode(body->>'encryptedName','hex')) NOT BETWEEN 40 AND 4096
  OR octet_length(decode(body->>'encryptedAddress','hex')) NOT BETWEEN 40 AND 4096
  OR coalesce(octet_length(decode(body->>'encryptedUsername','hex')),40) NOT BETWEEN 40 AND 4096 THEN RAISE EXCEPTION 'invalid_host_data' USING ERRCODE='22023'; END IF;
  INSERT INTO hosts VALUES(s.team_id,v,r,parent,body->>'protocol',(body->>'port')::integer,body->>'usernamePolicy',
    decode(body->>'encryptedAddress','hex'),decode(body->>'encryptedName','hex'),decode(body->>'encryptedUsername','hex'))
  ON CONFLICT(team_id,id) DO UPDATE SET protocol=excluded.protocol,port=excluded.port,username_policy=excluded.username_policy,
    encrypted_address=excluded.encrypted_address,encrypted_name=excluded.encrypted_name,encrypted_username=excluded.encrypted_username;
 ELSE
  IF body->>'encryptedDocument' IS NULL OR octet_length(decode(body->>'encryptedDocument','hex')) NOT BETWEEN 40 AND 65536
  OR body->>'encryptedName' IS NULL OR octet_length(decode(body->>'encryptedName','hex')) NOT BETWEEN 40 AND 4096
  OR body->>'protocol' IS NOT NULL OR body->>'port' IS NOT NULL OR body->>'usernamePolicy' IS NOT NULL
  OR body->>'encryptedAddress' IS NOT NULL OR body->>'encryptedUsername' IS NOT NULL
  THEN RAISE EXCEPTION 'invalid_document' USING ERRCODE='22023'; END IF;
  IF kind_input='snippet' THEN
   IF body->'workspaceHostIds' IS NOT NULL AND body->'workspaceHostIds'<>'null'::jsonb THEN RAISE EXCEPTION 'invalid_snippet' USING ERRCODE='22023'; END IF;
   INSERT INTO snippets VALUES(s.team_id,v,r,parent,decode(body->>'encryptedName','hex'),decode(body->>'encryptedDocument','hex'))
   ON CONFLICT(team_id,id) DO UPDATE SET encrypted_name=excluded.encrypted_name,encrypted_body=excluded.encrypted_body;
  ELSE
   IF jsonb_typeof(body->'workspaceHostIds') IS DISTINCT FROM 'array' OR jsonb_array_length(body->'workspaceHostIds') NOT BETWEEN 1 AND 16
   OR (SELECT count(DISTINCT x) FROM jsonb_array_elements_text(body->'workspaceHostIds') x)<>jsonb_array_length(body->'workspaceHostIds')
   THEN RAISE EXCEPTION 'invalid_workspace' USING ERRCODE='22023'; END IF;
   IF NOT is_deleted AND EXISTS(SELECT FROM jsonb_array_elements_text(body->'workspaceHostIds') x
    WHERE NOT EXISTS(SELECT FROM hosts h JOIN resources q ON (q.team_id,q.id)=(h.team_id,h.id)
     WHERE h.team_id=s.team_id AND h.vault_id=v AND h.id=x::uuid AND h.protocol IN ('ssh','mosh') AND NOT q.deleted AND allowed(s.team_id,s.account_id,h.id,'connect')))
   THEN RAISE EXCEPTION 'workspace_target_denied' USING ERRCODE='42501'; END IF;
   INSERT INTO workspaces VALUES(s.team_id,v,r,parent,decode(body->>'encryptedName','hex'),decode(body->>'encryptedDocument','hex'))
   ON CONFLICT(team_id,id) DO UPDATE SET encrypted_name=excluded.encrypted_name,encrypted_body=excluded.encrypted_body;
   DELETE FROM workspace_entries WHERE team_id=s.team_id AND workspace_id=r;
   INSERT INTO workspace_entries SELECT s.team_id,r,(ordinality-1)::smallint,x::uuid
   FROM jsonb_array_elements_text(body->'workspaceHostIds') WITH ORDINALITY AS a(x,ordinality);
  END IF;
 END IF;
 PERFORM check_shared_envelopes(s.team_id,r,envelopes);
 INSERT INTO resource_versions(team_id,resource_id,revision,encrypted_snapshot,actor_id) VALUES(s.team_id,r,rev,snapshot,s.account_id);
 INSERT INTO resource_key_envelopes SELECT s.team_id,r,rev,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,decode(e->>'envelope','hex') FROM jsonb_array_elements(envelopes)e;
 IF body->'dependencyEnvelopes' IS NOT NULL AND body->'dependencyEnvelopes'<>'null'::jsonb THEN
  DELETE FROM connection_dependency_envelopes WHERE team_id=s.team_id AND jump_host_id=r;
  INSERT INTO connection_dependency_envelopes(team_id,host_id,jump_host_id,resource_revision,account_id,identity_version,envelope)
  SELECT s.team_id,(e->>'rootId')::uuid,r,rev,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,decode(e->>'envelope','hex')
  FROM jsonb_array_elements(body->'dependencyEnvelopes')e;
 END IF;
 UPDATE resources SET deleted=is_deleted WHERE team_id=s.team_id AND id=r;
 UPDATE vaults SET revision=revision+1 WHERE team_id=s.team_id AND id=v RETURNING revision INTO vault_rev;
 INSERT INTO change_operations(team_id,id,account_id,device_id,sequence,resource_id,expected_revision,request_hash,status,client_at,result_revision)
 VALUES(s.team_id,op_id,s.account_id,s.device_id,seq,r,expected,request_hash,'applied',(body->>'clientAt')::timestamptz,rev);
 DELETE FROM resource_versions rv WHERE rv.team_id=s.team_id AND rv.resource_id=r AND rv.revision<rev-10
 AND NOT EXISTS(SELECT FROM revision_pins p WHERE (p.team_id,p.resource_id,p.revision)=(rv.team_id,rv.resource_id,rv.revision));
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,CASE WHEN is_deleted THEN 'resource.delete' ELSE 'resource.save' END,r,'succeeded');
 RETURN jsonb_build_object('operationId',op_id,'status','applied','revision',rev,'vaultRevision',vault_rev);
END $$;
CREATE OR REPLACE FUNCTION api_move_finish(token text,plan uuid,body jsonb,cancel boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p resource_move_plans;answer jsonb;hashed bytea:=sha256(convert_to(body::text,'UTF8'));
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 p:=move_actor(token,plan);
 IF cancel IS NULL THEN RAISE EXCEPTION 'invalid_action' USING ERRCODE='22023'; END IF;
 IF p.status='applied' AND NOT cancel THEN
  IF p.request_hash IS DISTINCT FROM hashed THEN RAISE EXCEPTION 'operation_mismatch' USING ERRCODE='40001'; END IF;
  RETURN p.result;
 END IF;
 IF p.status<>'pending' THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='22023'; END IF;
 IF cancel THEN
  UPDATE resource_move_plans SET status='cancelled' WHERE team_id=s.team_id AND id=plan;
  DELETE FROM move_plan_keys WHERE team_id=s.team_id AND plan_id=plan;RETURN '{"status":"cancelled"}';
 END IF;
 IF p.expires_at<=now() OR p.acl_revision<>(SELECT acl_revision FROM teams WHERE id=s.team_id)
 OR p.vault_revision<>(SELECT revision FROM vaults WHERE team_id=s.team_id AND id=p.vault_id)
 OR p.identity_hash<>move_identity_hash(s.team_id) OR p.route_hash<>move_route_hash(s.team_id)
 OR p.source_revision<>(SELECT revision FROM resources WHERE team_id=s.team_id AND id=p.resource_id)
 THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001'; END IF;
 IF (body->>'resourceId')::uuid IS DISTINCT FROM p.resource_id OR (body->>'vaultId')::uuid IS DISTINCT FROM p.vault_id
 OR (body->>'parentId')::uuid IS DISTINCT FROM p.destination_id OR (body->>'expectedRevision')::bigint IS DISTINCT FROM p.source_revision
 OR (body->>'deleted')::boolean IS DISTINCT FROM false THEN RAISE EXCEPTION 'invalid_move' USING ERRCODE='22023'; END IF;
 IF EXISTS(SELECT FROM move_plan_keys WHERE team_id=s.team_id AND plan_id=plan AND target_envelope IS NULL) THEN RAISE EXCEPTION 'keys_incomplete' USING ERRCODE='23514'; END IF;
 UPDATE folders SET parent_id=p.destination_id WHERE team_id=s.team_id AND id=p.resource_id;
 UPDATE hosts SET folder_id=p.destination_id WHERE team_id=s.team_id AND id=p.resource_id;
 UPDATE snippets SET folder_id=p.destination_id WHERE team_id=s.team_id AND id=p.resource_id;
 UPDATE workspaces SET folder_id=p.destination_id WHERE team_id=s.team_id AND id=p.resource_id;
 INSERT INTO resource_key_envelopes SELECT team_id,object_id,revision,account_id,identity_version,target_envelope FROM move_plan_keys
 WHERE team_id=s.team_id AND plan_id=plan AND kind='resource' ON CONFLICT DO NOTHING;
 INSERT INTO vault_key_envelopes SELECT team_id,object_id,account_id,identity_version,target_envelope FROM move_plan_keys
 WHERE team_id=s.team_id AND plan_id=plan AND kind='vault' ON CONFLICT DO NOTHING;
 -- Existing writer enforces revisions, recipients and history; it may prune an
 -- old root version, cascading its staged historical envelopes in this transaction.
 answer:=api_resource_save(token,body);
 IF answer->>'status'<>'applied' THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001'; END IF;
 INSERT INTO connection_dependency_envelopes SELECT team_id,root_id,object_id,revision,account_id,identity_version,target_envelope FROM move_plan_keys
 WHERE team_id=s.team_id AND plan_id=plan AND kind='dependency'
 ON CONFLICT(team_id,host_id,jump_host_id,account_id) DO UPDATE SET resource_revision=excluded.resource_revision,identity_version=excluded.identity_version,envelope=excluded.envelope;
 DELETE FROM resource_key_envelopes e USING resources r WHERE (e.team_id,e.resource_id)=(r.team_id,r.id) AND r.team_id=s.team_id AND r.vault_id=p.vault_id
 AND NOT EXISTS(SELECT permission FROM unnest(ARRAY['read','connect','edit'])permission WHERE allowed(s.team_id,e.account_id,r.id,permission));
 DELETE FROM connection_dependency_envelopes WHERE team_id=s.team_id AND NOT allowed(s.team_id,account_id,host_id,'connect');
 DELETE FROM vault_key_envelopes e WHERE e.team_id=s.team_id AND e.vault_id=p.vault_id AND NOT EXISTS(SELECT FROM resources r WHERE r.team_id=s.team_id AND r.vault_id=p.vault_id
 AND EXISTS(SELECT permission FROM unnest(ARRAY['read','connect','edit'])permission WHERE allowed(s.team_id,e.account_id,r.id,permission)));
 UPDATE teams SET acl_revision=acl_revision+1 WHERE id=s.team_id;
 UPDATE resource_move_plans SET status='applied',request_hash=hashed,result=answer WHERE team_id=s.team_id AND id=plan;
 DELETE FROM move_plan_keys WHERE team_id=s.team_id AND plan_id=plan;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'resource.move',p.resource_id,'succeeded');
 RETURN answer;
END $$;
CREATE OR REPLACE FUNCTION api_offline_decision_submit(token text,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;cap resource_edit_receipts;op change_operations;op_id uuid:=(body->>'operationId')::uuid;
 r uuid:=(body->>'resourceId')::uuid;v uuid:=(body->>'vaultId')::uuid;parent uuid:=(body->>'parentId')::uuid;
 expected bigint:=(body->>'expectedRevision')::bigint;seq bigint:=(body->>'sequence')::bigint;
 snapshot bytea:=decode(body->>'snapshot','hex');envelopes jsonb:=body->'envelopes';
 request_hash bytea:=public.digest(body::text,'sha256');decision uuid;
BEGIN
 s:=candidate_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=r)
 AND NOT EXISTS(SELECT FROM creation_decisions WHERE team_id=s.team_id AND operation_id=op_id)
 THEN RETURN existing_offline_decision_submit(token,body); END IF;
 IF op_id IS NULL OR r IS NULL OR v IS NULL OR expected IS NULL OR expected<0 OR expected=9223372036854775807
 OR seq IS NULL OR seq<=0 OR snapshot IS NULL OR octet_length(snapshot) NOT BETWEEN 40 AND 131072
 OR (body->>'clientAt')::timestamptz IS NULL OR body->>'kind' IS NULL OR body->>'kind' NOT IN ('host','folder','snippet','workspace')
 THEN RAISE EXCEPTION 'invalid_operation' USING ERRCODE='22023'; END IF;
 SELECT * INTO op FROM change_operations WHERE team_id=s.team_id AND id=op_id;
 IF op.id IS NOT NULL THEN
  IF op.account_id<>s.account_id OR op.device_id<>s.device_id OR op.request_hash<>request_hash
  THEN RAISE EXCEPTION 'operation_reuse' USING ERRCODE='22023'; END IF;
  RETURN jsonb_build_object('operationId',op.id,'status',op.status,'revision',op.result_revision,
   'conflictId',(SELECT id FROM creation_decisions WHERE team_id=s.team_id AND operation_id=op.id));
 END IF;
 SELECT * INTO cap FROM resource_edit_receipts WHERE team_id=s.team_id AND id=(body->>'writeCapability')::uuid
 AND account_id=s.account_id AND device_id=s.device_id AND resource_id=r AND vault_id=v;
 IF cap.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF EXISTS(SELECT FROM offline_creation_claims c JOIN offline_creation_scopes p ON(p.team_id,p.id)=(c.team_id,c.scope_id)
 WHERE c.team_id=s.team_id AND c.receipt_id=cap.id AND p.parent_id IS DISTINCT FROM parent)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 IF (body->>'aclRevision')::bigint IS DISTINCT FROM (SELECT acl_revision FROM teams WHERE id=s.team_id)
 THEN RAISE EXCEPTION 'acl_changed' USING ERRCODE='40001'; END IF;
 IF jsonb_typeof(envelopes) IS DISTINCT FROM 'array' OR jsonb_array_length(envelopes) NOT BETWEEN 1 AND 1000
 THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(envelopes)<>(SELECT count(*) FROM offline_decision_recipients(s.team_id,r))
 OR EXISTS(SELECT FROM jsonb_array_elements(envelopes)e WHERE NOT EXISTS(SELECT FROM offline_decision_recipients(s.team_id,r) current
 WHERE current.account_id=(e->>'accountId')::uuid AND current.identity_version=(e->>'identityVersion')::bigint))
 OR EXISTS(SELECT FROM jsonb_array_elements(envelopes)e GROUP BY e->>'accountId' HAVING count(*)>1)
 THEN RAISE EXCEPTION 'recipients_changed' USING ERRCODE='40001'; END IF;
 INSERT INTO change_operations(team_id,id,account_id,device_id,sequence,resource_id,expected_revision,request_hash,status,client_at)
 VALUES(s.team_id,op_id,s.account_id,s.device_id,seq,NULL,expected,request_hash,'conflict',(body->>'clientAt')::timestamptz);
 INSERT INTO creation_decisions(team_id,operation_id,resource_id,vault_id,parent_id,encrypted_candidate)
 VALUES(s.team_id,op_id,r,v,parent,snapshot) RETURNING id INTO decision;
 INSERT INTO creation_decision_envelopes SELECT s.team_id,decision,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,decode(e->>'envelope','hex') FROM jsonb_array_elements(envelopes)e;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'offline.creation_received','succeeded');
 RETURN jsonb_build_object('operationId',op_id,'status','conflict','conflictId',decision);
END $$;
INSERT INTO schema_migrations(version) VALUES(59);
COMMIT;
