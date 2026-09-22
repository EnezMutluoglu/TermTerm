-- Run as a dedicated schema owner/migration role, never as an everyday app user.
BEGIN;
CREATE SCHEMA IF NOT EXISTS termterm;
CREATE TABLE IF NOT EXISTS termterm.schema_version (version integer PRIMARY KEY);
INSERT INTO termterm.schema_version VALUES(1) ON CONFLICT DO NOTHING;
CREATE TABLE IF NOT EXISTS termterm.vaults (
  id uuid PRIMARY KEY, envelope bytea NOT NULL, encrypted_name bytea NOT NULL,
  revision bigint NOT NULL DEFAULT 0, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS termterm.members (
  vault_id uuid REFERENCES termterm.vaults ON DELETE CASCADE,
  principal name NOT NULL, role text NOT NULL CHECK(role IN ('owner','editor','viewer')),
  key_envelope bytea, PRIMARY KEY(vault_id,principal)
);
CREATE TABLE IF NOT EXISTS termterm.records (
  vault_id uuid REFERENCES termterm.vaults ON DELETE CASCADE, id uuid NOT NULL,
  revision bigint NOT NULL, payload bytea NOT NULL, deleted boolean NOT NULL DEFAULT false,
  PRIMARY KEY(vault_id,id)
);
CREATE INDEX IF NOT EXISTS records_revision ON termterm.records(vault_id,revision);
CREATE TABLE IF NOT EXISTS termterm.operations (
  vault_id uuid REFERENCES termterm.vaults ON DELETE CASCADE, op_id uuid NOT NULL,
  record_id uuid NOT NULL, revision bigint NOT NULL, principal name NOT NULL,
  PRIMARY KEY(vault_id,op_id)
);
CREATE TABLE IF NOT EXISTS termterm.terminal_sessions (
  id uuid PRIMARY KEY, vault_id uuid REFERENCES termterm.vaults ON DELETE CASCADE,
  owner name NOT NULL, writer name, lease uuid NOT NULL, expires_at timestamptz NOT NULL
);
CREATE TABLE IF NOT EXISTS termterm.terminal_frames (
  session_id uuid REFERENCES termterm.terminal_sessions ON DELETE CASCADE,
  seq bigint GENERATED ALWAYS AS IDENTITY, sender name NOT NULL DEFAULT session_user,
  kind text NOT NULL CHECK(kind IN ('output','input','close')),
  lease uuid NOT NULL, payload bytea NOT NULL, expires_at timestamptz NOT NULL DEFAULT now()+interval '30 seconds',
  PRIMARY KEY(session_id,seq)
);
CREATE OR REPLACE FUNCTION termterm.member_role(v uuid) RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER SET search_path=pg_catalog,termterm AS $$
  SELECT role FROM termterm.members WHERE vault_id=v AND principal=session_user
$$;
CREATE OR REPLACE FUNCTION termterm.create_vault(v uuid,e bytea,n bytea) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,termterm AS $$
BEGIN
  INSERT INTO termterm.vaults(id,envelope,encrypted_name) VALUES(v,e,n);
  INSERT INTO termterm.members(vault_id,principal,role,key_envelope) VALUES(v,session_user,'owner',e);
END $$;
CREATE OR REPLACE FUNCTION termterm.set_member(v uuid,p name,r text,e bytea DEFAULT NULL) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,termterm AS $$
BEGIN
  IF termterm.member_role(v) IS DISTINCT FROM 'owner' THEN RAISE EXCEPTION 'Owner permission required'; END IF;
  IF p=session_user THEN RAISE EXCEPTION 'Cannot change own owner membership'; END IF;
  IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname=p AND rolcanlogin) THEN RAISE EXCEPTION 'A PostgreSQL login for this member is required'; END IF;
  IF r='remove' THEN DELETE FROM termterm.members WHERE vault_id=v AND principal=p;
  ELSE INSERT INTO termterm.members(vault_id,principal,role,key_envelope) VALUES(v,p,r,e)
    ON CONFLICT(vault_id,principal) DO UPDATE SET role=excluded.role,key_envelope=excluded.key_envelope; END IF;
END $$;
CREATE OR REPLACE FUNCTION termterm.apply_operation(v uuid,op uuid,rid uuid,base bigint,body bytea,tombstone boolean)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,termterm AS $$
DECLARE previous bigint; actual bigint; nextrev bigint;
BEGIN
  IF COALESCE(termterm.member_role(v),'') NOT IN ('owner','editor') THEN RAISE EXCEPTION 'Write permission denied'; END IF;
  PERFORM 1 FROM termterm.vaults WHERE id=v FOR UPDATE;
  SELECT revision INTO previous FROM termterm.operations WHERE vault_id=v AND op_id=op AND record_id=rid AND principal=session_user;
  IF FOUND THEN RETURN previous; END IF;
  SELECT revision INTO actual FROM termterm.records WHERE vault_id=v AND id=rid;
  IF COALESCE(actual,0)<>base THEN RAISE EXCEPTION 'TERMTTERM_CONFLICT:%',COALESCE(actual,0); END IF;
  UPDATE termterm.vaults SET revision=revision+1 WHERE id=v RETURNING revision INTO nextrev;
  INSERT INTO termterm.records(vault_id,id,revision,payload,deleted) VALUES(v,rid,nextrev,body,tombstone)
    ON CONFLICT(vault_id,id) DO UPDATE SET revision=excluded.revision,payload=excluded.payload,deleted=excluded.deleted;
  INSERT INTO termterm.operations VALUES(v,op,rid,nextrev,session_user);
  PERFORM pg_notify('termterm_changes',v::text);
  RETURN nextrev;
END $$;
ALTER TABLE termterm.vaults ENABLE ROW LEVEL SECURITY;
ALTER TABLE termterm.members ENABLE ROW LEVEL SECURITY;
ALTER TABLE termterm.records ENABLE ROW LEVEL SECURITY;
ALTER TABLE termterm.operations ENABLE ROW LEVEL SECURITY;
ALTER TABLE termterm.terminal_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE termterm.terminal_frames ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS vault_read ON termterm.vaults;
CREATE POLICY vault_read ON termterm.vaults FOR SELECT USING(termterm.member_role(id) IS NOT NULL);
DROP POLICY IF EXISTS member_read ON termterm.members;
CREATE POLICY member_read ON termterm.members FOR SELECT USING(termterm.member_role(vault_id) IS NOT NULL);
DROP POLICY IF EXISTS record_read ON termterm.records;
CREATE POLICY record_read ON termterm.records FOR SELECT USING(termterm.member_role(vault_id) IS NOT NULL);
DROP POLICY IF EXISTS session_read ON termterm.terminal_sessions;
CREATE POLICY session_read ON termterm.terminal_sessions FOR SELECT USING(termterm.member_role(vault_id) IS NOT NULL AND expires_at>now());
DROP POLICY IF EXISTS session_create ON termterm.terminal_sessions;
CREATE POLICY session_create ON termterm.terminal_sessions FOR INSERT WITH CHECK(owner=session_user AND writer=session_user AND termterm.member_role(vault_id) IN ('owner','editor'));
DROP POLICY IF EXISTS session_control ON termterm.terminal_sessions;
CREATE POLICY session_control ON termterm.terminal_sessions FOR UPDATE USING(owner=session_user) WITH CHECK(owner=session_user AND termterm.member_role(vault_id) IN ('owner','editor'));
DROP POLICY IF EXISTS frame_read ON termterm.terminal_frames;
CREATE POLICY frame_read ON termterm.terminal_frames FOR SELECT USING(expires_at>now() AND EXISTS(SELECT FROM termterm.terminal_sessions s WHERE s.id=session_id));
DROP POLICY IF EXISTS frame_write ON termterm.terminal_frames;
CREATE POLICY frame_write ON termterm.terminal_frames FOR INSERT WITH CHECK(sender=session_user AND expires_at<=now()+interval '30 seconds' AND EXISTS(SELECT FROM termterm.terminal_sessions s WHERE s.id=session_id AND s.lease=terminal_frames.lease AND ((kind='input' AND s.writer=session_user) OR (kind IN ('output','close') AND s.owner=session_user))));
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm FROM PUBLIC;
REVOKE ALL ON ALL TABLES IN SCHEMA termterm FROM PUBLIC;
COMMIT;
