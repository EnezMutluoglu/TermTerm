BEGIN;
SET LOCAL search_path=termterm_team_v2,pg_catalog;
CREATE FUNCTION valid_password(p text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
 SELECT p IS NOT NULL AND length(p)>=12 AND octet_length(p)<=72 AND position(chr(1) in p)=0
$$;
CREATE FUNCTION temporary_password() RETURNS text LANGUAGE plpgsql VOLATILE AS $$
DECLARE alphabet text := 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789.,:;!?@#%+-_'; result text; n int; b int;
BEGIN
 LOOP
  result:='';
  WHILE length(result)<21 LOOP
   b:=get_byte(public.gen_random_bytes(1),0); n:=length(alphabet);
   IF b < (256/n)*n THEN result:=result || substr(alphabet,(b%n)+1,1); END IF;
  END LOOP;
  EXIT WHEN result ~ '[A-Z]' AND result ~ '[a-z]' AND result ~ '[0-9]' AND result ~ '[.,:;!?@#%+_-]';
 END LOOP;
 RETURN result;
END $$;
CREATE FUNCTION session_actor(token text, activation boolean DEFAULT false) RETURNS auth_sessions
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 SELECT x.* INTO s FROM auth_sessions x JOIN team_accounts a ON (a.team_id,a.id)=(x.team_id,x.account_id)
 WHERE x.access_hash=public.digest(token,'sha256') AND x.revoked_at IS NULL AND x.rotated_at IS NULL
 AND x.access_expires>clock_timestamp() AND x.auth_epoch=a.auth_epoch
 AND ((a.status='active' AND NOT x.activation_only) OR (activation AND a.status='pending' AND x.activation_only));
 IF s.id IS NULL THEN RAISE EXCEPTION 'invalid_session' USING ERRCODE='28000'; END IF;
 RETURN s;
END $$;
CREATE FUNCTION require_owner(s auth_sessions) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT FROM team_memberships WHERE team_id=s.team_id AND account_id=s.account_id AND role='owner')
 THEN RAISE EXCEPTION 'permission_denied' USING ERRCODE='42501'; END IF;
END $$;
CREATE FUNCTION issue_session(t uuid,a uuid,d uuid,activation boolean,family uuid DEFAULT NULL) RETURNS jsonb
LANGUAGE plpgsql AS $$
DECLARE access_token text:=encode(public.gen_random_bytes(32),'hex'); refresh_token text:=encode(public.gen_random_bytes(32),'hex'); s auth_sessions;
BEGIN
 INSERT INTO auth_sessions(family_id,team_id,account_id,device_id,auth_epoch,access_hash,refresh_hash,activation_only,access_expires,refresh_expires)
 SELECT coalesce(family,gen_random_uuid()),t,a,d,auth_epoch,public.digest(access_token,'sha256'),public.digest(refresh_token,'sha256'),activation,
 clock_timestamp()+interval '15 minutes',clock_timestamp()+CASE WHEN activation THEN interval '15 minutes' ELSE interval '30 days' END
 FROM team_accounts WHERE team_id=t AND id=a RETURNING * INTO s;
 RETURN jsonb_build_object('teamId',t,'accountId',a,'accessToken',access_token,'refreshToken',refresh_token,
 'expiresAt',s.access_expires,'activationRequired',activation);
END $$;
-- This function is migration-only: there is deliberately no public bootstrap HTTP route.
CREATE FUNCTION bootstrap_team(team_name text, username text, display_name text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE t uuid; a uuid; p text:=temporary_password();
BEGIN
 INSERT INTO teams(name) VALUES(team_name) RETURNING id INTO t;
 INSERT INTO team_accounts(team_id,username,display_name) VALUES(t,username,display_name) RETURNING id INTO a;
 INSERT INTO team_memberships VALUES(t,a,'owner');
 INSERT INTO activation_tokens VALUES(t,a,1,public.crypt(p,public.gen_salt('bf',12)),clock_timestamp()+interval '24 hours',NULL);
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(t,a,'team.bootstrap',a,'succeeded');
 RETURN jsonb_build_object('teamId',t,'accountId',a,'temporaryPassword',p,'expiresInHours',24);
END $$;
CREATE FUNCTION api_login(t uuid,login text,p text,device uuid,device_name text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE a team_accounts; act activation_tokens; rate auth_attempts; k bytea; h text; ok boolean:=false; answer jsonb;
BEGIN
 IF length(login)>120 OR octet_length(p)>72 OR length(device_name) NOT BETWEEN 1 AND 160 THEN RETURN '{"error":"invalid_credentials"}'; END IF;
 k:=public.digest(t::text||':'||lower(login),'sha256');
 INSERT INTO auth_attempts VALUES(k,clock_timestamp(),0) ON CONFLICT DO NOTHING;
 SELECT * INTO rate FROM auth_attempts WHERE key_hash=k FOR UPDATE;
 IF rate.window_at < clock_timestamp()-interval '15 minutes' THEN
  UPDATE auth_attempts SET window_at=clock_timestamp(),failures=0 WHERE key_hash=k; rate.failures:=0;
 END IF;
 IF rate.failures>=5 THEN RETURN '{"error":"rate_limited"}'; END IF;
 SELECT * INTO a FROM team_accounts WHERE team_id=t AND lower(username)=lower(login) FOR UPDATE;
 IF a.status='pending' THEN
  SELECT * INTO act FROM activation_tokens WHERE team_id=t AND account_id=a.id AND consumed_at IS NULL AND expires_at>clock_timestamp();
  h:=act.password_hash;
 ELSIF a.status='active' THEN h:=a.password_hash;
 END IF;
 IF h IS NULL THEN SELECT dummy_hash INTO h FROM auth_policy; END IF;
 ok:=public.crypt(p,h)=h AND a.id IS NOT NULL AND (a.status='active' OR (a.status='pending' AND act.account_id IS NOT NULL));
 IF NOT coalesce(ok,false) THEN
  UPDATE auth_attempts SET failures=failures+1 WHERE key_hash=k;
  RETURN '{"error":"invalid_credentials"}';
 END IF;
 UPDATE auth_attempts SET failures=0 WHERE key_hash=k;
 INSERT INTO account_devices VALUES(t,a.id,device,device_name,clock_timestamp())
 ON CONFLICT(team_id,account_id,id) DO UPDATE SET last_seen=clock_timestamp(),name=excluded.name;
 answer:=issue_session(t,a.id,device,a.status='pending');
 IF a.status='active' THEN
  answer:=answer || jsonb_build_object('identity',(SELECT jsonb_build_object('version',version,'publicKey',encode(public_key,'hex'),
  'encryptedPrivateKey',encode(encrypted_private_key,'hex')) FROM account_crypto_identities WHERE team_id=t AND account_id=a.id AND active));
 END IF;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(t,a.id,'account.login','succeeded');
 RETURN answer;
END $$;
CREATE FUNCTION api_activate(token text,p text,pub bytea,priv bytea) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; v bigint; old_count bigint;
BEGIN
 s:=session_actor(token,true);
 PERFORM 1 FROM team_accounts WHERE team_id=s.team_id AND id=s.account_id FOR UPDATE;
 s:=session_actor(token,true);
 IF NOT s.activation_only THEN RAISE EXCEPTION 'activation_required' USING ERRCODE='42501'; END IF;
 IF NOT valid_password(p) OR octet_length(pub)<>32 OR octet_length(priv) NOT BETWEEN 64 AND 65536 THEN
  RAISE EXCEPTION 'invalid_activation' USING ERRCODE='22023'; END IF;
 IF NOT EXISTS(SELECT FROM activation_tokens WHERE team_id=s.team_id AND account_id=s.account_id AND consumed_at IS NULL
 AND expires_at>clock_timestamp() AND public.crypt(p,password_hash)<>password_hash) THEN
  RAISE EXCEPTION 'activation_expired_or_password_unchanged' USING ERRCODE='22023'; END IF;
 SELECT coalesce(max(version),0)+1,count(*) INTO v,old_count FROM account_crypto_identities WHERE team_id=s.team_id AND account_id=s.account_id;
 UPDATE account_crypto_identities SET active=false WHERE team_id=s.team_id AND account_id=s.account_id;
 INSERT INTO account_crypto_identities(team_id,account_id,version,public_key,encrypted_private_key) VALUES(s.team_id,s.account_id,v,pub,priv);
 UPDATE team_accounts SET password_hash=public.crypt(p,public.gen_salt('bf',12)),status='active',auth_epoch=auth_epoch+1 WHERE team_id=s.team_id AND id=s.account_id;
 UPDATE activation_tokens SET consumed_at=clock_timestamp() WHERE team_id=s.team_id AND account_id=s.account_id AND consumed_at IS NULL;
 UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE team_id=s.team_id AND account_id=s.account_id AND revoked_at IS NULL;
 INSERT INTO audit_events(team_id,actor_id,action,outcome) VALUES(s.team_id,s.account_id,'account.activate','succeeded');
 RETURN jsonb_build_object('activated',true,'loginRequired',true,'recoveryRequired',old_count>0);
END $$;
CREATE FUNCTION api_refresh(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; a team_accounts;
BEGIN
 SELECT * INTO s FROM auth_sessions WHERE refresh_hash=public.digest(token,'sha256') FOR UPDATE;
 IF s.id IS NULL THEN RETURN '{"error":"invalid_session"}'; END IF;
 -- Returning an error instead of raising commits family revocation on replay.
 IF s.rotated_at IS NOT NULL THEN
  UPDATE auth_sessions SET revoked_at=coalesce(revoked_at,clock_timestamp()) WHERE family_id=s.family_id;
  RETURN '{"error":"refresh_replayed"}';
 END IF;
 SELECT * INTO a FROM team_accounts WHERE team_id=s.team_id AND id=s.account_id FOR UPDATE;
 IF s.revoked_at IS NOT NULL OR s.refresh_expires<=clock_timestamp() OR s.auth_epoch<>a.auth_epoch OR a.status<>'active' OR s.activation_only
 THEN RETURN '{"error":"invalid_session"}'; END IF;
 UPDATE auth_sessions SET rotated_at=clock_timestamp() WHERE id=s.id;
 RETURN issue_session(s.team_id,s.account_id,s.device_id,false,s.family_id);
END $$;
CREATE FUNCTION api_logout(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token,true);
 UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE family_id=s.family_id;
 RETURN '{"loggedOut":true}';
END $$;
CREATE FUNCTION api_me(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token);
 RETURN (SELECT jsonb_build_object('teamId',a.team_id,'accountId',a.id,'username',a.username,'displayName',a.display_name,'role',m.role,
 'teamName',t.name,'aclRevision',t.acl_revision,'offlineHours',t.offline_hours)
 FROM team_accounts a JOIN team_memberships m ON (m.team_id,m.account_id)=(a.team_id,a.id) JOIN teams t ON t.id=a.team_id
 WHERE a.team_id=s.team_id AND a.id=s.account_id);
END $$;
CREATE FUNCTION api_members(token text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token); PERFORM require_owner(s);
 RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('accountId',a.id,'username',a.username,'displayName',a.display_name,'status',a.status,'role',m.role) ORDER BY a.username)
 FROM team_accounts a JOIN team_memberships m ON (m.team_id,m.account_id)=(a.team_id,a.id) WHERE a.team_id=s.team_id),'[]');
END $$;
CREATE FUNCTION api_create_member(token text,u text,n text,r text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; a uuid; p text:=temporary_password();
BEGIN
 s:=session_actor(token); PERFORM require_owner(s);
 IF r NOT IN ('observer','operator','editor','manager') THEN RAISE EXCEPTION 'invalid_role' USING ERRCODE='22023'; END IF;
 INSERT INTO team_accounts(team_id,username,display_name) VALUES(s.team_id,u,n) RETURNING id INTO a;
 INSERT INTO team_memberships VALUES(s.team_id,a,r);
 INSERT INTO activation_tokens VALUES(s.team_id,a,1,public.crypt(p,public.gen_salt('bf',12)),clock_timestamp()+interval '24 hours',NULL);
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'account.create',a,'succeeded');
 RETURN jsonb_build_object('accountId',a,'temporaryPassword',p,'expiresInHours',24);
END $$;
CREATE FUNCTION api_reset_member(token text,target uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions; p text:=temporary_password(); gen bigint;
BEGIN
 s:=session_actor(token); PERFORM require_owner(s);
 IF target=s.account_id THEN RAISE EXCEPTION 'cannot_reset_self' USING ERRCODE='42501'; END IF;
 PERFORM 1 FROM team_accounts WHERE team_id=s.team_id AND id=target FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 UPDATE team_accounts SET status='pending',password_hash=NULL,auth_epoch=auth_epoch+1 WHERE team_id=s.team_id AND id=target;
 UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE team_id=s.team_id AND account_id=target AND revoked_at IS NULL;
 SELECT coalesce(max(generation),0)+1 INTO gen FROM activation_tokens WHERE team_id=s.team_id AND account_id=target;
 UPDATE activation_tokens SET consumed_at=coalesce(consumed_at,clock_timestamp()) WHERE team_id=s.team_id AND account_id=target;
 INSERT INTO activation_tokens VALUES(s.team_id,target,gen,public.crypt(p,public.gen_salt('bf',12)),clock_timestamp()+interval '24 hours',NULL);
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'account.reset',target,'succeeded');
 RETURN jsonb_build_object('accountId',target,'temporaryPassword',p,'expiresInHours',24);
END $$;
CREATE FUNCTION api_disable_member(token text,target uuid) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 s:=session_actor(token); PERFORM require_owner(s);
 IF target=s.account_id THEN RAISE EXCEPTION 'cannot_disable_self' USING ERRCODE='42501'; END IF;
 UPDATE team_accounts SET status='disabled',auth_epoch=auth_epoch+1 WHERE team_id=s.team_id AND id=target;
 IF NOT FOUND THEN RAISE EXCEPTION 'not_found' USING ERRCODE='P0002'; END IF;
 UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE team_id=s.team_id AND account_id=target AND revoked_at IS NULL;
 UPDATE activation_tokens SET consumed_at=coalesce(consumed_at,clock_timestamp()) WHERE team_id=s.team_id AND account_id=target;
 UPDATE teams SET acl_revision=acl_revision+1 WHERE id=s.team_id;
 INSERT INTO audit_events(team_id,actor_id,action,subject_id,outcome) VALUES(s.team_id,s.account_id,'account.disable',target,'succeeded');
 RETURN '{"disabled":true}';
END $$;
CREATE FUNCTION api_version() RETURNS jsonb LANGUAGE sql SECURITY DEFINER SET search_path=termterm_team_v2,pg_catalog,pg_temp AS $$
 SELECT jsonb_build_object('schemaVersion',max(version),'apiVersion',1) FROM schema_migrations
$$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team_v2 FROM PUBLIC;
INSERT INTO schema_migrations VALUES(2,now());
COMMIT;
