-- Run explicitly as a dedicated migration owner, never as the HTTP service role.
BEGIN;
CREATE SCHEMA termterm_team_v2;
REVOKE ALL ON SCHEMA termterm_team_v2 FROM PUBLIC;
CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;
SET LOCAL search_path = termterm_team_v2, pg_catalog;
CREATE TABLE schema_migrations(version integer PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE teams (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), name text NOT NULL CHECK(length(name) BETWEEN 1 AND 120),
 acl_revision bigint NOT NULL DEFAULT 1, offline_hours integer NOT NULL DEFAULT 24 CHECK(offline_hours BETWEEN 0 AND 24),
 audit_days integer NOT NULL DEFAULT 90 CHECK(audit_days BETWEEN 1 AND 3650), created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE team_accounts (
 team_id uuid NOT NULL REFERENCES teams(id), id uuid NOT NULL DEFAULT gen_random_uuid(),
 username text NOT NULL CHECK(username ~ '^[a-zA-Z0-9_.@-]{1,120}$'),
 display_name text NOT NULL CHECK(length(display_name) BETWEEN 1 AND 160),
 password_hash text, status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','active','disabled')),
 auth_epoch bigint NOT NULL DEFAULT 1, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,id)
);
CREATE UNIQUE INDEX account_username_unique ON team_accounts(team_id,lower(username));
CREATE TABLE team_memberships (
 team_id uuid NOT NULL, account_id uuid NOT NULL,
 role text NOT NULL CHECK(role IN ('owner','manager','editor','operator','observer')),
 PRIMARY KEY(team_id,account_id), FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id)
);
CREATE UNIQUE INDEX one_team_owner ON team_memberships(team_id) WHERE role='owner';
CREATE TABLE account_devices (
 team_id uuid NOT NULL, account_id uuid NOT NULL, id uuid NOT NULL,
 name text NOT NULL CHECK(length(name) BETWEEN 1 AND 160), last_seen timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,account_id,id), FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id)
);
CREATE TABLE activation_tokens (
 team_id uuid NOT NULL, account_id uuid NOT NULL, generation bigint NOT NULL,
 password_hash text NOT NULL, expires_at timestamptz NOT NULL, consumed_at timestamptz,
 PRIMARY KEY(team_id,account_id,generation), FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id)
);
CREATE UNIQUE INDEX one_live_activation ON activation_tokens(team_id,account_id) WHERE consumed_at IS NULL;
CREATE TABLE auth_sessions (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), family_id uuid NOT NULL,
 team_id uuid NOT NULL, account_id uuid NOT NULL, device_id uuid NOT NULL, auth_epoch bigint NOT NULL,
 access_hash bytea NOT NULL UNIQUE CHECK(octet_length(access_hash)=32), refresh_hash bytea NOT NULL UNIQUE CHECK(octet_length(refresh_hash)=32),
 activation_only boolean NOT NULL DEFAULT false, access_expires timestamptz NOT NULL, refresh_expires timestamptz NOT NULL,
 rotated_at timestamptz, revoked_at timestamptz, created_at timestamptz NOT NULL DEFAULT now(),
 FOREIGN KEY(team_id,account_id,device_id) REFERENCES account_devices(team_id,account_id,id)
);
CREATE INDEX sessions_account_live ON auth_sessions(team_id,account_id) WHERE revoked_at IS NULL;
CREATE INDEX sessions_family ON auth_sessions(family_id);
CREATE INDEX sessions_expiry ON auth_sessions(refresh_expires);
CREATE TABLE auth_attempts (
 key_hash bytea PRIMARY KEY, window_at timestamptz NOT NULL, failures integer NOT NULL DEFAULT 0
);
CREATE TABLE auth_policy (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton), dummy_hash text NOT NULL
);
INSERT INTO auth_policy VALUES(true,public.crypt(encode(public.gen_random_bytes(32),'hex'),public.gen_salt('bf',12)));
CREATE TABLE account_crypto_identities (
 team_id uuid NOT NULL, account_id uuid NOT NULL, version bigint NOT NULL,
 public_key bytea NOT NULL CHECK(octet_length(public_key)=32),
 encrypted_private_key bytea NOT NULL CHECK(octet_length(encrypted_private_key) BETWEEN 64 AND 65536),
 active boolean NOT NULL DEFAULT true, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,account_id,version), FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id)
);
CREATE UNIQUE INDEX one_active_identity ON account_crypto_identities(team_id,account_id) WHERE active;
CREATE TABLE team_recovery_keys (
 team_id uuid NOT NULL REFERENCES teams(id), id uuid NOT NULL DEFAULT gen_random_uuid(),
 public_key bytea NOT NULL CHECK(octet_length(public_key)=32), active boolean NOT NULL DEFAULT true,
 created_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(team_id,id)
);
CREATE UNIQUE INDEX one_active_recovery_key ON team_recovery_keys(team_id) WHERE active;
CREATE TABLE vaults (
 team_id uuid NOT NULL REFERENCES teams(id), id uuid NOT NULL DEFAULT gen_random_uuid(),
 encrypted_name bytea NOT NULL, revision bigint NOT NULL DEFAULT 0, PRIMARY KEY(team_id,id)
);
CREATE TABLE resources (
 team_id uuid NOT NULL, vault_id uuid NOT NULL, id uuid NOT NULL,
 kind text NOT NULL CHECK(kind IN ('folder','host','credential','snippet','workspace','tunnel','knownHost')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0), deleted boolean NOT NULL DEFAULT false,
 modified_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(team_id,id),
 UNIQUE(team_id,vault_id,id), FOREIGN KEY(team_id,vault_id) REFERENCES vaults(team_id,id)
);
CREATE INDEX resources_changes ON resources(team_id,vault_id,modified_at,id);
CREATE TABLE folders (
 team_id uuid NOT NULL, vault_id uuid NOT NULL, id uuid NOT NULL, parent_id uuid, encrypted_name bytea NOT NULL,
 PRIMARY KEY(team_id,id), UNIQUE(team_id,vault_id,id),
 FOREIGN KEY(team_id,vault_id,id) REFERENCES resources(team_id,vault_id,id),
 FOREIGN KEY(team_id,vault_id,parent_id) REFERENCES folders(team_id,vault_id,id), CHECK(id IS DISTINCT FROM parent_id)
);
CREATE INDEX folder_children ON folders(team_id,vault_id,parent_id);
CREATE TABLE hosts (
 team_id uuid NOT NULL, vault_id uuid NOT NULL, id uuid NOT NULL, folder_id uuid,
 protocol text NOT NULL CHECK(protocol IN ('ssh','mosh','telnet','serial')),
 port integer NOT NULL CHECK(port BETWEEN 1 AND 65535),
 username_policy text NOT NULL CHECK(username_policy IN ('shared_required','personal')),
 encrypted_address bytea NOT NULL, encrypted_name bytea NOT NULL, encrypted_username bytea,
 PRIMARY KEY(team_id,id), UNIQUE(team_id,vault_id,id),
 FOREIGN KEY(team_id,vault_id,id) REFERENCES resources(team_id,vault_id,id),
 FOREIGN KEY(team_id,vault_id,folder_id) REFERENCES folders(team_id,vault_id,id),
 CHECK(username_policy <> 'shared_required' OR encrypted_username IS NOT NULL)
);
CREATE INDEX hosts_folder ON hosts(team_id,vault_id,folder_id);
CREATE TABLE host_hops (
 team_id uuid NOT NULL, host_id uuid NOT NULL, position smallint NOT NULL CHECK(position BETWEEN 0 AND 15), jump_host_id uuid NOT NULL,
 PRIMARY KEY(team_id,host_id,position), UNIQUE(team_id,host_id,jump_host_id), CHECK(host_id<>jump_host_id),
 FOREIGN KEY(team_id,host_id) REFERENCES hosts(team_id,id), FOREIGN KEY(team_id,jump_host_id) REFERENCES hosts(team_id,id)
);
CREATE INDEX hop_dependency ON host_hops(team_id,jump_host_id);
CREATE TABLE credentials (
 team_id uuid NOT NULL REFERENCES teams(id), id uuid NOT NULL DEFAULT gen_random_uuid(),
 visibility text NOT NULL CHECK(visibility IN ('private','shared')), owner_account_id uuid, resource_id uuid,
 kind text NOT NULL CHECK(kind IN ('ssh_key','password','agent','hardware')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0), deleted boolean NOT NULL DEFAULT false,
 PRIMARY KEY(team_id,id), UNIQUE(team_id,owner_account_id,id),
 FOREIGN KEY(team_id,owner_account_id) REFERENCES team_accounts(team_id,id), FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id),
 CHECK((visibility='private' AND owner_account_id IS NOT NULL AND resource_id IS NULL) OR
       (visibility='shared' AND owner_account_id IS NULL AND resource_id IS NOT NULL))
);
CREATE INDEX private_credentials ON credentials(team_id,owner_account_id,id) WHERE visibility='private' AND NOT deleted;
CREATE UNIQUE INDEX shared_credential_resource ON credentials(team_id,resource_id) WHERE visibility='shared';
CREATE TABLE credential_versions (
 team_id uuid NOT NULL, credential_id uuid NOT NULL, revision bigint NOT NULL,
 encrypted_data bytea NOT NULL CHECK(octet_length(encrypted_data) BETWEEN 40 AND 1048576),
 encrypted_secret bytea NOT NULL CHECK(octet_length(encrypted_secret) BETWEEN 40 AND 1048576),
 actor_id uuid NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,credential_id,revision), FOREIGN KEY(team_id,credential_id) REFERENCES credentials(team_id,id),
 FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id)
);
CREATE TABLE credential_key_envelopes (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), team_id uuid NOT NULL, credential_id uuid NOT NULL, revision bigint NOT NULL,
 account_id uuid, identity_version bigint, recovery_key_id uuid,
 purpose text NOT NULL CHECK(purpose IN ('data','secret')), envelope bytea NOT NULL CHECK(octet_length(envelope) BETWEEN 80 AND 4096),
 FOREIGN KEY(team_id,credential_id,revision) REFERENCES credential_versions(team_id,credential_id,revision) ON DELETE CASCADE,
 FOREIGN KEY(team_id,account_id,identity_version) REFERENCES account_crypto_identities(team_id,account_id,version),
 FOREIGN KEY(team_id,recovery_key_id) REFERENCES team_recovery_keys(team_id,id),
 CHECK((account_id IS NOT NULL AND identity_version IS NOT NULL AND recovery_key_id IS NULL) OR
       (account_id IS NULL AND identity_version IS NULL AND recovery_key_id IS NOT NULL))
);
CREATE UNIQUE INDEX credential_user_envelope ON credential_key_envelopes(team_id,credential_id,revision,account_id,identity_version,purpose) WHERE account_id IS NOT NULL;
CREATE UNIQUE INDEX credential_recovery_envelope ON credential_key_envelopes(team_id,credential_id,revision,recovery_key_id,purpose) WHERE recovery_key_id IS NOT NULL;
CREATE INDEX envelopes_account ON credential_key_envelopes(team_id,account_id,credential_id,revision);
CREATE TABLE user_connection_defaults (
 team_id uuid NOT NULL, account_id uuid NOT NULL, credential_id uuid,
 method text NOT NULL CHECK(method IN ('credential','prompt')), encrypted_username bytea, revision bigint NOT NULL DEFAULT 1,
 PRIMARY KEY(team_id,account_id), FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,credential_id) REFERENCES credentials(team_id,id),
 CHECK((method='credential')=(credential_id IS NOT NULL))
);
CREATE TABLE user_host_profiles (
 team_id uuid NOT NULL, account_id uuid NOT NULL, host_id uuid NOT NULL, credential_id uuid,
 method text NOT NULL CHECK(method IN ('credential','prompt')), encrypted_username bytea, revision bigint NOT NULL DEFAULT 1,
 PRIMARY KEY(team_id,account_id,host_id), FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,host_id) REFERENCES hosts(team_id,id), FOREIGN KEY(team_id,credential_id) REFERENCES credentials(team_id,id),
 CHECK((method='credential')=(credential_id IS NOT NULL))
);
CREATE INDEX profiles_host ON user_host_profiles(team_id,host_id);
CREATE TABLE resource_acl (
 team_id uuid NOT NULL, resource_id uuid NOT NULL, account_id uuid NOT NULL,
 permission text NOT NULL CHECK(permission IN ('read','connect','edit','reveal','export','manage')),
 effect text NOT NULL CHECK(effect IN ('allow','deny')),
 PRIMARY KEY(team_id,resource_id,account_id,permission), FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id),
 FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id)
);
CREATE INDEX acl_account ON resource_acl(team_id,account_id,permission,resource_id) INCLUDE(effect);
CREATE TABLE acl_delegations (
 team_id uuid NOT NULL, resource_id uuid NOT NULL, account_id uuid NOT NULL,
 permission text NOT NULL CHECK(permission IN ('read','connect','edit','reveal','export')),
 PRIMARY KEY(team_id,resource_id,account_id,permission), FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id),
 FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id)
);
-- Non-host document content remains separately encrypted, with typed link relations.
CREATE TABLE documents (
 team_id uuid NOT NULL, resource_id uuid NOT NULL, encrypted_content bytea NOT NULL,
 PRIMARY KEY(team_id,resource_id), FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id)
);
CREATE TABLE resource_links (
 team_id uuid NOT NULL, source_id uuid NOT NULL, target_id uuid NOT NULL, relation text NOT NULL CHECK(relation IN ('identity','chain','member')),
 PRIMARY KEY(team_id,source_id,target_id,relation),
 FOREIGN KEY(team_id,source_id) REFERENCES resources(team_id,id), FOREIGN KEY(team_id,target_id) REFERENCES resources(team_id,id)
);
CREATE INDEX resource_links_target ON resource_links(team_id,target_id);
CREATE TABLE resource_versions (
 team_id uuid NOT NULL, resource_id uuid NOT NULL, revision bigint NOT NULL, encrypted_snapshot bytea NOT NULL,
 actor_id uuid NOT NULL, restored_from bigint, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,resource_id,revision), FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id),
 FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id)
);
CREATE TABLE revision_pins (
 team_id uuid NOT NULL, resource_id uuid NOT NULL, revision bigint NOT NULL, account_id uuid NOT NULL,
 purpose text NOT NULL CHECK(purpose IN ('temporary','return_point')),
 PRIMARY KEY(team_id,resource_id,revision,account_id,purpose),
 FOREIGN KEY(team_id,resource_id,revision) REFERENCES resource_versions(team_id,resource_id,revision),
 FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id)
);
CREATE TABLE change_operations (
 team_id uuid NOT NULL, id uuid NOT NULL, account_id uuid NOT NULL, device_id uuid NOT NULL,
 sequence bigint NOT NULL CHECK(sequence>0), resource_id uuid, expected_revision bigint NOT NULL CHECK(expected_revision>=0),
 request_hash bytea NOT NULL CHECK(octet_length(request_hash)=32),
 status text NOT NULL CHECK(status IN ('pending','applied','conflict','rejected')), client_at timestamptz NOT NULL,
 received_at timestamptz NOT NULL DEFAULT now(), result_revision bigint,
 PRIMARY KEY(team_id,id), UNIQUE(team_id,account_id,device_id,sequence),
 FOREIGN KEY(team_id,account_id,device_id) REFERENCES account_devices(team_id,account_id,id),
 FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id)
);
CREATE INDEX pending_operations ON change_operations(team_id,account_id,received_at) WHERE status='pending';
CREATE TABLE conflicts (
 team_id uuid NOT NULL, id uuid NOT NULL DEFAULT gen_random_uuid(), operation_id uuid NOT NULL, resource_id uuid NOT NULL,
 encrypted_candidate bytea NOT NULL, reason text NOT NULL CHECK(reason IN ('revision','permission','dependency')),
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','resolved','rejected')),
 decided_by uuid, decided_at timestamptz, received_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,id), FOREIGN KEY(team_id,operation_id) REFERENCES change_operations(team_id,id),
 FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id), FOREIGN KEY(team_id,decided_by) REFERENCES team_accounts(team_id,id)
);
CREATE INDEX pending_conflicts ON conflicts(team_id,resource_id,received_at) WHERE status='pending';
CREATE TABLE recovery_operations (
 team_id uuid NOT NULL, id uuid NOT NULL DEFAULT gen_random_uuid(), actor_id uuid NOT NULL, subject_id uuid NOT NULL,
 recovery_key_id uuid NOT NULL, reason text NOT NULL CHECK(length(reason) BETWEEN 10 AND 1000),
 status text NOT NULL CHECK(status IN ('authorized','completed','failed')), expires_at timestamptz NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(), completed_at timestamptz,
 PRIMARY KEY(team_id,id), FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,subject_id) REFERENCES team_accounts(team_id,id), FOREIGN KEY(team_id,recovery_key_id) REFERENCES team_recovery_keys(team_id,id)
);
CREATE TABLE audit_events (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, team_id uuid NOT NULL REFERENCES teams(id), actor_id uuid,
 action text NOT NULL CHECK(length(action) BETWEEN 1 AND 80), subject_id uuid, resource_id uuid,
 outcome text NOT NULL CHECK(outcome IN ('succeeded','denied','failed','authorized')),
 server_at timestamptz NOT NULL DEFAULT now(), FOREIGN KEY(team_id,actor_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,subject_id) REFERENCES team_accounts(team_id,id), FOREIGN KEY(team_id,resource_id) REFERENCES resources(team_id,id)
);
CREATE INDEX audit_team_time ON audit_events(team_id,server_at DESC,id DESC);
CREATE INDEX audit_resource_time ON audit_events(team_id,resource_id,server_at DESC) WHERE resource_id IS NOT NULL;
CREATE INDEX recovery_subject_time ON recovery_operations(team_id,subject_id,created_at DESC);

-- No application-level actor variable can bypass RLS. Only narrow SECURITY DEFINER
-- functions owned by the migration owner may read these default-deny tables.
DO $$ DECLARE t record; BEGIN
 FOR t IN SELECT tablename FROM pg_tables WHERE schemaname='termterm_team_v2' LOOP
  EXECUTE format('ALTER TABLE termterm_team_v2.%I ENABLE ROW LEVEL SECURITY',t.tablename);
  EXECUTE format('REVOKE ALL ON termterm_team_v2.%I FROM PUBLIC',t.tablename);
 END LOOP;
END $$;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA termterm_team_v2 FROM PUBLIC;
ALTER DEFAULT PRIVILEGES IN SCHEMA termterm_team_v2 REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(1);
COMMIT;
