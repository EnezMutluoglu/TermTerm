BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- A revoked editor may deliver a candidate, never fetch the resource or change
-- it. Recipients are recalculated from current authority on every submission.
CREATE FUNCTION offline_decision_recipients(t uuid,r uuid)
RETURNS TABLE(account_id uuid,identity_version bigint,public_key bytea)
LANGUAGE sql STABLE AS $$
 SELECT a.id,i.version,i.public_key FROM team_accounts a
 JOIN account_crypto_identities i ON (i.team_id,i.account_id)=(a.team_id,a.id) AND i.active
 WHERE a.team_id=t AND a.status='active' AND
 (allowed(t,a.id,r,'edit') OR EXISTS(SELECT FROM team_memberships WHERE team_id=t AND account_id=a.id AND role='owner'))
$$;
CREATE FUNCTION api_offline_decision_prepare(token text,capability uuid,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; recipients jsonb;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT EXISTS(SELECT FROM resource_edit_receipts WHERE team_id=s.team_id AND id=capability
 AND account_id=s.account_id AND device_id=s.device_id AND resource_id=r)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT coalesce(jsonb_agg(jsonb_build_object('accountId',account_id,'identityVersion',identity_version,
 'publicKey',encode(public_key,'hex')) ORDER BY account_id),'[]') INTO recipients FROM offline_decision_recipients(s.team_id,r);
 RETURN jsonb_build_object('writeCapability',capability,'aclRevision',(SELECT acl_revision FROM teams WHERE id=s.team_id),'recipients',recipients);
END $$;
CREATE FUNCTION api_offline_decision_submit(token text,body jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;cap resource_edit_receipts;op change_operations;op_id uuid:=(body->>'operationId')::uuid;
 r uuid:=(body->>'resourceId')::uuid;expected bigint:=(body->>'expectedRevision')::bigint;
 seq bigint:=(body->>'sequence')::bigint;snapshot bytea:=decode(body->>'snapshot','hex');
 envelopes jsonb:=body->'envelopes'; request_hash bytea:=public.digest(body::text,'sha256');conflict_id uuid;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
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
REVOKE ALL ON FUNCTION offline_decision_recipients(uuid,uuid),api_offline_decision_prepare(text,uuid,uuid),api_offline_decision_submit(text,jsonb) FROM PUBLIC;
INSERT INTO schema_migrations VALUES(35,now());
COMMIT;
