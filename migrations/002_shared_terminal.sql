BEGIN;
CREATE OR REPLACE FUNCTION termterm.open_terminal(v uuid,s uuid,l uuid) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,termterm AS $$
BEGIN
 IF COALESCE(termterm.member_role(v),'') NOT IN ('owner','editor') THEN RAISE EXCEPTION 'Editor permission required'; END IF;
 INSERT INTO termterm.terminal_sessions VALUES(s,v,session_user,session_user,l,now()+interval '10 seconds');
END $$;
CREATE OR REPLACE FUNCTION termterm.control_terminal(s uuid,w text,l uuid,finish boolean DEFAULT false) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,termterm AS $$
DECLARE v uuid;
BEGIN
 SELECT vault_id INTO v FROM termterm.terminal_sessions WHERE id=s AND owner=session_user AND expires_at>now() FOR UPDATE;
 IF NOT FOUND OR COALESCE(termterm.member_role(v),'') NOT IN ('owner','editor') THEN RAISE EXCEPTION 'Active session owner required'; END IF;
 IF NOT finish AND NOT EXISTS(SELECT FROM termterm.members WHERE vault_id=v AND principal=w::name AND role IN ('owner','editor')) THEN RAISE EXCEPTION 'Writer must be a vault editor'; END IF;
 UPDATE termterm.terminal_sessions SET writer=w::name,lease=l,expires_at=CASE WHEN finish THEN now() ELSE now()+interval '10 seconds' END WHERE id=s;
 DELETE FROM termterm.terminal_frames WHERE session_id=s AND kind='input';
END $$;
CREATE OR REPLACE FUNCTION termterm.renew_terminal(s uuid) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,termterm AS $$
BEGIN
 UPDATE termterm.terminal_sessions SET expires_at=now()+interval '10 seconds' WHERE id=s AND owner=session_user AND expires_at>now() AND termterm.member_role(vault_id) IN ('owner','editor');
 IF NOT FOUND THEN RAISE EXCEPTION 'Shared session expired'; END IF;
 DELETE FROM termterm.terminal_frames WHERE expires_at<now();
END $$;
CREATE OR REPLACE FUNCTION termterm.send_terminal_frame(s uuid,l uuid,k text,p bytea) RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,termterm AS $$
DECLARE t termterm.terminal_sessions; n bigint;
BEGIN
 SELECT * INTO t FROM termterm.terminal_sessions WHERE id=s AND expires_at>now();
 IF NOT FOUND OR termterm.member_role(t.vault_id) IS NULL OR (k='input' AND t.lease<>l) THEN RAISE EXCEPTION 'Shared session expired or control changed'; END IF;
 IF octet_length(p)>262144 THEN RAISE EXCEPTION 'Frame too large'; END IF;
 IF (k='input' AND (t.writer<>session_user OR termterm.member_role(t.vault_id) NOT IN ('owner','editor'))) OR (k IN ('output','close') AND t.owner<>session_user) OR k NOT IN ('input','output','close') THEN RAISE EXCEPTION 'Terminal permission denied'; END IF;
 INSERT INTO termterm.terminal_frames(session_id,sender,kind,lease,payload,expires_at) VALUES(s,session_user,k,l,p,now()+CASE WHEN k='input' THEN interval '2 seconds' ELSE interval '30 seconds' END) RETURNING seq INTO n;
 RETURN n;
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm FROM PUBLIC;
COMMIT;
