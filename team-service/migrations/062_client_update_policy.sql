BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE TABLE client_update_policies (
 team_id uuid PRIMARY KEY REFERENCES teams(id), minimum_version text NOT NULL CHECK(length(minimum_version) BETWEEN 5 AND 80),
 revision bigint NOT NULL CHECK(revision>0), updated_by uuid NOT NULL, updated_at timestamptz NOT NULL DEFAULT now(),
 FOREIGN KEY(team_id,updated_by) REFERENCES team_accounts(team_id,id)
);
CREATE TABLE client_update_history (
 team_id uuid NOT NULL REFERENCES teams(id), revision bigint NOT NULL, minimum_version text NOT NULL,
 actor_id uuid NOT NULL, server_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,revision), FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id)
);
ALTER TABLE client_update_policies ENABLE ROW LEVEL SECURITY;
ALTER TABLE client_update_history ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON client_update_policies,client_update_history FROM PUBLIC;

-- Public compatibility metadata only: this never authenticates an account.
CREATE FUNCTION client_policy_public(t uuid) RETURNS jsonb LANGUAGE sql STABLE SECURITY DEFINER
SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
 SELECT coalesce((SELECT jsonb_build_object('selectedVersion',minimum_version,'revision',revision,'updatedAt',updated_at)
 FROM client_update_policies WHERE team_id=t),jsonb_build_object('selectedVersion',NULL,'revision',0))
$$;
-- Derive scope from server-held proof hashes, never from a client team header.
-- Authentication/expiry checks remain in the actual operation function.
CREATE FUNCTION client_policy_proof(token text) RETURNS jsonb LANGUAGE sql STABLE SECURITY DEFINER
SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
 SELECT client_policy_public((SELECT team_id FROM (
 SELECT team_id FROM auth_sessions WHERE access_hash=public.digest(token,'sha256')
 UNION ALL SELECT team_id FROM auth_sessions WHERE refresh_hash=public.digest(token,'sha256')
 UNION ALL SELECT team_id FROM offline_delivery_tokens WHERE token_hash=public.digest(token,'sha256')
 )scopes LIMIT 1))
$$;
CREATE FUNCTION api_client_policy(token text) RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER
SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN s:=session_actor(token);PERFORM require_owner(s);RETURN client_policy_public(s.team_id);END $$;
CREATE FUNCTION api_client_policy_set(token text, expected bigint, target text) RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER
SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; p client_update_policies; next_revision bigint;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 IF target IS NULL OR target !~ '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' OR length(target)>80 OR expected IS NULL OR expected<0 THEN
  RAISE EXCEPTION 'invalid_version' USING ERRCODE='22023'; END IF;
 PERFORM 1 FROM teams WHERE id=s.team_id FOR UPDATE;
 SELECT * INTO p FROM client_update_policies WHERE team_id=s.team_id;
 IF p.revision IS NOT NULL AND p.revision=expected+1 AND p.minimum_version=target THEN RETURN client_policy_public(s.team_id); END IF;
 IF coalesce(p.revision,0)<>expected THEN RAISE EXCEPTION 'revision_conflict' USING ERRCODE='40001'; END IF;
 IF p.minimum_version=target THEN RETURN client_policy_public(s.team_id); END IF;
 next_revision:=coalesce(p.revision,0)+1;
 INSERT INTO client_update_policies VALUES(s.team_id,target,next_revision,s.account_id,clock_timestamp())
 ON CONFLICT(team_id) DO UPDATE SET minimum_version=excluded.minimum_version,revision=excluded.revision,updated_by=excluded.updated_by,updated_at=excluded.updated_at;
 INSERT INTO client_update_history(team_id,revision,minimum_version,actor_id) VALUES(s.team_id,next_revision,target,s.account_id);
 -- Keep a bounded policy history; audit follows the existing retention policy.
 DELETE FROM client_update_history WHERE team_id=s.team_id AND revision<next_revision-10;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'client_update.required','succeeded');
 RETURN client_policy_public(s.team_id);
END $$;
REVOKE ALL ON FUNCTION client_policy_public(uuid),client_policy_proof(text),api_client_policy(text),api_client_policy_set(text,bigint,text) FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(62);
COMMIT;
