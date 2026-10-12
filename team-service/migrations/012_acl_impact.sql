BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION api_acl_impact(token text,plan uuid,after_id uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE p acl_change_plans;items jsonb;
BEGIN
 p:=acl_plan_actor(token,plan);
 IF p.status<>'pending' OR p.expires_at<=now() THEN RAISE EXCEPTION 'invalid_plan' USING ERRCODE='22023'; END IF;
 IF p.acl_revision<>(SELECT acl_revision FROM teams WHERE id=p.team_id)
 OR p.vault_revision<>(SELECT revision FROM vaults WHERE team_id=p.team_id AND id=p.vault_id)
 THEN RAISE EXCEPTION 'preview_changed' USING ERRCODE='40001'; END IF;
 WITH permissions AS MATERIALIZED (
  SELECT r.id,r.kind,
   ARRAY(SELECT permission FROM unnest(ARRAY['read','connect','edit'])permission WHERE allowed(p.team_id,p.account_id,r.id,permission)) AS before_permissions,
   ARRAY(SELECT permission FROM unnest(ARRAY['read','connect','edit'])permission WHERE allowed_proposed(p.team_id,p.account_id,r.id,permission,p.resource_id,p.changes)) AS after_permissions
  FROM resources r WHERE r.team_id=p.team_id AND r.vault_id=p.vault_id AND NOT r.deleted AND (after_id IS NULL OR r.id>after_id)
 ), page AS (SELECT * FROM permissions WHERE before_permissions<>after_permissions ORDER BY id LIMIT 20)
 SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'kind',kind,'before',before_permissions,'after',after_permissions) ORDER BY id),'[]') INTO items FROM page;
 RETURN jsonb_build_object('records',items,'nextAfter',CASE WHEN jsonb_array_length(items)>0 THEN items->(jsonb_array_length(items)-1)->>'id' END);
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(12,now());
COMMIT;
