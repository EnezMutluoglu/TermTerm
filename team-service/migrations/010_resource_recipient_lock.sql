BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
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
 OR seq IS NULL OR seq<=0 OR is_deleted IS NULL OR kind_input IS NULL OR kind_input NOT IN ('folder','host')
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
 OR EXISTS(SELECT FROM host_hops WHERE team_id=s.team_id AND jump_host_id=r)
 OR EXISTS(SELECT FROM resource_links WHERE team_id=s.team_id AND target_id=r)) THEN RAISE EXCEPTION 'resource_in_use' USING ERRCODE='23503'; END IF;
 rev:=expected+1;
 INSERT INTO resources(team_id,vault_id,id,kind,revision) VALUES(s.team_id,v,r,kind_input,rev)
 ON CONFLICT(team_id,id) DO UPDATE SET revision=rev,modified_at=clock_timestamp();
 -- Writes to control columns are typed, never inferred from ciphertext or a generic document.
 IF kind_input='folder' THEN
  IF octet_length(decode(body->>'encryptedName','hex')) NOT BETWEEN 40 AND 4096 THEN RAISE EXCEPTION 'invalid_name' USING ERRCODE='22023'; END IF;
  INSERT INTO folders VALUES(s.team_id,v,r,parent,decode(body->>'encryptedName','hex'))
  ON CONFLICT(team_id,id) DO UPDATE SET encrypted_name=excluded.encrypted_name;
 ELSE
  IF octet_length(decode(body->>'encryptedName','hex')) NOT BETWEEN 40 AND 4096
  OR octet_length(decode(body->>'encryptedAddress','hex')) NOT BETWEEN 40 AND 4096
  OR coalesce(octet_length(decode(body->>'encryptedUsername','hex')),40) NOT BETWEEN 40 AND 4096 THEN RAISE EXCEPTION 'invalid_host_data' USING ERRCODE='22023'; END IF;
  INSERT INTO hosts VALUES(s.team_id,v,r,parent,body->>'protocol',(body->>'port')::integer,body->>'usernamePolicy',
    decode(body->>'encryptedAddress','hex'),decode(body->>'encryptedName','hex'),decode(body->>'encryptedUsername','hex'))
  ON CONFLICT(team_id,id) DO UPDATE SET protocol=excluded.protocol,port=excluded.port,username_policy=excluded.username_policy,
    encrypted_address=excluded.encrypted_address,encrypted_name=excluded.encrypted_name,encrypted_username=excluded.encrypted_username;
 END IF;
 PERFORM check_shared_envelopes(s.team_id,r,envelopes);
 INSERT INTO resource_versions(team_id,resource_id,revision,encrypted_snapshot,actor_id) VALUES(s.team_id,r,rev,snapshot,s.account_id);
 INSERT INTO resource_key_envelopes SELECT s.team_id,r,rev,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,decode(e->>'envelope','hex') FROM jsonb_array_elements(envelopes)e;
 UPDATE resources SET deleted=is_deleted WHERE team_id=s.team_id AND id=r;
 UPDATE vaults SET revision=revision+1 WHERE team_id=s.team_id AND id=v RETURNING revision INTO vault_rev;
 INSERT INTO change_operations(team_id,id,account_id,device_id,sequence,resource_id,expected_revision,request_hash,status,client_at,result_revision)
 VALUES(s.team_id,op_id,s.account_id,s.device_id,seq,r,expected,request_hash,'applied',(body->>'clientAt')::timestamptz,rev);
 DELETE FROM resource_versions rv WHERE rv.team_id=s.team_id AND rv.resource_id=r AND rv.revision<rev-10
 AND NOT EXISTS(SELECT FROM revision_pins p WHERE (p.team_id,p.resource_id,p.revision)=(rv.team_id,rv.resource_id,rv.revision));
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,CASE WHEN is_deleted THEN 'resource.delete' ELSE 'resource.save' END,r,'succeeded');
 RETURN jsonb_build_object('operationId',op_id,'status','applied','revision',rev,'vaultRevision',vault_rev);
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(10,now());
COMMIT;

