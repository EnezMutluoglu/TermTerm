BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Dependency envelopes grant access ONLY through an authorized final target.
CREATE TABLE connection_configurations (
 team_id uuid NOT NULL,host_id uuid NOT NULL,revision bigint NOT NULL DEFAULT 0,
 PRIMARY KEY(team_id,host_id), FOREIGN KEY(team_id,host_id) REFERENCES hosts(team_id,id)
);
CREATE TABLE connection_dependency_envelopes (
 team_id uuid NOT NULL,host_id uuid NOT NULL,jump_host_id uuid NOT NULL,resource_revision bigint NOT NULL,
 account_id uuid NOT NULL,identity_version bigint NOT NULL,envelope bytea NOT NULL CHECK(octet_length(envelope) BETWEEN 80 AND 4096),
 PRIMARY KEY(team_id,host_id,jump_host_id,account_id),
 FOREIGN KEY(team_id,host_id,jump_host_id) REFERENCES host_hops(team_id,host_id,jump_host_id) ON DELETE CASCADE,
 FOREIGN KEY(team_id,jump_host_id,resource_revision) REFERENCES resource_versions(team_id,resource_id,revision) ON DELETE CASCADE,
 FOREIGN KEY(team_id,account_id,identity_version) REFERENCES account_crypto_identities(team_id,account_id,version)
);
CREATE INDEX connection_dependency_account ON connection_dependency_envelopes(team_id,account_id,host_id);
CREATE TABLE connection_change_plans (
 team_id uuid NOT NULL,id uuid NOT NULL DEFAULT gen_random_uuid(),actor_id uuid NOT NULL,host_id uuid NOT NULL,
 manifest jsonb NOT NULL,expires_at timestamptz NOT NULL DEFAULT now()+interval '15 minutes',
 applied boolean NOT NULL DEFAULT false,body_hash bytea,
 PRIMARY KEY(team_id,id),FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,host_id) REFERENCES hosts(team_id,id)
);
CREATE INDEX connection_plans_pending ON connection_change_plans(team_id,expires_at) WHERE NOT applied;
ALTER TABLE connection_configurations ENABLE ROW LEVEL SECURITY;
ALTER TABLE connection_dependency_envelopes ENABLE ROW LEVEL SECURITY;
ALTER TABLE connection_change_plans ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON connection_configurations,connection_dependency_envelopes,connection_change_plans FROM PUBLIC;

CREATE FUNCTION connection_manifest(s auth_sessions,r uuid,hops jsonb) RETURNS jsonb
LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE ids uuid[]; hop uuid; rows jsonb:='[]'; recipient_rows jsonb;
BEGIN
 PERFORM require_owner(s);
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
CREATE FUNCTION api_chain_prepare(token text,r uuid,hops jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;m jsonb;p uuid;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 m:=connection_manifest(s,r,hops);
 INSERT INTO connection_change_plans(team_id,actor_id,host_id,manifest) VALUES(s.team_id,s.account_id,r,m) RETURNING id INTO p;
 RETURN m||jsonb_build_object('planId',p,'expiresInSeconds',900,'resources',coalesce((SELECT jsonb_agg(shared_resource_json(s.team_id,s.account_id,(h->>'id')::uuid)) FROM jsonb_array_elements(m->'hops')h),'[]'));
END $$;
CREATE FUNCTION api_chain_finish(token text,p uuid,items jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;plan connection_change_plans;current_manifest jsonb;hops jsonb;digest bytea;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO plan FROM connection_change_plans WHERE team_id=s.team_id AND id=p AND actor_id=s.account_id FOR UPDATE;
 IF plan.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
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

CREATE FUNCTION api_connection_plan(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;hop record;row jsonb;steps jsonb:='[]';c uuid;m text;dep bytea;
BEGIN
 s:=session_actor(token);
 IF NOT allowed(s.team_id,s.account_id,r,'connect') OR NOT EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=r)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 FOR hop IN SELECT jump_host_id AS id,position FROM host_hops WHERE team_id=s.team_id AND host_id=r UNION ALL SELECT r,16 ORDER BY position LOOP
  IF NOT EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=hop.id AND NOT deleted)
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  row:=shared_resource_json(s.team_id,s.account_id,hop.id);
  IF hop.id<>r THEN
   SELECT e.envelope INTO dep FROM connection_dependency_envelopes e JOIN account_crypto_identities i
   ON (i.team_id,i.account_id,i.version)=(e.team_id,e.account_id,e.identity_version) AND i.active
   WHERE e.team_id=s.team_id AND e.host_id=r AND e.jump_host_id=hop.id AND e.account_id=s.account_id AND e.resource_revision=(row->>'revision')::bigint;
   IF dep IS NULL THEN RAISE EXCEPTION 'chain_refresh_required' USING ERRCODE='42501'; END IF;
   row:=(row-'envelope'-'permissions')||jsonb_build_object('dependencyEnvelope',encode(dep,'hex'));
  ELSE
   row:=row||jsonb_build_object('temporaryRevision',(SELECT revision FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='temporary'));
  END IF;
  c:=NULL;m:=NULL;
  SELECT credential_id,method INTO c,m FROM user_host_profiles WHERE team_id=s.team_id AND account_id=s.account_id AND host_id=hop.id;
  IF m IS NULL OR m='inherit' THEN SELECT credential_id,method INTO c,m FROM user_connection_defaults WHERE team_id=s.team_id AND account_id=s.account_id; END IF;
  IF c IS NOT NULL AND (NOT credential_usable(s.team_id,s.account_id,c) OR EXISTS(SELECT FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c AND NOT within_scope(s.team_id,hop.id,scope_id)))
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  steps:=steps||jsonb_build_array(row);
 END LOOP;
 RETURN jsonb_build_object('targetId',r,'configurationRevision',coalesce((SELECT revision FROM connection_configurations WHERE team_id=s.team_id AND host_id=r),0),'steps',steps);
END $$;
CREATE FUNCTION api_connection_credential(token text,r uuid,hop uuid,c uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p jsonb;row credentials;answer jsonb;
BEGIN
 s:=session_actor(token);p:=api_connection_plan(token,r);
 IF NOT EXISTS(SELECT FROM jsonb_array_elements(p->'steps')x WHERE (x->>'id')::uuid=hop)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT * INTO row FROM credentials WHERE team_id=s.team_id AND id=c AND NOT deleted;
 IF row.visibility='private' THEN RETURN api_private_credential_get(token,c); END IF;
 IF row.id IS NULL OR NOT credential_usable(s.team_id,s.account_id,c) OR NOT EXISTS(SELECT FROM shared_credential_scopes
 WHERE team_id=s.team_id AND credential_id=c AND within_scope(s.team_id,hop,scope_id))
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT jsonb_build_object('id',row.id,'revision',row.revision,'kind',row.kind,'encryptedData',encode(v.encrypted_data,'hex'),
 'encryptedSecret',encode(v.encrypted_secret,'hex'),'envelopes',(SELECT jsonb_agg(jsonb_build_object('purpose',e.purpose,'envelope',encode(envelope,'hex'),'identityVersion',identity_version))
 FROM credential_key_envelopes e WHERE team_id=s.team_id AND credential_id=c AND revision=v.revision AND account_id=s.account_id)) INTO answer
 FROM credential_versions v WHERE v.team_id=s.team_id AND v.credential_id=c AND v.revision=row.revision;
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'credential.connect',row.resource_id,'succeeded');
 RETURN answer;
END $$;
REVOKE ALL ON FUNCTION connection_manifest(auth_sessions,uuid,jsonb),api_chain_prepare(text,uuid,jsonb),api_chain_finish(text,uuid,jsonb),api_connection_plan(text,uuid),api_connection_credential(text,uuid,uuid,uuid) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(37);
COMMIT;
