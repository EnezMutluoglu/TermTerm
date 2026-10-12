BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE TABLE vault_key_envelopes (
 team_id uuid NOT NULL,vault_id uuid NOT NULL,account_id uuid NOT NULL,identity_version bigint NOT NULL,
 envelope bytea NOT NULL CHECK(octet_length(envelope) BETWEEN 80 AND 4096),
 PRIMARY KEY(team_id,vault_id,account_id,identity_version),
 FOREIGN KEY(team_id,vault_id) REFERENCES vaults(team_id,id),
 FOREIGN KEY(team_id,account_id,identity_version) REFERENCES account_crypto_identities(team_id,account_id,version)
);
CREATE TABLE resource_key_envelopes (
 team_id uuid NOT NULL,resource_id uuid NOT NULL,revision bigint NOT NULL,account_id uuid NOT NULL,identity_version bigint NOT NULL,
 envelope bytea NOT NULL CHECK(octet_length(envelope) BETWEEN 80 AND 4096),
 PRIMARY KEY(team_id,resource_id,revision,account_id,identity_version),
 FOREIGN KEY(team_id,resource_id,revision) REFERENCES resource_versions(team_id,resource_id,revision) ON DELETE CASCADE,
 FOREIGN KEY(team_id,account_id,identity_version) REFERENCES account_crypto_identities(team_id,account_id,version)
);
CREATE INDEX resource_envelopes_account ON resource_key_envelopes(team_id,account_id,resource_id,revision);
ALTER TABLE vault_key_envelopes ENABLE ROW LEVEL SECURITY;
ALTER TABLE resource_key_envelopes ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON vault_key_envelopes,resource_key_envelopes FROM PUBLIC;

CREATE FUNCTION resource_parent(t uuid,r uuid) RETURNS uuid LANGUAGE sql STABLE AS $$
 SELECT parent_id FROM folders WHERE team_id=t AND id=r
 UNION ALL SELECT folder_id FROM hosts WHERE team_id=t AND id=r
$$;
CREATE FUNCTION shared_recipients(t uuid,r uuid) RETURNS TABLE(account_id uuid,identity_version bigint,public_key bytea)
LANGUAGE sql STABLE AS $$
 SELECT a.id,i.version,i.public_key FROM team_accounts a JOIN account_crypto_identities i
 ON (i.team_id,i.account_id)=(a.team_id,a.id) AND i.active
 WHERE a.team_id=t AND a.status='active' AND (allowed(t,a.id,r,'read') OR allowed(t,a.id,r,'connect') OR allowed(t,a.id,r,'edit'))
$$;
CREATE FUNCTION api_vault_create(token text,v uuid,name bytea,identity_ver bigint,key_envelope bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 IF octet_length(name) NOT BETWEEN 40 AND 4096 OR NOT EXISTS(SELECT FROM account_crypto_identities
 WHERE team_id=s.team_id AND account_id=s.account_id AND version=identity_ver AND active) THEN
  RAISE EXCEPTION 'invalid_vault_identity' USING ERRCODE='22023'; END IF;
 INSERT INTO vaults(team_id,id,encrypted_name) VALUES(s.team_id,v,name);
 INSERT INTO vault_key_envelopes VALUES(s.team_id,v,s.account_id,identity_ver,key_envelope);
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'vault.create','succeeded');
 RETURN jsonb_build_object('id',v,'revision',0);
END $$;
CREATE FUNCTION api_vaults(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',v.id,'revision',v.revision,'encryptedName',encode(v.encrypted_name,'hex'),
 'envelope',encode(e.envelope,'hex'),'identityVersion',e.identity_version) ORDER BY v.id)
 FROM vaults v LEFT JOIN vault_key_envelopes e ON (e.team_id,e.vault_id,e.account_id)=(v.team_id,v.id,s.account_id)
 AND e.identity_version=(SELECT version FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND active)
 WHERE v.team_id=s.team_id AND (EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner')
 OR EXISTS(SELECT FROM resources r WHERE r.team_id=s.team_id AND r.vault_id=v.id AND allowed(s.team_id,s.account_id,r.id,'read')))),'[]');
END $$;
-- Online recipient discovery is separate from saving. Save revalidates the exact
-- set against current ACL/identity versions; a stale preview cannot distribute keys.
CREATE FUNCTION api_resource_recipients(token text,v uuid,r uuid,parent uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; scope uuid; rev bigint; recipients jsonb;
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
 ((scope IS NOT NULL AND (allowed(s.team_id,a.id,scope,'read') OR allowed(s.team_id,a.id,scope,'connect') OR allowed(s.team_id,a.id,scope,'edit')))
 OR EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=a.id AND role='owner'));
 RETURN jsonb_build_object('aclRevision',rev,'recipients',recipients);
END $$;
CREATE FUNCTION check_shared_envelopes(t uuid,r uuid,items jsonb) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF jsonb_typeof(items) IS DISTINCT FROM 'array' OR jsonb_array_length(items)>1000 THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023'; END IF;
 IF EXISTS((SELECT account_id,identity_version FROM shared_recipients(t,r)
 EXCEPT SELECT (e->>'accountId')::uuid,(e->>'identityVersion')::bigint FROM jsonb_array_elements(items)e)
 UNION ALL (SELECT (e->>'accountId')::uuid,(e->>'identityVersion')::bigint FROM jsonb_array_elements(items)e
 EXCEPT SELECT account_id,identity_version FROM shared_recipients(t,r))) THEN
  RAISE EXCEPTION 'recipients_changed' USING ERRCODE='40001'; END IF;
 IF jsonb_array_length(items)<>(SELECT count(*) FROM shared_recipients(t,r)) THEN RAISE EXCEPTION 'duplicate_envelopes' USING ERRCODE='22023'; END IF;
END $$;
CREATE FUNCTION api_resource_save(token text,body jsonb) RETURNS jsonb
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
  IF reason='permission' AND NOT EXISTS(SELECT FROM resource_key_envelopes WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id)
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  -- Keep the encrypted candidate without changing the record. This receipt is
  -- available to the author; it does not disclose current server data/revision.
  INSERT INTO change_operations(team_id,id,account_id,device_id,sequence,resource_id,expected_revision,request_hash,status,client_at)
  VALUES(s.team_id,op_id,s.account_id,s.device_id,seq,r,expected,request_hash,'conflict',(body->>'clientAt')::timestamptz);
  INSERT INTO conflicts(team_id,operation_id,resource_id,encrypted_candidate,reason) VALUES(s.team_id,op_id,r,snapshot,reason) RETURNING id INTO conflict_id;
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
CREATE FUNCTION shared_resource_json(t uuid,a uuid,r uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',x.id,'vaultId',x.vault_id,'kind',x.kind,'revision',x.revision,'parentId',resource_parent(t,r),
 'deleted',x.deleted,'encryptedName',encode(coalesce(f.encrypted_name,h.encrypted_name),'hex'),'encryptedAddress',encode(h.encrypted_address,'hex'),
 'encryptedUsername',encode(h.encrypted_username,'hex'),'protocol',h.protocol,'port',h.port,'usernamePolicy',h.username_policy,
 'snapshot',encode(rv.encrypted_snapshot,'hex'),'envelope',encode(e.envelope,'hex'),'identityVersion',e.identity_version,
 'permissions',(SELECT coalesce(jsonb_agg(p),'[]') FROM unnest(ARRAY['read','connect','edit','reveal','export','manage'])p WHERE allowed(t,a,r,p)))
 FROM resources x LEFT JOIN folders f ON (f.team_id,f.id)=(x.team_id,x.id) LEFT JOIN hosts h ON (h.team_id,h.id)=(x.team_id,x.id)
 JOIN resource_versions rv ON (rv.team_id,rv.resource_id,rv.revision)=(x.team_id,x.id,x.revision)
 LEFT JOIN resource_key_envelopes e ON (e.team_id,e.resource_id,e.revision,e.account_id)=(x.team_id,x.id,x.revision,a)
 AND e.identity_version=(SELECT version FROM account_crypto_identities WHERE team_id=t AND account_id=a AND active)
 WHERE x.team_id=t AND x.id=r
$$;
CREATE FUNCTION api_resource_get(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 IF NOT allowed(s.team_id,s.account_id,r,'read') THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 RETURN shared_resource_json(s.team_id,s.account_id,r);
END $$;
CREATE FUNCTION api_resources(token text,v uuid,after_id uuid,page_size integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; result jsonb;
BEGIN
 s:=session_actor(token);
 IF page_size IS NULL OR page_size NOT BETWEEN 1 AND 100 THEN RAISE EXCEPTION 'invalid_page_size' USING ERRCODE='22023'; END IF;
 SELECT coalesce(jsonb_agg(shared_resource_json(s.team_id,s.account_id,id) ORDER BY id),'[]') INTO result FROM
 (SELECT id FROM resources WHERE team_id=s.team_id AND vault_id=v AND kind IN ('folder','host') AND NOT deleted
 AND (after_id IS NULL OR id>after_id) AND allowed(s.team_id,s.account_id,id,'read') ORDER BY id LIMIT page_size)rows;
 -- Never return a total host count: it could disclose inaccessible siblings.
 RETURN jsonb_build_object('records',result,'nextAfter',CASE WHEN jsonb_array_length(result)=page_size THEN result->(page_size-1)->>'id' ELSE NULL END);
END $$;
CREATE FUNCTION api_resource_history(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 IF NOT allowed(s.team_id,s.account_id,r,'read') THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('revision',rv.revision,'snapshot',encode(rv.encrypted_snapshot,'hex'),
 'envelope',encode(e.envelope,'hex'),'identityVersion',e.identity_version,'actorId',rv.actor_id,'createdAt',rv.created_at) ORDER BY rv.revision DESC)
 FROM resource_versions rv LEFT JOIN resource_key_envelopes e ON (e.team_id,e.resource_id,e.revision,e.account_id)=(rv.team_id,rv.resource_id,rv.revision,s.account_id)
 AND e.identity_version=(SELECT version FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND active)
 WHERE rv.team_id=s.team_id AND rv.resource_id=r),'[]');
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(7,now());
COMMIT;
