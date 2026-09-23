BEGIN;
SET search_path=termterm_team,public;
-- The executor is not the table owner and has no BYPASSRLS. These policies
-- protect encrypted content even if a future RPC forgets its record filter.
DROP POLICY executor_only ON record_versions;
CREATE POLICY versions_read ON record_versions FOR SELECT TO termterm_team_executor USING (
 EXISTS(SELECT FROM team_records r WHERE r.id=record_id AND (allowed(actor(),r.vault_id,r.id,'read') OR connection_dependency(actor(),r.vault_id,r.id))));
CREATE POLICY versions_insert ON record_versions FOR INSERT TO termterm_team_executor WITH CHECK (
 actor_id=actor() AND EXISTS(SELECT FROM team_records r WHERE r.id=record_id AND allowed(actor(),r.vault_id,r.id,'edit')));
CREATE POLICY versions_prune ON record_versions FOR DELETE TO termterm_team_executor USING (
 EXISTS(SELECT FROM team_records r WHERE r.id=record_id AND allowed(actor(),r.vault_id,r.id,'edit')));
DROP POLICY executor_only ON record_key_envelopes;
CREATE POLICY envelopes_read ON record_key_envelopes FOR SELECT TO termterm_team_executor USING(user_id=actor() OR EXISTS(SELECT FROM team_records r WHERE r.id=record_id AND allowed(actor(),r.vault_id,r.id,'manage')));
CREATE POLICY envelopes_insert ON record_key_envelopes FOR INSERT TO termterm_team_executor WITH CHECK (
 EXISTS(SELECT FROM team_records r WHERE r.id=record_id AND (allowed(actor(),r.vault_id,r.id,'edit') OR allowed(actor(),r.vault_id,r.id,'manage'))));
CREATE POLICY envelopes_update ON record_key_envelopes FOR UPDATE TO termterm_team_executor USING (
 EXISTS(SELECT FROM team_records r WHERE r.id=record_id AND (allowed(actor(),r.vault_id,r.id,'edit') OR allowed(actor(),r.vault_id,r.id,'manage'))));
CREATE POLICY envelopes_delete ON record_key_envelopes FOR DELETE TO termterm_team_executor USING (
 EXISTS(SELECT FROM team_records r JOIN team_vaults v ON v.id=r.vault_id WHERE r.id=record_id AND (is_owner(v.team_id,actor()) OR allowed(actor(),r.vault_id,r.id,'manage') OR NOT is_member(v.team_id,user_id))));
DROP POLICY executor_only ON audit_events;
CREATE POLICY audit_read ON audit_events FOR SELECT TO termterm_team_executor USING (
 is_owner(team_id,actor()) OR (actor_id=actor() AND is_member(team_id,actor())) OR EXISTS(SELECT FROM team_records r WHERE r.id=record_id AND allowed(actor(),r.vault_id,r.id,'read')));
CREATE POLICY audit_insert ON audit_events FOR INSERT TO termterm_team_executor WITH CHECK(actor_id=actor());
CREATE POLICY audit_prune ON audit_events FOR DELETE TO termterm_team_executor USING(is_owner(team_id,actor()));
COMMIT;
