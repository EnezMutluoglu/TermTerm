BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION api_move_keys(token text,plan uuid,after_index bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE p resource_move_plans;items jsonb;
BEGIN
 p:=move_actor(token,plan);
 IF p.status<>'pending' OR p.expires_at<=now() OR after_index IS NULL OR after_index<0 THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='22023'; END IF;
 SELECT coalesce(jsonb_agg(jsonb_build_object('ordinal',k.ordinal,'kind',k.kind,'id',k.object_id,'revision',k.revision,'rootId',k.root_id,
 'accountId',k.account_id,'identityVersion',k.identity_version,'envelope',encode(k.source_envelope,'hex'),'freshSource',k.fresh_source,
 'publicKey',encode(i.public_key,'hex')) ORDER BY k.ordinal),'[]') INTO items FROM
 (SELECT * FROM move_plan_keys WHERE team_id=p.team_id AND plan_id=plan AND ordinal>after_index ORDER BY ordinal LIMIT 20)k
 JOIN account_crypto_identities i ON (i.team_id,i.account_id,i.version)=(k.team_id,k.account_id,k.identity_version);
 RETURN jsonb_build_object('keys',items,'nextAfter',CASE WHEN jsonb_array_length(items)>0 THEN items->(jsonb_array_length(items)-1)->'ordinal' END);
END $$;
CREATE FUNCTION api_move_stage(token text,plan uuid,items jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE p resource_move_plans;e jsonb;
BEGIN
 p:=move_actor(token,plan);
 IF p.status<>'pending' OR p.expires_at<=now() OR jsonb_typeof(items) IS DISTINCT FROM 'array' OR jsonb_array_length(items) NOT BETWEEN 1 AND 20
 OR (SELECT count(DISTINCT x->>'ordinal') FROM jsonb_array_elements(items)x)<>jsonb_array_length(items) THEN RAISE EXCEPTION 'invalid_batch' USING ERRCODE='22023'; END IF;
 FOR e IN SELECT * FROM jsonb_array_elements(items) LOOP
  IF e-'ordinal'-'envelope'<>'{}' OR e->>'envelope' IS NULL THEN RAISE EXCEPTION 'invalid_envelope' USING ERRCODE='22023'; END IF;
  UPDATE move_plan_keys SET target_envelope=decode(e->>'envelope','hex') WHERE team_id=p.team_id AND plan_id=plan AND ordinal=(e->>'ordinal')::bigint;
  IF NOT FOUND THEN RAISE EXCEPTION 'invalid_key_index' USING ERRCODE='22023'; END IF;
 END LOOP;
 RETURN jsonb_build_object('staged',jsonb_array_length(items));
END $$;
CREATE FUNCTION api_move_impact(token text,plan uuid,after_index bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE p resource_move_plans;items jsonb;
BEGIN
 p:=move_actor(token,plan);
 IF after_index IS NULL OR after_index<0 THEN RAISE EXCEPTION 'invalid_cursor' USING ERRCODE='22023'; END IF;
 SELECT coalesce(jsonb_agg(jsonb_build_object('resourceId',c.resource_id,'accountId',c.account_id,'displayName',a.display_name,'before',c.before_permissions,'after',c.after_permissions) ORDER BY c.resource_id,c.account_id),'[]') INTO items
 FROM (SELECT * FROM move_access_changes WHERE team_id=p.team_id AND plan_id=plan ORDER BY resource_id,account_id OFFSET after_index LIMIT 100)c
 JOIN team_accounts a ON (a.team_id,a.id)=(c.team_id,c.account_id);
 RETURN jsonb_build_object('items',items,'nextAfter',CASE WHEN jsonb_array_length(items)=100 THEN after_index+100 END,
 'chainReferences',(SELECT count(DISTINCT host_id) FROM host_hops WHERE team_id=p.team_id AND (within_scope(p.team_id,host_id,p.resource_id) OR jump_host_id=p.resource_id)),
 'sharedIdentities',(SELECT count(*) FROM shared_credential_scopes WHERE team_id=p.team_id AND within_scope(p.team_id,scope_id,p.resource_id)));
END $$;
CREATE FUNCTION api_move_finish(token text,plan uuid,body jsonb,cancel boolean) RETURNS jsonb
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
REVOKE ALL ON FUNCTION api_move_keys(text,uuid,bigint),api_move_stage(text,uuid,jsonb),api_move_impact(text,uuid,bigint),api_move_finish(text,uuid,jsonb,boolean) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(55);
COMMIT;
