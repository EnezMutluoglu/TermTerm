BEGIN;
SET search_path=termterm_team,public;
CREATE TABLE IF NOT EXISTS terminal_sessions(
 id uuid PRIMARY KEY, vault_id uuid NOT NULL REFERENCES team_vaults, record_id uuid NOT NULL REFERENCES team_records,
 owner_id uuid NOT NULL REFERENCES users, device_id uuid NOT NULL REFERENCES user_devices, writer_id uuid NOT NULL REFERENCES users,
 lease uuid NOT NULL, acl_revision bigint NOT NULL, envelopes jsonb NOT NULL, expires_at timestamptz NOT NULL);
CREATE TABLE IF NOT EXISTS terminal_frames(
 seq bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, id uuid UNIQUE NOT NULL, session_id uuid NOT NULL REFERENCES terminal_sessions ON DELETE CASCADE,
 lease uuid NOT NULL, author_id uuid NOT NULL REFERENCES users, kind text NOT NULL CHECK(kind IN ('input','output')),
 payload bytea NOT NULL CHECK(octet_length(payload)<=131072), expires_at timestamptz NOT NULL);
CREATE INDEX IF NOT EXISTS terminal_frames_cursor ON terminal_frames(session_id,seq);
ALTER TABLE terminal_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE terminal_frames ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS terminal_access ON terminal_sessions;
CREATE POLICY terminal_access ON terminal_sessions TO termterm_team_executor USING(allowed(actor(),vault_id,record_id,'connect'));
DROP POLICY IF EXISTS terminal_frame_access ON terminal_frames;
CREATE POLICY terminal_frame_access ON terminal_frames TO termterm_team_executor USING(EXISTS(SELECT FROM terminal_sessions s WHERE s.id=session_id AND allowed(actor(),s.vault_id,s.record_id,'connect')));
GRANT SELECT,INSERT,UPDATE,DELETE ON terminal_sessions,terminal_frames TO termterm_team_executor;
GRANT USAGE,SELECT ON SEQUENCE terminal_frames_seq_seq TO termterm_team_executor;

CREATE OR REPLACE FUNCTION terminal_rpc(action text,b jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE u uuid=actor(); d uuid=current_setting('termterm.device')::uuid; v uuid=(b->>'vaultId')::uuid;
 r uuid=nullif(b->>'recordId','')::uuid; sid uuid=nullif(b->>'id','')::uuid; t uuid; s terminal_sessions; userset jsonb; target uuid;
BEGIN
 SELECT team_id INTO t FROM team_vaults WHERE id=v;
 IF action='terminal_recipients' THEN
   PERFORM require_permission(v,r,'connect');
   RETURN jsonb_build_object('aclRevision',(SELECT acl_revision FROM teams WHERE id=t),'members',coalesce((SELECT jsonb_agg(jsonb_build_object('userId',us.id,'username',us.username,'publicKey',encode(us.public_key,'hex'))) FROM users us WHERE us.active AND allowed(us.id,v,r,'connect')),'[]'));
 ELSIF action='terminal_list' THEN
   RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',ts.id,'recordId',ts.record_id,'owner',ow.username,'writer',wr.username,'members',(SELECT jsonb_agg(jsonb_build_object('username',us.username)) FROM users us JOIN jsonb_array_elements(ts.envelopes)x ON us.id=(x->>'userId')::uuid))) FROM terminal_sessions ts JOIN users ow ON ow.id=ts.owner_id JOIN users wr ON wr.id=ts.writer_id WHERE ts.vault_id=v AND ts.expires_at>now() AND ts.acl_revision=(SELECT acl_revision FROM teams WHERE id=t) AND allowed(u,v,ts.record_id,'connect')),'[]');
 ELSIF action='terminal_open' THEN
   PERFORM 1 FROM teams WHERE id=t FOR UPDATE;
   PERFORM require_permission(v,r,'connect');
   IF NOT EXISTS(SELECT FROM team_records WHERE id=r AND vault_id=v AND kind='host' AND NOT deleted) THEN RAISE EXCEPTION 'Host unavailable'; END IF;
   IF (b->>'aclRevision')::bigint<>(SELECT acl_revision FROM teams WHERE id=t) THEN RAISE EXCEPTION 'TEAM_ACL_CHANGED' USING ERRCODE='40001'; END IF;
   IF EXISTS(SELECT id::text FROM users WHERE active AND allowed(id,v,r,'connect') EXCEPT SELECT x->>'userId' FROM jsonb_array_elements(b->'envelopes')x) OR EXISTS(SELECT x->>'userId' FROM jsonb_array_elements(b->'envelopes')x EXCEPT SELECT id::text FROM users WHERE active AND allowed(id,v,r,'connect')) THEN RAISE EXCEPTION 'Terminal recipients changed'; END IF;
   INSERT INTO terminal_sessions VALUES(sid,v,r,u,d,u,(b->>'lease')::uuid,(b->>'aclRevision')::bigint,b->'envelopes',now()+interval '15 seconds');
   PERFORM audit(t,r,'terminal.shared',jsonb_build_object('id',sid));
   RETURN jsonb_build_object('id',sid);
 END IF;
 SELECT * INTO s FROM terminal_sessions WHERE id=sid AND vault_id=v FOR UPDATE;
 IF s.id IS NULL OR s.expires_at<=now() OR s.acl_revision<>(SELECT acl_revision FROM teams WHERE id=t) THEN RAISE EXCEPTION 'Shared terminal ended or permissions changed' USING ERRCODE='42501'; END IF;
 PERFORM require_permission(v,s.record_id,'connect');
 IF NOT EXISTS(SELECT FROM team_records WHERE id=s.record_id AND NOT deleted) THEN RAISE EXCEPTION 'Host deleted' USING ERRCODE='42501'; END IF;
 IF action='terminal_control' THEN
   IF s.owner_id<>u OR s.device_id<>d THEN RAISE EXCEPTION 'Only the sharing device controls writers' USING ERRCODE='42501'; END IF;
   IF coalesce((b->>'finish')::boolean,false) THEN DELETE FROM terminal_sessions WHERE id=sid; RETURN '{}'; END IF;
   SELECT id INTO target FROM users WHERE username=b->>'writer' AND active;
   IF target IS NULL OR NOT allowed(target,v,s.record_id,'connect') OR NOT EXISTS(SELECT FROM jsonb_array_elements(s.envelopes)x WHERE x->>'userId'=target::text) THEN RAISE EXCEPTION 'Writer not permitted' USING ERRCODE='42501'; END IF;
   UPDATE terminal_sessions SET writer_id=target,lease=gen_random_uuid() WHERE id=sid;
   DELETE FROM terminal_frames WHERE session_id=sid AND kind='input'; RETURN '{}';
 ELSIF action='terminal_send' THEN
   IF s.lease<>(b->>'lease')::uuid THEN RAISE EXCEPTION 'Old input lease' USING ERRCODE='42501'; END IF;
   IF b->>'kind'='output' THEN
     IF s.owner_id<>u OR s.device_id<>d THEN RAISE EXCEPTION 'Output owner required' USING ERRCODE='42501'; END IF;
   ELSIF b->>'kind'='input' THEN
     IF s.writer_id<>u THEN RAISE EXCEPTION 'Read-only terminal' USING ERRCODE='42501'; END IF;
   ELSE RAISE EXCEPTION 'Invalid terminal frame'; END IF;
   INSERT INTO terminal_frames(id,session_id,lease,author_id,kind,payload,expires_at) VALUES((b->>'frameId')::uuid,sid,s.lease,u,b->>'kind',decode(b->>'payload','hex'),now()+interval '5 seconds') ON CONFLICT(id) DO NOTHING; RETURN '{}';
 ELSIF action IN ('terminal_join','terminal_poll') THEN
   IF s.owner_id=u AND s.device_id=d THEN UPDATE terminal_sessions SET expires_at=now()+interval '15 seconds' WHERE id=sid; END IF;
   DELETE FROM terminal_frames WHERE session_id=sid AND expires_at<=now();
   RETURN jsonb_build_object('id',sid,'lease',s.lease,'owner',(SELECT username FROM users WHERE id=s.owner_id),'writer',(SELECT username FROM users WHERE id=s.writer_id),'envelope',(SELECT x->>'envelope' FROM jsonb_array_elements(s.envelopes)x WHERE x->>'userId'=u::text),'cursor',(SELECT coalesce(max(seq),0) FROM terminal_frames WHERE session_id=sid),'frames',CASE WHEN action='terminal_join' THEN '[]'::jsonb ELSE coalesce((SELECT jsonb_agg(jsonb_build_object('seq',f.seq,'id',f.id,'lease',f.lease,'kind',f.kind,'payload',encode(f.payload,'hex')) ORDER BY f.seq) FROM terminal_frames f WHERE f.session_id=sid AND f.seq>coalesce((b->>'cursor')::bigint,0) AND f.expires_at>now() AND f.lease=s.lease AND ((s.owner_id=u AND s.device_id=d AND f.kind='input') OR (NOT(s.owner_id=u AND s.device_id=d) AND f.kind='output'))),'[]') END);
 END IF;
 RAISE EXCEPTION 'Unsupported terminal operation';
END $$;
REVOKE ALL ON FUNCTION terminal_rpc(text,jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION terminal_rpc(text,jsonb) TO termterm_team_executor;
INSERT INTO schema_version(version) VALUES(5) ON CONFLICT DO NOTHING;
COMMIT;
