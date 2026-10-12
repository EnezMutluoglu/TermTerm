BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
-- A historical create permission can authorize candidate delivery only. Normal
-- changes always go through current ACL and current-recipient validation.
CREATE TABLE offline_creation_scopes (
 team_id uuid NOT NULL,id uuid NOT NULL DEFAULT gen_random_uuid(),account_id uuid NOT NULL,device_id uuid NOT NULL,
 vault_id uuid NOT NULL,parent_id uuid,acl_revision bigint NOT NULL,issued_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(team_id,id),FOREIGN KEY(team_id,account_id,device_id) REFERENCES account_devices(team_id,account_id,id),
 FOREIGN KEY(team_id,vault_id) REFERENCES vaults(team_id,id),
 FOREIGN KEY(team_id,vault_id,parent_id) REFERENCES folders(team_id,vault_id,id)
);
CREATE INDEX creation_scopes_account ON offline_creation_scopes(team_id,account_id,device_id,vault_id,parent_id,issued_at DESC);
CREATE TABLE offline_creation_claims (
 team_id uuid NOT NULL,scope_id uuid NOT NULL,resource_id uuid NOT NULL,receipt_id uuid NOT NULL,
 PRIMARY KEY(team_id,scope_id,resource_id),FOREIGN KEY(team_id,scope_id) REFERENCES offline_creation_scopes(team_id,id),
 FOREIGN KEY(team_id,receipt_id) REFERENCES resource_edit_receipts(team_id,id)
);
CREATE INDEX creation_claims_resource ON offline_creation_claims(team_id,resource_id);
ALTER TABLE offline_creation_scopes ENABLE ROW LEVEL SECURITY;
ALTER TABLE offline_creation_claims ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON offline_creation_scopes,offline_creation_claims FROM PUBLIC;
CREATE FUNCTION api_creation_scope(token text,v uuid,parent uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;cap uuid;rev bigint;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 IF NOT EXISTS(SELECT FROM vaults WHERE team_id=s.team_id AND id=v) THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 IF parent IS NULL THEN PERFORM require_owner(s);
 ELSIF NOT EXISTS(SELECT FROM folders f JOIN resources r ON (r.team_id,r.id)=(f.team_id,f.id)
 WHERE f.team_id=s.team_id AND f.vault_id=v AND f.id=parent AND NOT r.deleted)
 OR NOT allowed(s.team_id,s.account_id,parent,'edit') THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT acl_revision INTO rev FROM teams WHERE id=s.team_id;
 INSERT INTO offline_creation_scopes(team_id,account_id,device_id,vault_id,parent_id,acl_revision)
 VALUES(s.team_id,s.account_id,s.device_id,v,parent,rev) RETURNING id INTO cap;
 RETURN jsonb_build_object('capability',cap,'vaultId',v,'parentId',parent,'editWindowSeconds',86400);
END $$;
CREATE FUNCTION api_creation_claim(token text,scope uuid,r uuid,v uuid,parent uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;cap offline_creation_scopes;receipt uuid;
BEGIN
 s:=session_actor(token);PERFORM pg_advisory_xact_lock(hashtextextended(s.team_id::text,419));
 SELECT * INTO cap FROM offline_creation_scopes WHERE team_id=s.team_id AND id=scope
 AND account_id=s.account_id AND device_id=s.device_id AND vault_id=v AND parent_id IS NOT DISTINCT FROM parent;
 IF cap.id IS NULL OR r IS NULL THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 SELECT receipt_id INTO receipt FROM offline_creation_claims WHERE team_id=s.team_id AND scope_id=scope AND resource_id=r;
 IF receipt IS NOT NULL THEN RETURN jsonb_build_object('writeCapability',receipt); END IF;
 -- A historical scope cannot be used to impersonate an edit of an existing
 -- record, nor can a different scope claim an already reserved identifier.
 IF EXISTS(SELECT FROM resources WHERE team_id=s.team_id AND id=r)
 OR EXISTS(SELECT FROM offline_creation_claims WHERE team_id=s.team_id AND resource_id=r)
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
 INSERT INTO resource_edit_receipts(team_id,account_id,device_id,vault_id,resource_id,acl_revision)
 VALUES(s.team_id,s.account_id,s.device_id,v,r,cap.acl_revision) RETURNING id INTO receipt;
 INSERT INTO offline_creation_claims VALUES(s.team_id,scope,r,receipt);
 RETURN jsonb_build_object('writeCapability',receipt);
END $$;
REVOKE ALL ON FUNCTION api_creation_scope(text,uuid,uuid),api_creation_claim(text,uuid,uuid,uuid,uuid) FROM PUBLIC;
INSERT INTO schema_migrations VALUES(43,now());
COMMIT;
