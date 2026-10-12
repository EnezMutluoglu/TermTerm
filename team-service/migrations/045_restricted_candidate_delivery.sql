BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- Device proof is usable only for prior-authorised candidate delivery. It never
-- establishes an auth session and survives account disable/password reset.
CREATE TABLE offline_delivery_tokens (
 team_id uuid NOT NULL,account_id uuid NOT NULL,device_id uuid NOT NULL,id uuid NOT NULL DEFAULT gen_random_uuid(),
 token_hash bytea NOT NULL UNIQUE CHECK(octet_length(token_hash)=32),issued_at timestamptz NOT NULL DEFAULT now(),
 expires_at timestamptz NOT NULL DEFAULT now()+interval '90 days',revoked_at timestamptz,
 PRIMARY KEY(team_id,id),FOREIGN KEY(team_id,account_id,device_id) REFERENCES account_devices(team_id,account_id,id)
);
CREATE INDEX delivery_tokens_device ON offline_delivery_tokens(team_id,account_id,device_id,expires_at);
ALTER TABLE offline_delivery_tokens ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON offline_delivery_tokens FROM PUBLIC;
CREATE FUNCTION api_delivery_token(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;proof text:=encode(public.gen_random_bytes(32),'hex');expiry timestamptz;
BEGIN
 s:=session_actor(token);
 INSERT INTO offline_delivery_tokens(team_id,account_id,device_id,token_hash)
 VALUES(s.team_id,s.account_id,s.device_id,public.digest(proof,'sha256')) RETURNING expires_at INTO expiry;
 RETURN jsonb_build_object('proof',proof,'expiresAtMs',floor(extract(epoch from expiry)*1000)::bigint);
END $$;
CREATE FUNCTION candidate_actor(proof text) RETURNS auth_sessions
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;d offline_delivery_tokens;
BEGIN
 BEGIN RETURN session_actor(proof); EXCEPTION WHEN SQLSTATE '28000' THEN NULL; END;
 SELECT * INTO d FROM offline_delivery_tokens WHERE token_hash=public.digest(proof,'sha256')
 AND expires_at>clock_timestamp() AND revoked_at IS NULL;
 IF d.id IS NULL THEN RAISE EXCEPTION 'invalid_delivery_proof' USING ERRCODE='28000'; END IF;
 s.team_id:=d.team_id;s.account_id:=d.account_id;s.device_id:=d.device_id;
 RETURN s;
END $$;

CREATE OR REPLACE FUNCTION api_offline_decision_prepare(token text,capability uuid,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; recipients jsonb;
BEGIN
 s:=candidate_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT EXISTS(SELECT FROM resource_edit_receipts WHERE team_id=s.team_id AND id=capability
 AND account_id=s.account_id AND device_id=s.device_id AND resource_id=r)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT coalesce(jsonb_agg(jsonb_build_object('accountId',account_id,'identityVersion',identity_version,
 'publicKey',encode(public_key,'hex')) ORDER BY account_id),'[]') INTO recipients FROM offline_decision_recipients(s.team_id,r);
 RETURN jsonb_build_object('writeCapability',capability,'aclRevision',(SELECT acl_revision FROM teams WHERE id=s.team_id),'recipients',recipients);
END $$;

CREATE OR REPLACE FUNCTION existing_offline_decision_submit(token text,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;cap resource_edit_receipts;op change_operations;op_id uuid:=(body->>'operationId')::uuid;
 r uuid:=(body->>'resourceId')::uuid;expected bigint:=(body->>'expectedRevision')::bigint;
 seq bigint:=(body->>'sequence')::bigint;snapshot bytea:=decode(body->>'snapshot','hex');
 envelopes jsonb:=body->'envelopes'; request_hash bytea:=public.digest(body::text,'sha256');conflict_id uuid;
BEGIN
 s:=candidate_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF op_id IS NULL OR r IS NULL OR expected IS NULL OR expected<0 OR expected=9223372036854775807
 OR seq IS NULL OR seq<=0 OR snapshot IS NULL OR octet_length(snapshot) NOT BETWEEN 40 AND 131072
 OR (body->>'clientAt')::timestamptz IS NULL THEN RAISE EXCEPTION 'invalid_operation' USING ERRCODE='22023'; END IF;
 SELECT * INTO op FROM change_operations WHERE team_id=s.team_id AND id=op_id;
 IF op.id IS NOT NULL THEN
  IF op.account_id<>s.account_id OR op.device_id<>s.device_id OR op.request_hash<>request_hash
  THEN RAISE EXCEPTION 'operation_reuse' USING ERRCODE='22023'; END IF;
  RETURN jsonb_build_object('operationId',op.id,'status',op.status,'revision',op.result_revision,
   'conflictId',(SELECT id FROM conflicts WHERE team_id=s.team_id AND operation_id=op.id));
 END IF;
 SELECT * INTO cap FROM resource_edit_receipts WHERE team_id=s.team_id AND id=(body->>'writeCapability')::uuid
 AND account_id=s.account_id AND device_id=s.device_id AND resource_id=r AND vault_id=(body->>'vaultId')::uuid;
 IF cap.id IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 PERFORM id FROM team_accounts WHERE team_id=s.team_id AND status='active' ORDER BY id FOR SHARE;
 IF (body->>'aclRevision')::bigint IS DISTINCT FROM (SELECT acl_revision FROM teams WHERE id=s.team_id)
 THEN RAISE EXCEPTION 'acl_changed' USING ERRCODE='40001'; END IF;
 IF jsonb_typeof(envelopes) IS DISTINCT FROM 'array' OR jsonb_array_length(envelopes) NOT BETWEEN 1 AND 1000
 THEN RAISE EXCEPTION 'invalid_envelopes' USING ERRCODE='22023'; END IF;
 IF jsonb_array_length(envelopes)<>(SELECT count(*) FROM offline_decision_recipients(s.team_id,r))
 OR EXISTS(SELECT FROM jsonb_array_elements(envelopes)e WHERE NOT EXISTS(
 SELECT FROM offline_decision_recipients(s.team_id,r) current WHERE current.account_id=(e->>'accountId')::uuid
 AND current.identity_version=(e->>'identityVersion')::bigint))
 OR EXISTS(SELECT FROM jsonb_array_elements(envelopes)e GROUP BY e->>'accountId' HAVING count(*)>1)
 THEN RAISE EXCEPTION 'recipients_changed' USING ERRCODE='40001'; END IF;
 INSERT INTO change_operations(team_id,id,account_id,device_id,sequence,resource_id,expected_revision,request_hash,status,client_at)
 VALUES(s.team_id,op_id,s.account_id,s.device_id,seq,r,expected,request_hash,'conflict',(body->>'clientAt')::timestamptz);
 INSERT INTO conflicts(team_id,operation_id,resource_id,encrypted_candidate,reason)
 VALUES(s.team_id,op_id,r,snapshot,'permission') RETURNING id INTO conflict_id;
 INSERT INTO conflict_key_envelopes(team_id,conflict_id,account_id,identity_version,envelope)
 SELECT s.team_id,conflict_id,(e->>'accountId')::uuid,(e->>'identityVersion')::bigint,decode(e->>'envelope','hex') FROM jsonb_array_elements(envelopes)e;
 INSERT INTO audit_events(team_id,actor_id,resource_id,action,outcome) VALUES(s.team_id,s.account_id,r,'offline.candidate_received','succeeded');
 RETURN jsonb_build_object('operationId',op_id,'status','conflict','conflictId',conflict_id);
END $$;

CREATE OR REPLACE FUNCTION api_creation_claim(token text,scope uuid,r uuid,v uuid,parent uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;cap offline_creation_scopes;receipt uuid;
BEGIN
 s:=candidate_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO cap FROM offline_creation_scopes WHERE team_id=s.team_id AND id=scope
 AND account_id=s.account_id AND device_id=s.device_id AND vault_id=v AND parent_id IS NOT DISTINCT FROM parent;
 IF cap.id IS NULL OR r IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT receipt_id INTO receipt FROM offline_creation_claims WHERE team_id=s.team_id AND scope_id=scope AND resource_id=r;
 IF receipt IS NOT NULL THEN RETURN jsonb_build_object('writeCapability',receipt); END IF;
 -- A historical scope cannot be used to impersonate an edit of an existing
 -- record, nor can a different scope claim an already reserved identifier.
 IF EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=r)
 OR EXISTS(SELECT FROM offline_creation_claims WHERE team_id=s.team_id AND resource_id=r)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 INSERT INTO resource_edit_receipts(team_id,account_id,device_id,vault_id,resource_id,acl_revision)
 VALUES(s.team_id,s.account_id,s.device_id,v,r,cap.acl_revision) RETURNING id INTO receipt;
 INSERT INTO offline_creation_claims VALUES(s.team_id,scope,r,receipt);
 RETURN jsonb_build_object('writeCapability',receipt);
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

CREATE OR REPLACE FUNCTION existing_operation_receipt(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;op change_operations;
BEGIN
 s:=candidate_actor(token);
 IF operation IS NULL THEN RAISE EXCEPTION 'invalid_operation' USING ERRCODE='22023'; END IF;
 SELECT * INTO op FROM change_operations WHERE team_id=s.team_id AND id=operation AND account_id=s.account_id AND device_id=s.device_id;
 IF op.id IS NULL THEN RETURN jsonb_build_object('found',false); END IF;
 RETURN jsonb_build_object('found',true,'operationId',op.id,'status',op.status,'revision',op.result_revision,
  'conflictId',(SELECT id FROM conflicts WHERE team_id=s.team_id AND operation_id=op.id),
  'decisionStatus',(SELECT status FROM conflicts WHERE team_id=s.team_id AND operation_id=op.id));
END $$;

CREATE OR REPLACE FUNCTION api_operation_receipt(token text,operation uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;result jsonb;d creation_decisions;
BEGIN
 s:=candidate_actor(token);result:=existing_operation_receipt(token,operation);
 IF result->>'found'='true' THEN
  SELECT * INTO d FROM creation_decisions WHERE team_id=s.team_id AND operation_id=operation;
  IF d.id IS NOT NULL THEN result:=result||jsonb_build_object('conflictId',d.id,'decisionStatus',d.status); END IF;
 END IF;RETURN result;
END $$;

REVOKE ALL ON FUNCTION api_delivery_token(text),candidate_actor(text) FROM PUBLIC;
INSERT INTO schema_migrations VALUES(45,now());
COMMIT;
