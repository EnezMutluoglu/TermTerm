BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE TABLE connection_route_snapshots (
 team_id uuid NOT NULL,host_id uuid NOT NULL,resource_revision bigint NOT NULL,
 PRIMARY KEY(team_id,host_id,resource_revision),
 FOREIGN KEY(team_id,host_id,resource_revision) REFERENCES resource_versions(team_id,resource_id,revision) ON DELETE CASCADE
);
CREATE TABLE connection_route_steps (
 team_id uuid NOT NULL,host_id uuid NOT NULL,resource_revision bigint NOT NULL,position smallint NOT NULL CHECK(position BETWEEN 0 AND 14),jump_host_id uuid NOT NULL,
 PRIMARY KEY(team_id,host_id,resource_revision,position),UNIQUE(team_id,host_id,resource_revision,jump_host_id),
 FOREIGN KEY(team_id,host_id,resource_revision) REFERENCES connection_route_snapshots(team_id,host_id,resource_revision) ON DELETE CASCADE,
 FOREIGN KEY(team_id,jump_host_id) REFERENCES hosts(team_id,id),CHECK(host_id<>jump_host_id)
);
CREATE INDEX connection_route_dependency ON connection_route_steps(team_id,jump_host_id,host_id);
ALTER TABLE connection_route_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE connection_route_steps ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON connection_route_snapshots,connection_route_steps FROM PUBLIC;
CREATE FUNCTION snapshot_connection_route() RETURNS trigger LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
BEGIN
 IF EXISTS(SELECT FROM hosts WHERE team_id=NEW.team_id AND id=NEW.resource_id) THEN
  INSERT INTO connection_route_snapshots VALUES(NEW.team_id,NEW.resource_id,NEW.revision);
  INSERT INTO connection_route_steps SELECT team_id,host_id,NEW.revision,position,jump_host_id FROM host_hops WHERE team_id=NEW.team_id AND host_id=NEW.resource_id;
 END IF;
 RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION snapshot_connection_route() FROM PUBLIC;
CREATE TRIGGER resource_route_snapshot AFTER INSERT ON resource_versions FOR EACH ROW EXECUTE FUNCTION snapshot_connection_route();
-- Only the current route is known during upgrade; never invent older routes.
INSERT INTO connection_route_snapshots SELECT r.team_id,r.id,r.revision FROM resources r JOIN hosts h ON (h.team_id,h.id)=(r.team_id,r.id) JOIN resource_versions v ON (v.team_id,v.resource_id,v.revision)=(r.team_id,r.id,r.revision);
INSERT INTO connection_route_steps SELECT h.team_id,h.host_id,r.revision,h.position,h.jump_host_id FROM host_hops h JOIN resources r ON (r.team_id,r.id)=(h.team_id,h.host_id) JOIN connection_route_snapshots s ON (s.team_id,s.host_id,s.resource_revision)=(r.team_id,r.id,r.revision);
CREATE FUNCTION api_chain_finish(token text,p uuid,items jsonb,change jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;plan connection_change_plans;result jsonb;expected bigint;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO plan FROM connection_change_plans WHERE team_id=s.team_id AND id=p AND actor_id=s.account_id FOR UPDATE;
 IF plan.id IS NULL THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 expected:=(plan.manifest->>'targetRevision')::bigint;
 IF change->>'kind' IS DISTINCT FROM 'host' OR (change->>'resourceId')::uuid IS DISTINCT FROM plan.host_id
 OR (change->>'expectedRevision')::bigint IS DISTINCT FROM expected OR (change->>'deleted')::boolean IS DISTINCT FROM false
 THEN RAISE EXCEPTION 'invalid_chain_revision' USING ERRCODE='22023'; END IF;
 IF plan.applied THEN
  -- Resource-save also binds the complete ciphertext/operation to the receipt.
  IF plan.body_hash IS DISTINCT FROM sha256(convert_to(items::text,'UTF8')) THEN RAISE EXCEPTION 'operation_mismatch' USING ERRCODE='22023'; END IF;
  IF NOT EXISTS(SELECT FROM change_operations WHERE team_id=s.team_id AND id=(change->>'operationId')::uuid AND resource_id=plan.host_id AND result_revision=expected+1)
  THEN RAISE EXCEPTION 'operation_mismatch' USING ERRCODE='22023'; END IF;
 ELSE
  PERFORM api_chain_finish(token,p,items);
 END IF;
 result:=api_resource_save(token,change);
 IF result->>'status'<>'applied' OR (result->>'revision')::bigint<>expected+1 THEN RAISE EXCEPTION 'plan_changed' USING ERRCODE='40001'; END IF;
 RETURN result||jsonb_build_object('planId',p);
END $$;
REVOKE ALL ON FUNCTION api_chain_finish(text,uuid,jsonb,jsonb) FROM PUBLIC;
CREATE OR REPLACE FUNCTION api_connection_plan(token text,r uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;hop record;row jsonb;steps jsonb:='[]';c uuid;m text;dep bytea;selected bigint;
BEGIN
 s:=session_actor(token);
 IF NOT allowed(s.team_id,s.account_id,r,'connect') OR NOT EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=r)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT revision INTO selected FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='temporary';
 IF selected IS NOT NULL AND NOT EXISTS(SELECT FROM connection_route_snapshots WHERE team_id=s.team_id AND host_id=r AND resource_revision=selected)
 THEN RAISE EXCEPTION 'history_route_unavailable' USING ERRCODE='22023'; END IF;
 FOR hop IN SELECT jump_host_id AS id,position FROM host_hops WHERE team_id=s.team_id AND host_id=r AND selected IS NULL
 UNION ALL SELECT jump_host_id,position FROM connection_route_steps WHERE team_id=s.team_id AND host_id=r AND resource_revision=selected
 UNION ALL SELECT r,16 ORDER BY position LOOP
  IF NOT EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=hop.id AND NOT deleted)
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  row:=shared_resource_json(s.team_id,s.account_id,hop.id);
  IF hop.id<>r THEN
   SELECT e.envelope INTO dep FROM connection_dependency_envelopes e JOIN account_crypto_identities i
   ON (i.team_id,i.account_id,i.version)=(e.team_id,e.account_id,e.identity_version) AND i.active
   WHERE e.team_id=s.team_id AND e.host_id=r AND e.jump_host_id=hop.id AND e.account_id=s.account_id AND e.resource_revision=(row->>'revision')::bigint;
   IF dep IS NULL THEN
    IF selected IS NULL OR NOT allowed(s.team_id,s.account_id,hop.id,'connect') OR row->>'envelope' IS NULL
    THEN RAISE EXCEPTION 'chain_refresh_required' USING ERRCODE='42501'; END IF;
    row:=(row-'permissions')||jsonb_build_object('directEnvelope',true);
   ELSE
    row:=(row-'envelope'-'permissions')||jsonb_build_object('dependencyEnvelope',encode(dep,'hex'));
   END IF;
  ELSE
   row:=row||jsonb_build_object('temporaryRevision',(SELECT revision FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='temporary'));
  END IF;
  c:=NULL;m:=NULL;
  SELECT credential_id,method INTO c,m FROM user_host_profiles WHERE team_id=s.team_id AND account_id=s.account_id AND host_id=hop.id;
  IF m IS NULL OR m='inherit' THEN SELECT credential_id,method INTO c,m FROM user_connection_defaults WHERE team_id=s.team_id AND account_id=s.account_id; END IF;
  IF c IS NOT NULL AND (NOT credential_usable(s.team_id,s.account_id,c) OR EXISTS(SELECT FROM shared_credential_scopes WHERE team_id=s.team_id AND credential_id=c AND NOT within_scope(s.team_id,hop.id,scope_id)))
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  steps:=steps||jsonb_build_array(row);
 END LOOP;
 RETURN jsonb_build_object('targetId',r,'configurationRevision',coalesce((SELECT revision FROM connection_configurations WHERE team_id=s.team_id AND host_id=r),0),'steps',steps);
END $$;
CREATE OR REPLACE FUNCTION api_history_select(token text,r uuid,selected_revision bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF selected_revision IS NOT NULL THEN
  IF NOT allowed(s.team_id,s.account_id,r,'read') OR NOT allowed(s.team_id,s.account_id,r,'connect')
  THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
  IF NOT EXISTS(SELECT FROM hosts WHERE team_id=s.team_id AND id=r) OR NOT EXISTS(SELECT FROM resource_versions
  WHERE team_id=s.team_id AND resource_id=r AND revision=selected_revision)
  THEN RAISE EXCEPTION 'history_unavailable' USING ERRCODE='22023'; END IF;

 END IF;
 -- A user may release their own selection even after access is revoked.
 DELETE FROM revision_pins WHERE team_id=s.team_id AND resource_id=r AND account_id=s.account_id AND purpose='temporary';
 IF selected_revision IS NOT NULL THEN
  INSERT INTO revision_pins VALUES(s.team_id,r,selected_revision,s.account_id,'temporary');
  PERFORM api_connection_plan(token,r);
 END IF;
 -- Releasing/switching a selection must not retain old versions until the next edit.
 -- Other users' temporary selections and restore return points remain protected.
 DELETE FROM resource_versions rv USING resources x WHERE (x.team_id,x.id)=(rv.team_id,rv.resource_id)
 AND rv.team_id=s.team_id AND rv.resource_id=r AND rv.revision<x.revision-10
 AND NOT EXISTS(SELECT FROM revision_pins p WHERE (p.team_id,p.resource_id,p.revision)=(rv.team_id,rv.resource_id,rv.revision));
 RETURN jsonb_build_object('resourceId',r,'revision',selected_revision);
END $$;
INSERT INTO schema_migrations(version) VALUES(63);
COMMIT;
