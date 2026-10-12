BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE TABLE shared_recovery_plans (
 team_id uuid NOT NULL, plan_id uuid NOT NULL, credential_id uuid NOT NULL, actor_id uuid NOT NULL,
 reason text NOT NULL CHECK(length(reason) BETWEEN 8 AND 2000), created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,plan_id), FOREIGN KEY(team_id,plan_id) REFERENCES credential_update_plans(team_id,id) ON DELETE CASCADE,
 FOREIGN KEY(team_id,credential_id) REFERENCES credentials(team_id,id), FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id)
);
CREATE INDEX shared_recovery_actor ON shared_recovery_plans(team_id,actor_id,created_at DESC);
ALTER TABLE shared_recovery_plans ENABLE ROW LEVEL SECURITY;
CREATE FUNCTION api_shared_recovery_begin(token text,c uuid,password text,pub bytea,reason_input text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;options jsonb;recipients jsonb;plan jsonb;key_id uuid;record jsonb;revision bigint;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 IF NOT reauthenticate(s,password) THEN RETURN '{"error":"invalid_credentials"}'; END IF;
 IF length(reason_input) NOT BETWEEN 8 AND 2000 THEN RAISE EXCEPTION 'recovery_reason_required' USING ERRCODE='22023'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,418));
 SELECT id INTO key_id FROM team_recovery_keys WHERE team_id=s.team_id AND active AND public_key=pub;
 IF key_id IS NULL THEN RAISE EXCEPTION 'recovery_key_mismatch' USING ERRCODE='40001'; END IF;
 options:=api_shared_update_options(token,c);revision:=(options->>'revision')::bigint;
 -- Preserve exactly today's grants, never the old encrypted copy's recipients.
 SELECT coalesce(jsonb_agg(x-'displayName'),'[]') INTO recipients FROM jsonb_array_elements(options->'recipients')x
 WHERE (x->>'connect')::boolean OR (x->>'reveal')::boolean OR (x->>'export')::boolean;
 plan:=api_shared_update_prepare(token,c,revision,NULL,recipients);
 SELECT jsonb_build_object('id',c,'revision',revision,'kind',cr.kind,'encryptedData',encode(v.encrypted_data,'hex'),
 'encryptedSecret',encode(v.encrypted_secret,'hex'),'envelopes',(SELECT jsonb_agg(jsonb_build_object('purpose',e.purpose,'envelope',encode(e.envelope,'hex')))
 FROM credential_key_envelopes e WHERE (e.team_id,e.credential_id,e.revision,e.recovery_key_id)=(s.team_id,c,revision,key_id))) INTO record
 FROM credential_versions v JOIN credentials cr ON(cr.team_id,cr.id)=(v.team_id,v.credential_id)
 WHERE v.team_id=s.team_id AND v.credential_id=c AND v.revision=revision AND cr.visibility='shared';
 IF record IS NULL OR jsonb_array_length(record->'envelopes') IS DISTINCT FROM 2 THEN RAISE EXCEPTION 'recovery_incomplete' USING ERRCODE='23514'; END IF;
 INSERT INTO shared_recovery_plans(team_id,plan_id,credential_id,actor_id,reason) VALUES(s.team_id,(plan->>'id')::uuid,c,s.account_id,reason_input);
 INSERT INTO audit_events(team_id,actor_id,action,resource_id,outcome) VALUES(s.team_id,s.account_id,'recovery.shared_access',c,'authorized');
 RETURN jsonb_build_object('plan',plan,'record',record);
END $$;
REVOKE ALL ON FUNCTION api_shared_recovery_begin(text,uuid,text,bytea,text) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(48);
COMMIT;
