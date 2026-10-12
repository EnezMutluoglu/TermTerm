BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Proposed identifiers are not shared resources. Keep them out of Hosts until
-- an authorised decision creates the record, preserving relational FKs.
CREATE TABLE creation_decisions (
 team_id uuid NOT NULL,id uuid NOT NULL DEFAULT gen_random_uuid(),operation_id uuid NOT NULL,
 resource_id uuid NOT NULL,vault_id uuid NOT NULL,parent_id uuid,encrypted_candidate bytea NOT NULL,
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','resolved','rejected')),
 decided_by uuid,decided_at timestamptz,received_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,id),UNIQUE(team_id,operation_id),
 FOREIGN KEY(team_id,operation_id) REFERENCES change_operations(team_id,id),
 FOREIGN KEY(team_id,vault_id) REFERENCES vaults(team_id,id),
 FOREIGN KEY(team_id,vault_id,parent_id) REFERENCES folders(team_id,vault_id,id),
 FOREIGN KEY(team_id,decided_by) REFERENCES team_accounts(team_id,id)
);
CREATE INDEX pending_creation_decisions ON creation_decisions(team_id,id) WHERE status='pending';
CREATE TABLE creation_decision_envelopes (
 team_id uuid NOT NULL,decision_id uuid NOT NULL,account_id uuid NOT NULL,identity_version bigint NOT NULL,
 envelope bytea NOT NULL CHECK(octet_length(envelope) BETWEEN 80 AND 4096),
 PRIMARY KEY(team_id,decision_id,account_id,identity_version),
 FOREIGN KEY(team_id,decision_id) REFERENCES creation_decisions(team_id,id),
 FOREIGN KEY(team_id,account_id,identity_version) REFERENCES account_crypto_identities(team_id,account_id,version)
);
ALTER TABLE creation_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE creation_decision_envelopes ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON creation_decisions,creation_decision_envelopes FROM PUBLIC;

ALTER FUNCTION api_offline_decision_submit(text,jsonb) RENAME TO existing_offline_decision_submit;
CREATE FUNCTION api_offline_decision_submit(token text,body jsonb) RETURNS jsonb
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
 OR (body->>'clientAt')::timestamptz IS NULL OR body->>'kind' IS NULL OR body->>'kind' NOT IN ('host','folder')
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
ALTER FUNCTION api_operation_receipt(text,uuid) RENAME TO existing_operation_receipt;
CREATE FUNCTION api_operation_receipt(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;result jsonb;d creation_decisions;
BEGIN
 s:=session_actor(token);result:=existing_operation_receipt(token,operation);
 IF result->>'found'='true' THEN
  SELECT * INTO d FROM creation_decisions WHERE team_id=s.team_id AND operation_id=operation;
  IF d.id IS NOT NULL THEN result:=result||jsonb_build_object('conflictId',d.id,'decisionStatus',d.status); END IF;
 END IF;RETURN result;
END $$;
ALTER FUNCTION api_conflicts(text,uuid,integer) RENAME TO existing_conflicts;
CREATE FUNCTION api_conflicts(token text,after_id uuid,page_size integer) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;original jsonb;result jsonb;n integer;
BEGIN
 s:=session_actor(token);original:=existing_conflicts(token,after_id,page_size);
 IF NOT EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner') THEN RETURN original; END IF;
 WITH combined AS (
  SELECT e AS value FROM jsonb_array_elements(original->'conflicts')e
  UNION ALL
  SELECT jsonb_build_object('id',d.id,'resourceId',d.resource_id,'vaultId',d.vault_id,'creation',true,
   'expectedRevision',o.expected_revision,'authorId',o.account_id,'reason','permission','receivedAt',d.received_at,
   'encryptedCandidate',encode(d.encrypted_candidate,'hex'),'envelope',encode(e.envelope,'hex'),'identityVersion',e.identity_version)
  FROM creation_decisions d JOIN change_operations o ON(o.team_id,o.id)=(d.team_id,d.operation_id)
  LEFT JOIN creation_decision_envelopes e ON(e.team_id,e.decision_id,e.account_id)=(d.team_id,d.id,s.account_id)
  AND e.identity_version=(SELECT version FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id AND active)
  WHERE d.team_id=s.team_id AND d.status='pending' AND (after_id IS NULL OR d.id>after_id)
 ), limited AS(SELECT value FROM combined ORDER BY(value->>'id')::uuid LIMIT page_size),
 bounded AS(SELECT value,sum(octet_length(value::text)) OVER(ORDER BY(value->>'id')::uuid) AS bytes FROM limited)
 SELECT coalesce(jsonb_agg(value ORDER BY(value->>'id')::uuid),'[]'),count(*) INTO result,n FROM bounded WHERE bytes<=2097152;
 RETURN jsonb_build_object('conflicts',result,'nextAfter',CASE WHEN n>0 THEN result->(n-1)->>'id' ELSE NULL END);
END $$;
CREATE FUNCTION api_conflict_current(token text,decision uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;d creation_decisions;r uuid;
BEGIN
 s:=session_actor(token);SELECT * INTO d FROM creation_decisions WHERE team_id=s.team_id AND id=decision;
 IF d.id IS NOT NULL THEN
  PERFORM require_owner(s);
  IF EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=d.resource_id) THEN RETURN api_resource_get(token,d.resource_id); END IF;
  RETURN jsonb_build_object('id',d.resource_id,'vaultId',d.vault_id,'revision',0,'creation',true);
 END IF;
 SELECT resource_id INTO r FROM conflicts WHERE team_id=s.team_id AND id=decision;
 IF r IS NULL OR NOT allowed(s.team_id,s.account_id,r,'edit') THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 RETURN api_resource_get(token,r);
END $$;
ALTER FUNCTION api_conflict_decide(text,uuid,bigint,text,jsonb) RENAME TO existing_conflict_decide;
CREATE FUNCTION api_conflict_decide(token text,c uuid,current_revision bigint,choice text,change jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;d creation_decisions;rev bigint;result jsonb;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO d FROM creation_decisions WHERE team_id=s.team_id AND id=c FOR UPDATE;
 IF d.id IS NULL THEN RETURN existing_conflict_decide(token,c,current_revision,choice,change); END IF;
 PERFORM require_owner(s);
 IF d.status<>'pending' THEN RAISE EXCEPTION 'decision_already_taken' USING ERRCODE='40001'; END IF;
 SELECT coalesce((SELECT revision FROM resources WHERE team_id=s.team_id AND id=d.resource_id),0) INTO rev;
 IF current_revision IS DISTINCT FROM rev THEN RAISE EXCEPTION 'revision_changed' USING ERRCODE='40001'; END IF;
 IF choice='server' THEN
  IF change IS NOT NULL AND change<>'null'::jsonb THEN RAISE EXCEPTION 'unexpected_change' USING ERRCODE='22023'; END IF;
  result:=jsonb_build_object('status','resolved','revision',rev);
 ELSIF choice='replacement' THEN
  IF change IS NULL OR (change->>'resourceId')::uuid IS DISTINCT FROM d.resource_id OR (change->>'vaultId')::uuid IS DISTINCT FROM d.vault_id
  OR (change->>'parentId')::uuid IS DISTINCT FROM d.parent_id OR (change->>'expectedRevision')::bigint IS DISTINCT FROM rev
  THEN RAISE EXCEPTION 'invalid_replacement' USING ERRCODE='22023'; END IF;
  result:=api_resource_save(token,change);
  IF result->>'status'<>'applied' THEN RAISE EXCEPTION 'replacement_not_applied' USING ERRCODE='40001'; END IF;
 ELSE RAISE EXCEPTION 'invalid_choice' USING ERRCODE='22023'; END IF;
 UPDATE creation_decisions SET status=CASE WHEN choice='server' THEN 'rejected' ELSE 'resolved' END,decided_by=s.account_id,decided_at=clock_timestamp() WHERE team_id=s.team_id AND id=c;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'creation.'||choice,'succeeded');
 RETURN result;
END $$;
REVOKE ALL ON FUNCTION existing_offline_decision_submit(text,jsonb),existing_operation_receipt(text,uuid),existing_conflicts(text,uuid,integer),existing_conflict_decide(text,uuid,bigint,text,jsonb),
 api_offline_decision_submit(text,jsonb),api_operation_receipt(text,uuid),api_conflicts(text,uuid,integer),api_conflict_current(text,uuid),api_conflict_decide(text,uuid,bigint,text,jsonb) FROM PUBLIC;
INSERT INTO schema_migrations VALUES(44,now());
COMMIT;
