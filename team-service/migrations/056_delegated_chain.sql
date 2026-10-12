BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- A manager may redistribute hop metadata only inside the owner-delegated
-- connection scope. Both the old and proposed route are checked again at apply.
CREATE FUNCTION require_chain_scope(s auth_sessions,r uuid,hops jsonb) RETURNS void
LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
BEGIN
 IF EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner') THEN RETURN; END IF;
 IF jsonb_typeof(hops) IS DISTINCT FROM 'array' OR jsonb_array_length(hops)>15 THEN RAISE EXCEPTION 'invalid_chain' USING ERRCODE='22023'; END IF;
 IF NOT EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='manager')
 OR NOT allowed(s.team_id,s.account_id,r,'edit')
 OR EXISTS(SELECT FROM (
   SELECT r AS id UNION SELECT value::uuid FROM jsonb_array_elements_text(hops)
   UNION SELECT jump_host_id FROM host_hops WHERE team_id=s.team_id AND host_id=r
 )route WHERE NOT allowed(s.team_id,s.account_id,route.id,'read') OR NOT allowed(s.team_id,s.account_id,route.id,'connect')
 OR NOT EXISTS(SELECT FROM acl_delegations ad WHERE ad.team_id=s.team_id AND ad.account_id=s.account_id AND ad.permission='connect'
 AND within_scope(s.team_id,r,ad.resource_id) AND within_scope(s.team_id,route.id,ad.resource_id)))
 THEN RAISE EXCEPTION 'delegation_scope_denied' USING ERRCODE='42501'; END IF;
END $$;
CREATE OR REPLACE FUNCTION connection_manifest(s auth_sessions,r uuid,hops jsonb) RETURNS jsonb
LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE ids uuid[]; hop uuid; rows jsonb:='[]'; recipient_rows jsonb;
BEGIN
 PERFORM require_chain_scope(s,r,hops);
 IF jsonb_typeof(hops) IS DISTINCT FROM 'array' OR jsonb_array_length(hops)>15 THEN RAISE EXCEPTION 'invalid_chain' USING ERRCODE='22023'; END IF;
 SELECT coalesce(array_agg(value::uuid),'{}') INTO ids FROM jsonb_array_elements_text(hops);
 IF r=ANY(ids) OR cardinality(ids)<>(SELECT count(DISTINCT x) FROM unnest(ids)x)
 THEN RAISE EXCEPTION 'chain_cycle' USING ERRCODE='22023'; END IF;
 IF NOT EXISTS(SELECT FROM hosts h JOIN resources x ON (x.team_id,x.id)=(h.team_id,h.id)
 WHERE h.team_id=s.team_id AND h.id=r AND NOT x.deleted AND h.protocol IN ('ssh','mosh'))
 THEN RAISE EXCEPTION 'invalid_target' USING ERRCODE='22023'; END IF;
 FOREACH hop IN ARRAY ids LOOP
  IF NOT EXISTS(SELECT FROM hosts h JOIN resources x ON (x.team_id,x.id)=(h.team_id,h.id)
  WHERE h.team_id=s.team_id AND h.id=hop AND h.protocol='ssh' AND NOT x.deleted)
  THEN RAISE EXCEPTION 'invalid_jump_host' USING ERRCODE='22023'; END IF;
  -- Ordered hops form a complete route; never silently omit a hop's own route.
  IF EXISTS(SELECT FROM host_hops WHERE team_id=s.team_id AND host_id=hop)
  THEN RAISE EXCEPTION 'nested_chain_requires_flat_route' USING ERRCODE='22023'; END IF;
  IF EXISTS(SELECT FROM host_hops WHERE team_id=s.team_id AND jump_host_id=r) AND cardinality(ids)>0
  THEN RAISE EXCEPTION 'chain_dependency_requires_flat_route' USING ERRCODE='22023'; END IF;
  rows:=rows||jsonb_build_array(jsonb_build_object('id',hop,'revision',(SELECT revision FROM resources WHERE team_id=s.team_id AND id=hop)));
 END LOOP;
 SELECT coalesce(jsonb_agg(jsonb_build_object('accountId',a.id,'identityVersion',i.version,'publicKey',encode(i.public_key,'hex')) ORDER BY a.id),'[]') INTO recipient_rows
 FROM team_accounts a JOIN account_crypto_identities i ON (i.team_id,i.account_id)=(a.team_id,a.id) AND i.active
 WHERE a.team_id=s.team_id AND a.status='active' AND allowed(s.team_id,a.id,r,'connect');
 RETURN jsonb_build_object('targetId',r,'targetRevision',(SELECT revision FROM resources WHERE team_id=s.team_id AND id=r),
 'configurationRevision',coalesce((SELECT revision FROM connection_configurations WHERE team_id=s.team_id AND host_id=r),0),
 'aclRevision',(SELECT acl_revision FROM teams WHERE id=s.team_id),'hops',rows,'recipients',recipient_rows);
END $$;
CREATE OR REPLACE FUNCTION api_chain_finish(token text,p uuid,items jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;plan connection_change_plans;current_manifest jsonb;hops jsonb;digest bytea;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO plan FROM connection_change_plans WHERE team_id=s.team_id AND id=p AND actor_id=s.account_id FOR UPDATE;
 IF plan.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 PERFORM require_chain_scope(s,plan.host_id,(SELECT coalesce(jsonb_agg(h->'id'),'[]') FROM jsonb_array_elements(plan.manifest->'hops')h));
 digest:=sha256(convert_to(items::text,'UTF8'));
 IF plan.applied THEN
  IF plan.body_hash IS DISTINCT FROM digest THEN RAISE EXCEPTION 'operation_mismatch' USING ERRCODE='22023'; END IF;
  RETURN jsonb_build_object('status','applied','planId',p);
 END IF;
 IF plan.expires_at<=now() THEN RAISE EXCEPTION 'plan_expired' USING ERRCODE='40001'; END IF;
 SELECT coalesce(jsonb_agg(h->'id'),'[]') INTO hops FROM jsonb_array_elements(plan.manifest->'hops')h;
 current_manifest:=connection_manifest(s,plan.host_id,hops);
 IF current_manifest IS DISTINCT FROM plan.manifest THEN RAISE EXCEPTION 'plan_changed' USING ERRCODE='40001'; END IF;
 IF jsonb_typeof(items) IS DISTINCT FROM 'array' OR jsonb_array_length(items)>15000 THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(items)<>jsonb_array_length(plan.manifest->'hops')*jsonb_array_length(plan.manifest->'recipients') OR EXISTS(
 (SELECT h->>'id',a->>'accountId',a->>'identityVersion' FROM jsonb_array_elements(plan.manifest->'hops')h CROSS JOIN jsonb_array_elements(plan.manifest->'recipients')a
 EXCEPT SELECT e->>'hopId',e->>'accountId',e->>'identityVersion' FROM jsonb_array_elements(items)e)
 UNION ALL
 (SELECT e->>'hopId',e->>'accountId',e->>'identityVersion' FROM jsonb_array_elements(items)e EXCEPT
 SELECT h->>'id',a->>'accountId',a->>'identityVersion' FROM jsonb_array_elements(plan.manifest->'hops')h CROSS JOIN jsonb_array_elements(plan.manifest->'recipients')a))
 THEN RAISE EXCEPTION 'recipients_changed' USING ERRCODE='40001'; END IF;
 DELETE FROM host_hops WHERE team_id=s.team_id AND host_id=plan.host_id;
 INSERT INTO host_hops SELECT s.team_id,plan.host_id,(n-1)::smallint,(h->>'id')::uuid FROM jsonb_array_elements(plan.manifest->'hops') WITH ORDINALITY AS hs(h,n);
 INSERT INTO connection_dependency_envelopes SELECT s.team_id,plan.host_id,(e->>'hopId')::uuid,(h->>'revision')::bigint,
 (e->>'accountId')::uuid,(e->>'identityVersion')::bigint,decode(e->>'envelope','hex')
 FROM jsonb_array_elements(items)e JOIN jsonb_array_elements(plan.manifest->'hops')h ON h->>'id'=e->>'hopId';
 INSERT INTO connection_configurations VALUES(s.team_id,plan.host_id,1) ON CONFLICT(team_id,host_id) DO UPDATE SET revision=connection_configurations.revision+1;
 UPDATE connection_change_plans SET applied=true,body_hash=digest WHERE team_id=s.team_id AND id=p;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'connection.chain_update',plan.host_id,'succeeded');
 RETURN jsonb_build_object('status','applied','planId',p);
END $$;
CREATE OR REPLACE FUNCTION api_chain_configuration(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);PERFORM require_chain_scope(s,r,'[]');
 IF NOT EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=r) THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 RETURN jsonb_build_object('targetId',r,'revision',coalesce((SELECT revision FROM connection_configurations WHERE team_id=s.team_id AND host_id=r),0),
 'hops',coalesce((SELECT jsonb_agg(jump_host_id ORDER BY position) FROM host_hops WHERE team_id=s.team_id AND host_id=r),'[]'));
END $$;
REVOKE ALL ON FUNCTION require_chain_scope(auth_sessions,uuid,jsonb) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(56);
COMMIT;
