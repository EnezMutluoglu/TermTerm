BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION allowed_moved(t uuid,a uuid,r uuid,p text,moving uuid,destination uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 WITH RECURSIVE ancestry(id,depth) AS (
 SELECT r,0 UNION ALL SELECT CASE WHEN id=moving THEN destination ELSE resource_parent(t,id) END,depth+1
 FROM ancestry WHERE id IS NOT NULL AND depth<64
 ), rules AS (SELECT effect FROM resource_acl acl JOIN ancestry x ON x.id=acl.resource_id
 WHERE acl.team_id=t AND acl.account_id=a AND acl.permission=p)
 SELECT EXISTS(SELECT FROM team_accounts WHERE team_id=t AND id=a AND status='active')
 AND EXISTS(SELECT FROM resources WHERE team_id=t AND id=r AND NOT deleted)
 AND (EXISTS(SELECT FROM team_memberships WHERE team_id=t AND account_id=a AND role='owner')
 OR (NOT EXISTS(SELECT FROM rules WHERE effect='deny') AND EXISTS(SELECT FROM rules WHERE effect='allow')))
$$;
CREATE TABLE resource_move_plans (
 team_id uuid NOT NULL,id uuid NOT NULL DEFAULT gen_random_uuid(),actor_id uuid NOT NULL,vault_id uuid NOT NULL,
 resource_id uuid NOT NULL,destination_id uuid,source_revision bigint NOT NULL,acl_revision bigint NOT NULL,vault_revision bigint NOT NULL,
 identity_hash bytea NOT NULL,route_hash bytea NOT NULL,status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','applied','cancelled')),
 expires_at timestamptz NOT NULL DEFAULT now()+interval '15 minutes',request_hash bytea,result jsonb,
 PRIMARY KEY(team_id,id),FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,vault_id,resource_id) REFERENCES resources(team_id,vault_id,id),
 FOREIGN KEY(team_id,vault_id,destination_id) REFERENCES folders(team_id,vault_id,id)
);
CREATE INDEX move_plans_pending ON resource_move_plans(team_id,actor_id,expires_at) WHERE status='pending';
CREATE TABLE move_access_changes (
 team_id uuid NOT NULL,plan_id uuid NOT NULL,resource_id uuid NOT NULL,account_id uuid NOT NULL,
 before_permissions text[] NOT NULL,after_permissions text[] NOT NULL,
 PRIMARY KEY(team_id,plan_id,resource_id,account_id),
 FOREIGN KEY(team_id,plan_id) REFERENCES resource_move_plans ON DELETE CASCADE,
 FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id),
 FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id)
);
CREATE TABLE move_plan_keys (
 team_id uuid NOT NULL,plan_id uuid NOT NULL,ordinal bigint NOT NULL,
 kind text NOT NULL CHECK(kind IN ('resource','vault','dependency')),object_id uuid NOT NULL,revision bigint NOT NULL,
 root_id uuid,account_id uuid NOT NULL,identity_version bigint NOT NULL,source_envelope bytea,
 fresh_source boolean NOT NULL DEFAULT false,target_envelope bytea CHECK(octet_length(target_envelope) BETWEEN 80 AND 4096),
 PRIMARY KEY(team_id,plan_id,ordinal),FOREIGN KEY(team_id,plan_id) REFERENCES resource_move_plans ON DELETE CASCADE,
 FOREIGN KEY(team_id,account_id,identity_version) REFERENCES account_crypto_identities(team_id,account_id,version)
);
ALTER TABLE resource_move_plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE move_access_changes ENABLE ROW LEVEL SECURITY;
ALTER TABLE move_plan_keys ENABLE ROW LEVEL SECURITY;
CREATE FUNCTION move_identity_hash(t uuid) RETURNS bytea LANGUAGE sql STABLE AS $$
 SELECT sha256(convert_to(coalesce(string_agg(a.id::text||':'||a.status||':'||i.version::text,',' ORDER BY a.id),''),'UTF8'))
 FROM team_accounts a JOIN account_crypto_identities i ON (i.team_id,i.account_id)=(a.team_id,a.id) AND i.active WHERE a.team_id=t
$$;
CREATE FUNCTION require_move_scope(s auth_sessions,r uuid,d uuid) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner') THEN RETURN; END IF;
 IF d IS NULL OR NOT EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='manager')
 OR EXISTS(SELECT p FROM unnest(ARRAY['read','connect','edit'])p WHERE NOT allowed(s.team_id,s.account_id,r,p)
 OR NOT allowed(s.team_id,s.account_id,d,p) OR NOT EXISTS(SELECT FROM acl_delegations ad WHERE ad.team_id=s.team_id AND ad.account_id=s.account_id
 AND ad.permission=p AND within_scope(s.team_id,r,ad.resource_id) AND within_scope(s.team_id,d,ad.resource_id)))
 OR EXISTS(SELECT FROM resources x CROSS JOIN unnest(ARRAY['read','connect','edit','reveal','export','manage'])p
 WHERE x.team_id=s.team_id AND within_scope(s.team_id,x.id,r) AND
 (allowed(s.team_id,s.account_id,x.id,p) IS DISTINCT FROM allowed_moved(s.team_id,s.account_id,x.id,p,r,d)
 OR (p IN ('read','connect','edit') AND NOT allowed(s.team_id,s.account_id,x.id,p))))
 THEN RAISE EXCEPTION 'delegation_scope_denied' USING ERRCODE='42501'; END IF;
 IF EXISTS(SELECT FROM host_hops WHERE team_id=s.team_id AND jump_host_id=r AND NOT allowed(s.team_id,s.account_id,host_id,'read')) THEN RAISE EXCEPTION 'delegation_dependency_denied' USING ERRCODE='42501'; END IF;
END $$;
CREATE FUNCTION move_route_hash(t uuid) RETURNS bytea LANGUAGE sql STABLE AS $$
 SELECT sha256(convert_to(coalesce(string_agg(h.host_id::text||':'||h.position::text||':'||h.jump_host_id::text||':'||r.revision::text,',' ORDER BY h.host_id,h.position),''),'UTF8'))
 FROM host_hops h JOIN resources r ON (r.team_id,r.id)=(h.team_id,h.jump_host_id) WHERE h.team_id=t
$$;
CREATE FUNCTION move_actor(token text,plan uuid) RETURNS resource_move_plans LANGUAGE plpgsql AS $$
DECLARE s auth_sessions;p resource_move_plans;
BEGIN
 s:=session_actor(token);SELECT * INTO p FROM resource_move_plans WHERE team_id=s.team_id AND id=plan AND actor_id=s.account_id;
 IF p.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 PERFORM require_move_scope(s,p.resource_id,p.destination_id);RETURN p;
END $$;
CREATE FUNCTION api_move_prepare(token text,r uuid,d uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p resource_move_plans;x resources;av bigint;n bigint;recipient_rows jsonb;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO x FROM resources WHERE team_id=s.team_id AND id=r AND NOT deleted AND kind IN ('folder','host');
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
REVOKE ALL ON FUNCTION allowed_moved(uuid,uuid,uuid,text,uuid,uuid),move_identity_hash(uuid),move_route_hash(uuid),require_move_scope(auth_sessions,uuid,uuid),move_actor(text,uuid),api_move_prepare(text,uuid,uuid) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(54);
COMMIT;
