-- psql -v app_role=termterm_team_app -f grant_app.sql (administrator only)
-- Intentionally explicit: private helpers and future functions are NOT exposed.
GRANT USAGE ON SCHEMA termterm_team TO :"app_role";
GRANT EXECUTE ON FUNCTION termterm_team.register_account(text,text,text,bytea,bytea,uuid,text) TO :"app_role";
GRANT EXECUTE ON FUNCTION termterm_team.login_account(text,text,uuid,text) TO :"app_role";
GRANT EXECUTE ON FUNCTION termterm_team.refresh_session(text) TO :"app_role";
GRANT EXECUTE ON FUNCTION termterm_team.rpc(text,text,jsonb) TO :"app_role";
GRANT EXECUTE ON FUNCTION termterm_team.deliver_pending(text,jsonb) TO :"app_role";
