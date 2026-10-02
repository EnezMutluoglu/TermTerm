-- TermTerm Team schema 1. Run as the separate migration administrator, never the app login.
-- The app login is granted only the five entry points listed in grant_app.sql.
BEGIN;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='termterm_team_executor') THEN
    CREATE ROLE termterm_team_executor NOLOGIN NOSUPERUSER NOBYPASSRLS;
  END IF;
END $$;
CREATE SCHEMA IF NOT EXISTS termterm_team;
REVOKE ALL ON SCHEMA termterm_team FROM PUBLIC;
GRANT USAGE ON SCHEMA termterm_team, public TO termterm_team_executor;
SET search_path = termterm_team, public;
CREATE TABLE schema_version(version integer PRIMARY KEY, installed_at timestamptz NOT NULL DEFAULT now());
INSERT INTO schema_version VALUES(1,now());
CREATE TABLE users(
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), username text NOT NULL, email text NOT NULL,
 password_hash text NOT NULL, active boolean NOT NULL DEFAULT true,
 public_key bytea NOT NULL CHECK(octet_length(public_key)=32), private_identity bytea NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now());
CREATE UNIQUE INDEX users_username ON users(lower(username));
CREATE UNIQUE INDEX users_email ON users(lower(email));
CREATE TABLE user_devices(id uuid PRIMARY KEY, user_id uuid NOT NULL REFERENCES users, name text NOT NULL, created_at timestamptz NOT NULL DEFAULT now(), UNIQUE(id,user_id));
CREATE TABLE auth_sessions(
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), user_id uuid NOT NULL REFERENCES users, device_id uuid NOT NULL,
 access_hash bytea UNIQUE NOT NULL, refresh_hash bytea UNIQUE NOT NULL, delivery_hash bytea UNIQUE NOT NULL,
 access_expires timestamptz NOT NULL DEFAULT now()+interval '1 hour', refresh_expires timestamptz NOT NULL DEFAULT now()+interval '30 days',
 revoked_at timestamptz, FOREIGN KEY(device_id,user_id) REFERENCES user_devices(id,user_id));
CREATE TABLE auth_attempts(login_hash bytea PRIMARY KEY, attempts integer NOT NULL, window_start timestamptz NOT NULL);
CREATE TABLE teams(id uuid PRIMARY KEY DEFAULT gen_random_uuid(), name text NOT NULL CHECK(length(name) BETWEEN 1 AND 120), owner_id uuid NOT NULL REFERENCES users, acl_revision bigint NOT NULL DEFAULT 1, offline_hours integer NOT NULL DEFAULT 24 CHECK(offline_hours=24), audit_days integer NOT NULL DEFAULT 90 CHECK(audit_days BETWEEN 1 AND 3650), created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE team_members(team_id uuid REFERENCES teams, user_id uuid REFERENCES users, role text NOT NULL CHECK(role IN ('owner','manager','editor','operator','observer')), active boolean NOT NULL DEFAULT true, PRIMARY KEY(team_id,user_id));
CREATE TABLE team_vaults(id uuid PRIMARY KEY DEFAULT gen_random_uuid(), team_id uuid NOT NULL REFERENCES teams, name text NOT NULL, revision bigint NOT NULL DEFAULT 0, UNIQUE(id,team_id));
CREATE TABLE team_records(
 id uuid PRIMARY KEY, vault_id uuid NOT NULL REFERENCES team_vaults, kind text NOT NULL CHECK(kind IN ('host','group','credential','snippet','workspace','tunnel','knownHost')),
 parent_id uuid REFERENCES team_records DEFERRABLE INITIALLY DEFERRED,
 revision bigint NOT NULL, payload bytea NOT NULL, secrets bytea NOT NULL, deleted boolean NOT NULL DEFAULT false,
 updated_by uuid NOT NULL REFERENCES users, updated_at timestamptz NOT NULL DEFAULT now(), UNIQUE(id,vault_id));
CREATE INDEX team_records_parent ON team_records(vault_id,parent_id);
CREATE TABLE record_links(record_id uuid REFERENCES team_records, target_id uuid REFERENCES team_records, relation text NOT NULL CHECK(relation IN ('identity','chain','member')), PRIMARY KEY(record_id,target_id,relation));
CREATE TABLE acl_entries(
 team_id uuid NOT NULL REFERENCES teams, user_id uuid NOT NULL REFERENCES users, vault_id uuid NOT NULL,
 resource_id uuid, permission text NOT NULL CHECK(permission IN ('read','connect','edit','reveal','export','manage')),
 effect text NOT NULL CHECK(effect IN ('allow','deny')), granted_by uuid NOT NULL REFERENCES users,
 FOREIGN KEY(vault_id,team_id) REFERENCES team_vaults(id,team_id), FOREIGN KEY(resource_id,vault_id) REFERENCES team_records(id,vault_id),
 UNIQUE NULLS NOT DISTINCT(user_id,vault_id,resource_id,permission));
CREATE TABLE acl_delegations(team_id uuid NOT NULL REFERENCES teams, user_id uuid NOT NULL REFERENCES users, vault_id uuid NOT NULL, resource_id uuid, permissions text[] NOT NULL, FOREIGN KEY(vault_id,team_id) REFERENCES team_vaults(id,team_id), FOREIGN KEY(resource_id,vault_id) REFERENCES team_records(id,vault_id), UNIQUE NULLS NOT DISTINCT(user_id,vault_id,resource_id));
CREATE TABLE record_versions(
 record_id uuid REFERENCES team_records, revision bigint NOT NULL, parent_revision bigint NOT NULL,
 payload bytea NOT NULL, secrets bytea NOT NULL, deleted boolean NOT NULL, parent_id uuid, kind text NOT NULL,
 actor_id uuid NOT NULL REFERENCES users, device_id uuid NOT NULL REFERENCES user_devices,
 client_at timestamptz NOT NULL, server_at timestamptz NOT NULL DEFAULT now(), restore_from bigint,
 PRIMARY KEY(record_id,revision));
CREATE TABLE record_key_envelopes(record_id uuid NOT NULL, revision bigint NOT NULL, user_id uuid NOT NULL REFERENCES users, purpose text NOT NULL CHECK(purpose IN ('data','secret')), envelope bytea NOT NULL, PRIMARY KEY(record_id,revision,user_id,purpose), FOREIGN KEY(record_id,revision) REFERENCES record_versions ON DELETE CASCADE);
CREATE TABLE change_operations(id uuid PRIMARY KEY, user_id uuid NOT NULL REFERENCES users, device_id uuid NOT NULL REFERENCES user_devices, sequence bigint NOT NULL CHECK(sequence>0), record_id uuid NOT NULL, vault_id uuid NOT NULL REFERENCES team_vaults, expected_revision bigint NOT NULL, client_at timestamptz NOT NULL, received_at timestamptz NOT NULL DEFAULT now(), result jsonb NOT NULL, UNIQUE(device_id,sequence));
CREATE TABLE conflicts(id uuid PRIMARY KEY DEFAULT gen_random_uuid(), operation_id uuid UNIQUE NOT NULL REFERENCES change_operations, record_id uuid NOT NULL, vault_id uuid NOT NULL REFERENCES team_vaults, author_id uuid NOT NULL REFERENCES users, candidate jsonb NOT NULL, status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','resolved','discarded')), reason text NOT NULL, decided_by uuid REFERENCES users, decided_at timestamptz, decision_revision bigint);
CREATE INDEX conflicts_pending ON conflicts(vault_id) WHERE status='pending';
CREATE TABLE audit_events(id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, team_id uuid NOT NULL REFERENCES teams, actor_id uuid NOT NULL REFERENCES users, device_id uuid REFERENCES user_devices, record_id uuid, action text NOT NULL, summary jsonb NOT NULL DEFAULT '{}', client_at timestamptz, server_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX audit_team_time ON audit_events(team_id,server_at DESC);
CREATE TABLE revision_pins(record_id uuid NOT NULL, revision bigint NOT NULL, user_id uuid NOT NULL REFERENCES users, purpose text NOT NULL CHECK(purpose IN ('temporary','return')), PRIMARY KEY(record_id,user_id,purpose), FOREIGN KEY(record_id,revision) REFERENCES record_versions);

-- Helpers never granted to the application login. A SECURITY DEFINER entry point
-- sets this identity ONLY after checking an unpredictable hashed access token.
CREATE FUNCTION actor() RETURNS uuid LANGUAGE sql STABLE AS $$ SELECT nullif(current_setting('termterm.actor',true),'')::uuid $$;
CREATE FUNCTION is_member(t uuid, u uuid) RETURNS boolean LANGUAGE sql STABLE AS $$ SELECT EXISTS(SELECT 1 FROM team_members WHERE team_id=t AND user_id=u AND active) $$;
CREATE FUNCTION is_owner(t uuid,u uuid) RETURNS boolean LANGUAGE sql STABLE AS $$ SELECT EXISTS(SELECT 1 FROM teams WHERE id=t AND owner_id=u) $$;
CREATE FUNCTION ancestors(v uuid,r uuid) RETURNS SETOF uuid LANGUAGE sql STABLE AS $$
 WITH RECURSIVE path(id,parent_id,seen) AS (
 SELECT id,parent_id,ARRAY[id] FROM team_records WHERE id=r AND vault_id=v
 UNION ALL SELECT t.id,t.parent_id,p.seen||t.id FROM team_records t JOIN path p ON t.id=p.parent_id WHERE t.vault_id=v AND NOT t.id=ANY(p.seen)) SELECT id FROM path
$$;
CREATE FUNCTION allowed(u uuid,v uuid,r uuid,p text) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT FROM team_vaults vault WHERE vault.id=v AND is_member(vault.team_id,u) AND
 (is_owner(vault.team_id,u) OR
 (NOT EXISTS(SELECT FROM acl_entries a WHERE a.user_id=u AND a.vault_id=v AND a.permission=p AND a.effect='deny' AND (a.resource_id IS NULL OR a.resource_id IN (SELECT ancestors(v,r)))) AND
 EXISTS(SELECT FROM acl_entries a WHERE a.user_id=u AND a.vault_id=v AND a.permission=p AND a.effect='allow' AND (a.resource_id IS NULL OR a.resource_id IN (SELECT ancestors(v,r)))))))
$$;
CREATE FUNCTION require_permission(v uuid,r uuid,p text) RETURNS void LANGUAGE plpgsql AS $$ BEGIN
 IF NOT allowed(actor(),v,r,p) THEN RAISE EXCEPTION 'TEAM_FORBIDDEN: %',p USING ERRCODE='42501'; END IF;
END $$;
CREATE FUNCTION audit(t uuid, r uuid, a text, s jsonb DEFAULT '{}') RETURNS void LANGUAGE sql AS $$
 INSERT INTO audit_events(team_id,actor_id,device_id,record_id,action,summary) VALUES(t,actor(),nullif(current_setting('termterm.device',true),'')::uuid,r,a,s)
$$;
CREATE FUNCTION password_check(p text) RETURNS void LANGUAGE plpgsql AS $$ BEGIN
 IF length(p)<12 OR octet_length(p)>72 THEN RAISE EXCEPTION 'Password must contain at least 12 characters and at most 72 UTF-8 bytes'; END IF;
END $$;
CREATE FUNCTION issue_session(u uuid,d uuid,n text) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE a text=encode(gen_random_bytes(32),'hex'); r text=encode(gen_random_bytes(32),'hex'); q text=encode(gen_random_bytes(32),'hex'); s uuid;
BEGIN
 INSERT INTO user_devices VALUES(d,u,left(n,120),now()) ON CONFLICT(id) DO NOTHING;
 IF NOT EXISTS(SELECT FROM user_devices WHERE id=d AND user_id=u) THEN RAISE EXCEPTION 'Device belongs to another account'; END IF;
 INSERT INTO auth_sessions(user_id,device_id,access_hash,refresh_hash,delivery_hash) VALUES(u,d,digest(a,'sha256'),digest(r,'sha256'),digest(q,'sha256')) RETURNING id INTO s;
 RETURN (SELECT jsonb_build_object('userId',id,'username',username,'email',email,'publicKey',encode(public_key,'hex'),'privateIdentity',encode(private_identity,'hex'),'accessToken',a,'refreshToken',r,'deliveryToken',q,'sessionId',s,'expiresIn',3600) FROM users WHERE id=u);
END $$;
CREATE FUNCTION register_account(username text,email text,password text,public_key bytea,private_identity bytea,device uuid,device_name text) RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team,public,pg_temp AS $$
DECLARE u uuid;
BEGIN
 PERFORM password_check(password);
 IF username !~ '^[A-Za-z0-9_.-]{3,64}$' OR length(email)>254 OR email !~ '^[^@[:space:]]+@[^@[:space:]]+\.[^@[:space:]]+$' OR octet_length(private_identity)>4096 THEN RAISE EXCEPTION 'Invalid account fields'; END IF;
 INSERT INTO users(username,email,password_hash,public_key,private_identity) VALUES(username,email,crypt(password,gen_salt('bf',12)),public_key,private_identity) RETURNING id INTO u;
 RETURN issue_session(u,device,device_name);
END $$;
CREATE FUNCTION login_account(login text,password text,device uuid,device_name text) RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team,public,pg_temp AS $$
DECLARE u users; h bytea=digest(lower(login),'sha256'); attempts integer;
BEGIN
 INSERT INTO auth_attempts VALUES(h,1,now()) ON CONFLICT(login_hash) DO UPDATE SET attempts=CASE WHEN auth_attempts.window_start<now()-interval '15 minutes' THEN 1 ELSE auth_attempts.attempts+1 END, window_start=CASE WHEN auth_attempts.window_start<now()-interval '15 minutes' THEN now() ELSE auth_attempts.window_start END RETURNING auth_attempts.attempts INTO attempts;
 IF attempts>20 OR octet_length(password)>72 THEN RETURN jsonb_build_object('error','Invalid credentials or retry later'); END IF;
 SELECT * INTO u FROM users WHERE (lower(username)=lower(login) OR lower(email)=lower(login)) AND active;
 IF u.id IS NULL OR crypt(password,u.password_hash)<>u.password_hash THEN RETURN jsonb_build_object('error','Invalid credentials or retry later'); END IF;
 DELETE FROM auth_attempts WHERE login_hash=h;
 RETURN issue_session(u.id,device,device_name);
END $$;
CREATE FUNCTION refresh_session(token text) RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team,public,pg_temp AS $$
DECLARE s auth_sessions; a text=encode(gen_random_bytes(32),'hex'); r text=encode(gen_random_bytes(32),'hex');
BEGIN
 SELECT * INTO s FROM auth_sessions WHERE refresh_hash=digest(token,'sha256') AND revoked_at IS NULL AND refresh_expires>now() FOR UPDATE;
 IF s.id IS NULL OR NOT EXISTS(SELECT FROM users WHERE id=s.user_id AND active) THEN RAISE EXCEPTION 'TEAM_AUTH_EXPIRED' USING ERRCODE='28000'; END IF;
 UPDATE auth_sessions SET access_hash=digest(a,'sha256'), refresh_hash=digest(r,'sha256'),access_expires=now()+interval '1 hour' WHERE id=s.id;
 RETURN jsonb_build_object('accessToken',a,'refreshToken',r,'expiresIn',3600);
END $$;

-- Application role has no table privileges. RLS is additional protection for the
-- non-login executor: private control tables are accessed only by checked RPCs;
-- audit and user-specific envelopes additionally bind to the verified identity.
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['users','user_devices','auth_sessions','auth_attempts','teams','team_members','team_vaults','team_records','record_links','acl_entries','acl_delegations','record_versions','record_key_envelopes','change_operations','conflicts','audit_events','revision_pins','schema_version'] LOOP
 EXECUTE format('ALTER TABLE termterm_team.%I ENABLE ROW LEVEL SECURITY',t);
 EXECUTE format('ALTER TABLE termterm_team.%I FORCE ROW LEVEL SECURITY',t);
 EXECUTE format('CREATE POLICY executor_only ON termterm_team.%I TO termterm_team_executor USING (true) WITH CHECK (true)',t);
 END LOOP;
END $$;
GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA termterm_team TO termterm_team_executor;
GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA termterm_team TO termterm_team_executor;
REVOKE ALL ON ALL TABLES IN SCHEMA termterm_team FROM PUBLIC;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team FROM PUBLIC;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA termterm_team TO termterm_team_executor;
ALTER FUNCTION register_account(text,text,text,bytea,bytea,uuid,text) OWNER TO termterm_team_executor;
ALTER FUNCTION login_account(text,text,uuid,text) OWNER TO termterm_team_executor;
ALTER FUNCTION refresh_session(text) OWNER TO termterm_team_executor;
COMMIT;
