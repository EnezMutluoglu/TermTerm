BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- A password reset can leave a legacy username encrypted to the old identity.
-- Re-entering a username must preserve the old ciphertext rather than destroy it.
CREATE TABLE profile_username_archives (
 team_id uuid NOT NULL,account_id uuid NOT NULL,host_key text NOT NULL,revision bigint NOT NULL,
 method text NOT NULL,credential_id uuid,encrypted_username bytea NOT NULL,created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,account_id,host_key,revision),
 FOREIGN KEY(team_id,account_id) REFERENCES team_accounts(team_id,id),
 FOREIGN KEY(team_id,credential_id) REFERENCES credentials(team_id,id)
);
ALTER TABLE profile_username_archives ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON profile_username_archives FROM PUBLIC;
CREATE FUNCTION archive_profile_username() RETURNS trigger
LANGUAGE plpgsql SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE h text;
BEGIN
 IF TG_TABLE_NAME='user_host_profiles' THEN h:=OLD.host_id::text;ELSE h:='default';END IF;
 IF OLD.encrypted_username IS NOT NULL AND OLD.encrypted_username IS DISTINCT FROM NEW.encrypted_username THEN
  INSERT INTO profile_username_archives(team_id,account_id,host_key,revision,method,credential_id,encrypted_username)
  VALUES(OLD.team_id,OLD.account_id,h,OLD.revision,OLD.method,OLD.credential_id,OLD.encrypted_username);
  DELETE FROM profile_username_archives WHERE team_id=OLD.team_id AND account_id=OLD.account_id AND host_key=h
  AND revision NOT IN(SELECT revision FROM profile_username_archives WHERE team_id=OLD.team_id AND account_id=OLD.account_id AND host_key=h ORDER BY revision DESC LIMIT 10);
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER archive_default_username BEFORE UPDATE ON user_connection_defaults FOR EACH ROW EXECUTE FUNCTION archive_profile_username();
CREATE TRIGGER archive_host_username BEFORE UPDATE ON user_host_profiles FOR EACH ROW EXECUTE FUNCTION archive_profile_username();
REVOKE ALL ON FUNCTION archive_profile_username() FROM PUBLIC;
INSERT INTO schema_migrations(version) VALUES(27);
COMMIT;
