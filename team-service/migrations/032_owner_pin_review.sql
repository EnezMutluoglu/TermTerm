BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
ALTER TABLE revision_pins ADD COLUMN id bigint GENERATED ALWAYS AS IDENTITY;
ALTER TABLE revision_pins ADD COLUMN created_at timestamptz NOT NULL DEFAULT clock_timestamp();
CREATE UNIQUE INDEX revision_pin_id ON revision_pins(team_id,id);
CREATE TABLE pin_release_plans (
 team_id uuid NOT NULL,id uuid NOT NULL DEFAULT gen_random_uuid(),actor_id uuid NOT NULL,
 expires_at timestamptz NOT NULL DEFAULT now()+interval '15 minutes',status text NOT NULL DEFAULT 'pending' CHECK(status IN('pending','applied','cancelled')),
 released integer,pruned integer,PRIMARY KEY(team_id,id),FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id)
);
CREATE TABLE pin_release_items (
 team_id uuid NOT NULL,plan_id uuid NOT NULL,pin_id bigint NOT NULL,resource_id uuid NOT NULL,revision bigint NOT NULL,
 account_id uuid NOT NULL,purpose text NOT NULL,created_at timestamptz NOT NULL,
 PRIMARY KEY(team_id,plan_id,pin_id),FOREIGN KEY(team_id,plan_id) REFERENCES pin_release_plans(team_id,id) ON DELETE CASCADE
);
CREATE INDEX pin_release_expiry ON pin_release_plans(expires_at) WHERE status<>'applied';
ALTER TABLE pin_release_plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE pin_release_items ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON pin_release_plans,pin_release_items FROM PUBLIC;
CREATE FUNCTION api_pins_review(token text,after_id bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;items jsonb;last_id bigint;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);
 IF after_id<0 THEN RAISE EXCEPTION 'invalid_cursor' USING ERRCODE='22023';END IF;
 SELECT coalesce(jsonb_agg(jsonb_build_object('id',p.id::text,'accountId',p.account_id,'displayName',a.display_name,'accountStatus',a.status,
 'resourceId',p.resource_id,'revision',p.revision,'purpose',p.purpose,'createdAt',p.created_at,'record',shared_resource_json(s.team_id,s.account_id,p.resource_id)) ORDER BY p.id),'[]'),max(p.id)
 INTO items,last_id FROM(SELECT * FROM revision_pins WHERE team_id=s.team_id AND id>coalesce(after_id,0) ORDER BY id LIMIT 20)p
 JOIN team_accounts a ON(a.team_id,a.id)=(p.team_id,p.account_id);
 RETURN jsonb_build_object('items',items,'nextAfter',CASE WHEN EXISTS(SELECT FROM revision_pins WHERE team_id=s.team_id AND id>last_id) THEN last_id::text ELSE NULL END);
END $$;
CREATE FUNCTION api_pins_prepare(token text,ids jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;plan uuid;n integer;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF jsonb_typeof(ids) IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'invalid_pins' USING ERRCODE='22023';END IF;
 n:=jsonb_array_length(ids);
 IF n NOT BETWEEN 1 AND 100 OR(SELECT count(DISTINCT x) FROM jsonb_array_elements_text(ids)x)<>n
 THEN RAISE EXCEPTION 'invalid_pins' USING ERRCODE='22023';END IF;
 IF(SELECT count(*) FROM pin_release_plans WHERE team_id=s.team_id AND actor_id=s.account_id AND status='pending' AND expires_at>now())>=20
 THEN RAISE EXCEPTION 'too_many_plans' USING ERRCODE='22023';END IF;
 INSERT INTO pin_release_plans(team_id,actor_id) VALUES(s.team_id,s.account_id) RETURNING id INTO plan;
 INSERT INTO pin_release_items SELECT s.team_id,plan,p.id,p.resource_id,p.revision,p.account_id,p.purpose,p.created_at
 FROM revision_pins p WHERE p.team_id=s.team_id AND p.id IN(SELECT x::bigint FROM jsonb_array_elements_text(ids)x);
 IF (SELECT count(*) FROM pin_release_items WHERE team_id=s.team_id AND plan_id=plan)<>n THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001';END IF;
 RETURN jsonb_build_object('id',plan,'pins',n,'items',(SELECT jsonb_agg(jsonb_build_object('id',i.pin_id::text,'displayName',a.display_name,
 'resourceId',i.resource_id,'revision',i.revision,'purpose',i.purpose)) FROM pin_release_items i JOIN team_accounts a ON(a.team_id,a.id)=(i.team_id,i.account_id) WHERE i.team_id=s.team_id AND i.plan_id=plan));
END $$;
CREATE FUNCTION api_pins_finish(token text,plan uuid,cancel boolean) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;p pin_release_plans;n integer;k integer;
BEGIN
 s:=session_actor(token);PERFORM require_owner(s);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO p FROM pin_release_plans WHERE team_id=s.team_id AND id=plan AND actor_id=s.account_id FOR UPDATE;
 IF p.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002';END IF;
 IF p.status='applied' THEN
  IF cancel THEN RAISE EXCEPTION 'already_applied' USING ERRCODE='22023';END IF;
  RETURN jsonb_build_object('released',p.released,'pruned',p.pruned);
 END IF;
 IF cancel THEN UPDATE pin_release_plans SET status='cancelled' WHERE team_id=s.team_id AND id=plan;RETURN '{"status":"cancelled"}';END IF;
 IF p.status<>'pending' OR p.expires_at<=now() OR EXISTS(SELECT FROM pin_release_items i WHERE i.team_id=s.team_id AND i.plan_id=plan AND NOT EXISTS(
 SELECT FROM revision_pins rp WHERE rp.team_id=i.team_id AND rp.id=i.pin_id AND rp.resource_id=i.resource_id AND rp.revision=i.revision AND rp.account_id=i.account_id AND rp.purpose=i.purpose AND rp.created_at=i.created_at))
 THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001';END IF;
 DELETE FROM revision_pins WHERE team_id=s.team_id AND id IN(SELECT pin_id FROM pin_release_items WHERE team_id=s.team_id AND plan_id=plan);GET DIAGNOSTICS n=ROW_COUNT;
 DELETE FROM resource_versions rv USING resources x WHERE (x.team_id,x.id)=(rv.team_id,rv.resource_id) AND rv.team_id=s.team_id AND rv.revision<x.revision-10
 AND rv.resource_id IN(SELECT resource_id FROM pin_release_items WHERE team_id=s.team_id AND plan_id=plan)
 AND NOT EXISTS(SELECT FROM revision_pins rp WHERE (rp.team_id,rp.resource_id,rp.revision)=(rv.team_id,rv.resource_id,rv.revision));GET DIAGNOSTICS k=ROW_COUNT;
 UPDATE pin_release_plans SET status='applied',released=n,pruned=k WHERE team_id=s.team_id AND id=plan;
 INSERT INTO audit_events(team_id,actor_id,subject_id,action,resource_id,outcome)
 SELECT s.team_id,s.account_id,account_id,'history.pin_release',resource_id,'succeeded' FROM pin_release_items WHERE team_id=s.team_id AND plan_id=plan;
 RETURN jsonb_build_object('released',n,'pruned',k);
END $$;
-- Global physical size is explicitly separate from per-team encrypted payload.
ALTER FUNCTION api_storage(text) RENAME TO storage_v31;
CREATE FUNCTION api_storage(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE answer jsonb;
BEGIN
 answer:=storage_v31(token);
 RETURN answer||jsonb_build_object('databasePhysicalBytes',pg_database_size(current_database())::text,
 'teamSchemaTableBytes',(SELECT coalesce(sum(pg_table_size(c.oid)),0)::text FROM pg_class c JOIN pg_namespace ns ON ns.oid=c.relnamespace WHERE ns.nspname='termterm_team_v2' AND c.relkind='r'),
 'teamSchemaIndexBytes',(SELECT coalesce(sum(pg_indexes_size(c.oid)),0)::text FROM pg_class c JOIN pg_namespace ns ON ns.oid=c.relnamespace WHERE ns.nspname='termterm_team_v2' AND c.relkind='r'));
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
DO $$ DECLARE n text;BEGIN FOR n IN SELECT DISTINCT grantee::regrole::text FROM pg_proc p,aclexplode(coalesce(p.proacl,acldefault('f',p.proowner))) a
 WHERE p.oid='storage_v31(text)'::regprocedure AND a.grantee<>p.proowner AND a.grantee<>0 LOOP EXECUTE format('REVOKE ALL ON FUNCTION storage_v31(text) FROM %s',n);END LOOP;END $$;
INSERT INTO schema_migrations(version) VALUES(32);
COMMIT;
