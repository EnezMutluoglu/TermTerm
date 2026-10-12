BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
ALTER TABLE credentials DROP CONSTRAINT credentials_kind_check;
ALTER TABLE credentials ADD CONSTRAINT credentials_kind_check CHECK(kind IN ('ssh_key','password','agent','hardware','proxy'));
CREATE TABLE user_proxy_profiles (
 team_id uuid NOT NULL,account_id uuid NOT NULL,host_id uuid NOT NULL,credential_id uuid NOT NULL,
 PRIMARY KEY(team_id,account_id,host_id),UNIQUE(team_id,credential_id),
 FOREIGN KEY(team_id,host_id) REFERENCES hosts(team_id,id),
 FOREIGN KEY(team_id,account_id,credential_id) REFERENCES credentials(team_id,owner_account_id,id)
);
ALTER TABLE user_proxy_profiles ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON user_proxy_profiles FROM PUBLIC;
CREATE FUNCTION api_proxy_profile(token text,root uuid,hop uuid,c uuid,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;plan jsonb;old uuid;result jsonb;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 plan:=api_connection_plan(token,root);
 IF NOT EXISTS(SELECT FROM jsonb_array_elements(plan->'steps') e WHERE (e->>'id')::uuid=hop)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT credential_id INTO old FROM user_proxy_profiles WHERE team_id=s.team_id AND account_id=s.account_id AND host_id=hop FOR UPDATE;
 IF body IS NULL THEN RETURN jsonb_build_object('credentialId',old); END IF;
 IF c IS NULL OR (old IS NOT NULL AND c<>old) THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
 IF body->>'action'='forget' THEN
  IF old IS NULL OR NOT EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=old AND revision=(body->>'expectedRevision')::bigint)
  THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
  DELETE FROM user_proxy_profiles WHERE team_id=s.team_id AND account_id=s.account_id AND host_id=hop;
  UPDATE credentials SET deleted=true WHERE team_id=s.team_id AND id=old;
  INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'proxy.forget',hop,'succeeded');
  RETURN jsonb_build_object('forgotten',true);
 END IF;
 IF old IS NULL AND EXISTS(SELECT FROM credentials WHERE team_id=s.team_id AND id=c) THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
 result:=api_private_credential_put(token,c,(body->>'expectedRevision')::bigint,'proxy',decode(body->>'encryptedData','hex'),decode(body->>'encryptedSecret','hex'),
 (body->>'identityVersion')::bigint,(body->>'recoveryKeyId')::uuid,decode(body->>'userData','hex'),decode(body->>'userSecret','hex'),decode(body->>'recoveryData','hex'),decode(body->>'recoverySecret','hex'));
 INSERT INTO user_proxy_profiles VALUES(s.team_id,s.account_id,hop,c) ON CONFLICT(team_id,account_id,host_id) DO NOTHING;
 RETURN result;
END $$;
CREATE FUNCTION api_chain_history(token text,r uuid,rev bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 IF NOT allowed(s.team_id,s.account_id,r,'read') OR NOT allowed(s.team_id,s.account_id,r,'edit') THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF NOT EXISTS(SELECT FROM connection_route_snapshots WHERE team_id=s.team_id AND host_id=r AND resource_revision=rev)
 THEN RAISE EXCEPTION 'history_route_unavailable' USING ERRCODE='22023'; END IF;
 RETURN coalesce((SELECT jsonb_agg(jump_host_id ORDER BY position) FROM connection_route_steps WHERE team_id=s.team_id AND host_id=r AND resource_revision=rev),'[]');
END $$;
CREATE FUNCTION restore_resource_internal(token text, source_revision bigint, body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;r uuid:=(body->>'resourceId')::uuid; old resources;result jsonb;op change_operations;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT allowed(s.team_id,s.account_id,r,'edit') OR NOT allowed(s.team_id,s.account_id,r,'read')
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT * INTO op FROM change_operations WHERE team_id=s.team_id AND id=(body->>'operationId')::uuid;
 IF op.id IS NOT NULL THEN
  IF NOT EXISTS(SELECT FROM resource_restorations rr WHERE rr.team_id=s.team_id AND rr.operation_id=op.id AND rr.source_revision=restore_resource_internal.source_revision)
  THEN RAISE EXCEPTION 'operation_reuse' USING ERRCODE='22023'; END IF;
  -- The regular save validates actor/device/request hash before replaying.
  RETURN api_resource_save(token,body) || (SELECT jsonb_build_object('returnRevision',rr.return_revision,'sourceRevision',rr.source_revision)
  FROM resource_restorations rr WHERE rr.team_id=s.team_id AND rr.operation_id=op.id);
 END IF;
 SELECT * INTO old FROM resources WHERE team_id=s.team_id AND id=r FOR UPDATE;
 IF old.revision<>(body->>'expectedRevision')::bigint THEN RAISE EXCEPTION 'revision_changed' USING ERRCODE='40001'; END IF;
 IF source_revision IS NULL OR source_revision=old.revision OR NOT EXISTS(SELECT FROM resource_versions
 WHERE team_id=s.team_id AND resource_id=r AND revision=source_revision)
 THEN RAISE EXCEPTION 'history_unavailable' USING ERRCODE='22023'; END IF;
 -- Keep the previous current version before api_resource_save prunes history.
 DELETE FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='return_point';
 INSERT INTO revision_pins VALUES(s.team_id,r,old.revision,s.account_id,'return_point');
 result:=api_resource_save(token,body);
 IF result->>'status'<>'applied' THEN RAISE EXCEPTION 'restore_not_applied' USING ERRCODE='40001'; END IF;
 INSERT INTO resource_restorations VALUES(s.team_id,(body->>'operationId')::uuid,source_revision,old.revision);
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome)
 VALUES(s.team_id,s.account_id,'resource.restore',r,'succeeded');
 RETURN result || jsonb_build_object('returnRevision',old.revision,'sourceRevision',source_revision);
END $$;
REVOKE ALL ON FUNCTION restore_resource_internal(text,bigint,jsonb) FROM PUBLIC;
CREATE OR REPLACE FUNCTION api_resource_restore(token text,source_revision bigint,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
BEGIN
 IF body->>'kind'='host' THEN
  IF api_chain_history(token,(body->>'resourceId')::uuid,source_revision) IS DISTINCT FROM api_chain_history(token,(body->>'resourceId')::uuid,(body->>'expectedRevision')::bigint)
  THEN RAISE EXCEPTION 'route_preview_required' USING ERRCODE='22023'; END IF;
 END IF;
 RETURN restore_resource_internal(token,source_revision,body);
END $$;
CREATE FUNCTION api_chain_restore(token text,p uuid,items jsonb,change jsonb,source bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;plan connection_change_plans;hops jsonb;result jsonb;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO plan FROM connection_change_plans WHERE team_id=s.team_id AND id=p AND actor_id=s.account_id FOR UPDATE;
 IF plan.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 IF change->>'kind' IS DISTINCT FROM 'host' OR (change->>'resourceId')::uuid IS DISTINCT FROM plan.host_id
 OR (change->>'expectedRevision')::bigint IS DISTINCT FROM (plan.manifest->>'targetRevision')::bigint OR (change->>'deleted')::boolean IS DISTINCT FROM false
 THEN RAISE EXCEPTION 'invalid_chain_revision' USING ERRCODE='22023'; END IF;
 IF NOT plan.applied THEN
  hops:=api_chain_history(token,plan.host_id,source);
  IF hops IS DISTINCT FROM (SELECT coalesce(jsonb_agg(h->'id'),'[]') FROM jsonb_array_elements(plan.manifest->'hops') h)
  THEN RAISE EXCEPTION 'plan_changed' USING ERRCODE='40001'; END IF;
 END IF;
 PERFORM api_chain_finish(token,p,items);
 result:=restore_resource_internal(token,source,change);
 RETURN result||jsonb_build_object('planId',p);
END $$;
REVOKE ALL ON FUNCTION api_proxy_profile(text,uuid,uuid,uuid,jsonb),api_chain_history(text,uuid,bigint),api_chain_restore(text,uuid,jsonb,jsonb,bigint) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(64);
COMMIT;
