-- psql -v app_role=termterm_app -f grant_app.sql (repeat per distinct login)
GRANT USAGE ON SCHEMA termterm TO :"app_role";
GRANT SELECT ON termterm.schema_version,termterm.vaults,termterm.members,termterm.records,termterm.terminal_sessions,termterm.terminal_frames TO :"app_role";
REVOKE INSERT,UPDATE ON termterm.terminal_sessions FROM :"app_role";
REVOKE INSERT ON termterm.terminal_frames FROM :"app_role";
GRANT USAGE ON ALL SEQUENCES IN SCHEMA termterm TO :"app_role";
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA termterm TO :"app_role";
