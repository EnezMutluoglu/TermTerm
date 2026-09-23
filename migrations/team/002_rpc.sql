BEGIN;
SET search_path=termterm_team,public;
CREATE OR REPLACE FUNCTION connection_dependency(u uuid,v uuid,r uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 WITH RECURSIVE deps(id,seen) AS (
 SELECT tr.id,ARRAY[tr.id] FROM team_records tr WHERE tr.vault_id=v AND tr.kind='host' AND NOT tr.deleted AND allowed(u,v,tr.id,'connect')
 UNION ALL SELECT target,seen||target FROM deps d JOIN LATERAL (
 SELECT l.target_id AS target FROM record_links l WHERE l.record_id=d.id AND l.relation IN ('identity','chain')
 UNION SELECT tr.parent_id FROM team_records tr WHERE tr.id=d.id AND tr.parent_id IS NOT NULL
 ) links ON NOT target=ANY(d.seen)) SELECT EXISTS(SELECT FROM deps WHERE id=r)
$$;
CREATE OR REPLACE FUNCTION recipients(v uuid,r uuid,parent uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT coalesce(jsonb_agg(jsonb_build_object('userId',u.id,'publicKey',encode(u.public_key,'hex'),'data',allowed(u.id,v,coalesce(r,parent),'read') OR connection_dependency(u.id,v,r),'secret',allowed(u.id,v,coalesce(r,parent),'connect') OR allowed(u.id,v,coalesce(r,parent),'reveal') OR allowed(u.id,v,coalesce(r,parent),'edit') OR connection_dependency(u.id,v,r))),'[]')
 FROM users u JOIN team_members m ON m.user_id=u.id JOIN team_vaults tv ON tv.team_id=m.team_id
 WHERE tv.id=v AND m.active AND u.active AND (allowed(u.id,v,coalesce(r,parent),'read') OR allowed(u.id,v,coalesce(r,parent),'connect') OR allowed(u.id,v,coalesce(r,parent),'reveal') OR allowed(u.id,v,coalesce(r,parent),'edit') OR connection_dependency(u.id,v,r))
$$;
CREATE OR REPLACE FUNCTION prune_versions(r uuid) RETURNS void LANGUAGE sql AS $$
 DELETE FROM record_versions old WHERE old.record_id=r AND old.revision NOT IN (SELECT revision FROM record_versions WHERE record_id=r ORDER BY revision DESC LIMIT 11)
 AND NOT EXISTS(SELECT FROM revision_pins p WHERE p.record_id=old.record_id AND p.revision=old.revision)
$$;
CREATE OR REPLACE FUNCTION apply_change(b jsonb,delivery boolean DEFAULT false) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE u uuid=actor(); d uuid=current_setting('termterm.device')::uuid; v uuid=(b->>'vaultId')::uuid; r uuid=(b->>'recordId')::uuid; o uuid=(b->>'operationId')::uuid;
 t uuid; head team_records; rev bigint; response jsonb; permitted boolean; prior change_operations; parent uuid=nullif(b->>'parentId','')::uuid; e jsonb; target uuid; reason text; c uuid;
BEGIN
 SELECT team_id INTO t FROM team_vaults WHERE id=v;
 IF t IS NULL THEN RAISE EXCEPTION 'Unknown vault'; END IF;
 -- Serialize ACL changes and record mutations within each team, before CAS.
 PERFORM 1 FROM teams WHERE id=t FOR UPDATE;
 SELECT * INTO prior FROM change_operations WHERE id=o;
 IF prior.id IS NOT NULL THEN
   IF prior.user_id<>u OR prior.device_id<>d OR prior.record_id<>r THEN RAISE EXCEPTION 'Operation identity mismatch'; END IF;
   RETURN prior.result;
 END IF;
 IF EXISTS(SELECT FROM change_operations WHERE device_id=d AND sequence=(b->>'sequence')::bigint) THEN RAISE EXCEPTION 'Device sequence already belongs to another operation'; END IF;
 IF octet_length(b::text)>4194304 THEN RAISE EXCEPTION 'Record too large'; END IF;
 SELECT * INTO head FROM team_records WHERE id=r FOR UPDATE;
 IF head.id IS NOT NULL AND head.vault_id<>v THEN RAISE EXCEPTION 'Record identity belongs to another vault'; END IF;
 permitted=NOT delivery AND allowed(u,v,coalesce(head.id,parent),'edit');
 IF head.id IS NULL AND NOT is_member(t,u) THEN RAISE EXCEPTION 'No previous membership for creation'; END IF;
 IF NOT EXISTS(SELECT FROM team_members WHERE team_id=t AND user_id=u) THEN RAISE EXCEPTION 'No previous membership'; END IF;
 rev=coalesce(head.revision,0);
 IF NOT permitted OR rev<>(b->>'expectedRevision')::bigint THEN
   reason=CASE WHEN NOT permitted THEN 'permission' ELSE 'revision' END;
   response=jsonb_build_object('status','conflict','reason',reason,'operationId',o);
   INSERT INTO change_operations VALUES(o,u,d,(b->>'sequence')::bigint,r,v,(b->>'expectedRevision')::bigint,(b->>'clientAt')::timestamptz,now(),response);
   INSERT INTO conflicts(operation_id,record_id,vault_id,author_id,candidate,reason) VALUES(o,r,v,u,b,reason) RETURNING id INTO c;
   response=response||jsonb_build_object('conflictId',c);
   UPDATE change_operations SET result=response WHERE id=o;
   PERFORM audit(t,r,'decision.requested',jsonb_build_object('reason',reason,'conflictId',c));
   RETURN response;
 END IF;
 IF (b->>'aclRevision')::bigint<>(SELECT acl_revision FROM teams WHERE id=t) THEN RAISE EXCEPTION 'TEAM_ACL_CHANGED: refresh recipients before encrypting' USING ERRCODE='40001'; END IF;
 IF head.id IS NOT NULL AND head.kind<>b->>'kind' THEN RAISE EXCEPTION 'Record kind is immutable'; END IF;
 IF parent IS NOT NULL THEN
   IF NOT EXISTS(SELECT FROM team_records WHERE id=parent AND vault_id=v AND kind='group' AND NOT deleted) OR parent=r OR r IN (SELECT ancestors(v,parent)) THEN RAISE EXCEPTION 'Invalid folder relationship'; END IF;
   PERFORM require_permission(v,parent,'edit');
 END IF;
 IF head.id IS NOT NULL AND head.parent_id IS DISTINCT FROM parent THEN
   PERFORM require_permission(v,r,'manage'); PERFORM require_permission(v,parent,'manage');
   IF coalesce((b->>'movePreviewRevision')::bigint,-1)<>(SELECT acl_revision FROM teams WHERE id=t) THEN RAISE EXCEPTION 'Online move preview required'; END IF;
   UPDATE team_records SET parent_id=parent WHERE id=r;
 END IF;
 IF coalesce((b->>'deleted')::boolean,false) AND (EXISTS(SELECT FROM team_records WHERE parent_id=r AND NOT deleted) OR EXISTS(SELECT FROM record_links l JOIN team_records tr ON tr.id=l.record_id WHERE l.target_id=r AND tr.id<>r AND NOT tr.deleted)) THEN RAISE EXCEPTION 'Record still has dependent records'; END IF;
 -- Reject unknown, extra or missing recipients, not merely invalid encryption.
 IF EXISTS(
   SELECT x->>'userId',p FROM jsonb_array_elements(recipients(v,head.id,parent)) x CROSS JOIN unnest(ARRAY['data','secret']) p WHERE (x->>p)::boolean
   EXCEPT SELECT x->>'userId',x->>'purpose' FROM jsonb_array_elements(b->'envelopes') x
 ) OR EXISTS(
   SELECT x->>'userId',x->>'purpose' FROM jsonb_array_elements(b->'envelopes') x
   EXCEPT SELECT x->>'userId',p FROM jsonb_array_elements(recipients(v,head.id,parent)) x CROSS JOIN unnest(ARRAY['data','secret']) p WHERE (x->>p)::boolean
 ) THEN RAISE EXCEPTION 'Recipient set does not match current permissions'; END IF;
 rev=rev+1;
 INSERT INTO team_records(id,vault_id,kind,parent_id,revision,payload,secrets,deleted,updated_by) VALUES(r,v,b->>'kind',parent,rev,decode(b->>'payload','hex'),decode(b->>'secrets','hex'),coalesce((b->>'deleted')::boolean,false),u)
 ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id,revision=excluded.revision,payload=excluded.payload,secrets=excluded.secrets,deleted=excluded.deleted,updated_by=u,updated_at=now();
 DELETE FROM record_links WHERE record_id=r;
 FOR e IN SELECT * FROM jsonb_array_elements(coalesce(b->'links','[]')) LOOP
   target=(e->>'targetId')::uuid;
   IF target=r OR NOT EXISTS(SELECT FROM team_records WHERE id=target AND vault_id=v AND NOT deleted) THEN RAISE EXCEPTION 'Invalid dependency'; END IF;
   PERFORM require_permission(v,target,'read');
   INSERT INTO record_links VALUES(r,target,e->>'relation');
 END LOOP;
 INSERT INTO record_versions VALUES(r,rev,rev-1,decode(b->>'payload','hex'),decode(b->>'secrets','hex'),coalesce((b->>'deleted')::boolean,false),parent,b->>'kind',u,d,(b->>'clientAt')::timestamptz,now(),nullif(b->>'restoreFrom','')::bigint);
 FOR e IN SELECT * FROM jsonb_array_elements(b->'envelopes') LOOP
   INSERT INTO record_key_envelopes VALUES(r,rev,(e->>'userId')::uuid,e->>'purpose',decode(e->>'envelope','hex'));
 END LOOP;
 IF head.id IS NOT NULL AND head.parent_id IS DISTINCT FROM parent THEN
   DELETE FROM record_key_envelopes k USING team_records tr WHERE tr.id=k.record_id AND tr.vault_id=v AND NOT EXISTS(SELECT FROM jsonb_array_elements(recipients(v,tr.id,NULL)) x WHERE x->>'userId'=k.user_id::text AND (x->>k.purpose)::boolean);
   FOR e IN SELECT * FROM jsonb_array_elements(coalesce(b->'rewrap','[]')) LOOP
     IF NOT EXISTS(SELECT FROM team_records tr WHERE tr.id=(e->>'recordId')::uuid AND tr.vault_id=v AND allowed(u,v,tr.id,'manage')) OR NOT EXISTS(SELECT FROM jsonb_array_elements(recipients(v,(e->>'recordId')::uuid,NULL)) x WHERE x->>'userId'=e->>'userId' AND (x->>(e->>'purpose'))::boolean) THEN RAISE EXCEPTION 'Invalid move key distribution'; END IF;
     INSERT INTO record_key_envelopes VALUES((e->>'recordId')::uuid,(e->>'revision')::bigint,(e->>'userId')::uuid,e->>'purpose',decode(e->>'envelope','hex')) ON CONFLICT(record_id,revision,user_id,purpose) DO UPDATE SET envelope=excluded.envelope;
   END LOOP;
   UPDATE teams SET acl_revision=acl_revision+1 WHERE id=t;
 END IF;
 IF b->>'restoreFrom' IS NOT NULL AND head.id IS NOT NULL THEN
   INSERT INTO revision_pins VALUES(r,head.revision,u,'return') ON CONFLICT(record_id,user_id,purpose) DO UPDATE SET revision=excluded.revision;
 END IF;
 response=jsonb_build_object('status','applied','revision',rev,'operationId',o);
 INSERT INTO change_operations VALUES(o,u,d,(b->>'sequence')::bigint,r,v,(b->>'expectedRevision')::bigint,(b->>'clientAt')::timestamptz,now(),response);
 UPDATE team_vaults SET revision=revision+1 WHERE id=v;
 PERFORM audit(t,r,CASE WHEN b->>'restoreFrom' IS NOT NULL THEN 'record.restored' WHEN (b->>'deleted')::boolean THEN 'record.deleted' ELSE 'record.saved' END,jsonb_build_object('revision',rev));
 PERFORM prune_versions(r);
 PERFORM pg_notify('termterm_team_changes',t::text);
 RETURN response;
END $$;

CREATE OR REPLACE FUNCTION rpc(token text,action text,b jsonb DEFAULT '{}') RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team,public,pg_temp AS $$
DECLARE s auth_sessions; u uuid; t uuid=nullif(b->>'teamId','')::uuid; v uuid=nullif(b->>'vaultId','')::uuid; r uuid=nullif(b->>'recordId','')::uuid;
 target uuid=nullif(b->>'userId','')::uuid; p text; effect text; response jsonb; c conflicts; ev jsonb; old_team uuid;
BEGIN
 -- Clear caller-controlled state before ANY authorization. Never accept a user ID as identity.
 PERFORM set_config('termterm.actor','',true); PERFORM set_config('termterm.device','',true);
 SELECT * INTO s FROM auth_sessions WHERE access_hash=digest(token,'sha256') AND revoked_at IS NULL AND access_expires>now();
 IF s.id IS NULL OR NOT EXISTS(SELECT FROM users WHERE id=s.user_id AND active) THEN RAISE EXCEPTION 'TEAM_AUTH_EXPIRED' USING ERRCODE='28000'; END IF;
 u=s.user_id; PERFORM set_config('termterm.actor',u::text,true); PERFORM set_config('termterm.device',s.device_id::text,true);
 IF v IS NOT NULL THEN SELECT team_id INTO old_team FROM team_vaults WHERE id=v; IF t IS NOT NULL AND t IS DISTINCT FROM old_team THEN RAISE EXCEPTION 'Team/vault mismatch'; END IF; t=old_team; END IF;
 IF r IS NOT NULL AND EXISTS(SELECT FROM team_records WHERE id=r AND vault_id IS DISTINCT FROM v) THEN RAISE EXCEPTION 'Record/vault mismatch'; END IF;
 CASE action
 WHEN 'overview' THEN
   RETURN jsonb_build_object('user',jsonb_build_object('id',u,'username',(SELECT username FROM users WHERE id=u)), 'serverTime',now(),'teams',coalesce((SELECT jsonb_agg(jsonb_build_object('id',tm.id,'name',tm.name,'ownerId',tm.owner_id,'role',m.role,'aclRevision',tm.acl_revision,'offlineHours',tm.offline_hours,'auditDays',tm.audit_days)) FROM teams tm JOIN team_members m ON m.team_id=tm.id WHERE m.user_id=u AND m.active),'[]'));
 WHEN 'logout' THEN UPDATE auth_sessions SET revoked_at=now() WHERE id=s.id; RETURN '{}';
 WHEN 'create_team' THEN
   INSERT INTO teams(name,owner_id) VALUES(b->>'name',u) RETURNING id INTO t;
   INSERT INTO team_members VALUES(t,u,'owner',true); PERFORM audit(t,NULL,'team.created'); RETURN jsonb_build_object('id',t);
 WHEN 'create_vault' THEN
   IF NOT is_owner(t,u) THEN RAISE EXCEPTION 'Owner required' USING ERRCODE='42501'; END IF;
   INSERT INTO team_vaults(team_id,name) VALUES(t,b->>'name') RETURNING id INTO v; PERFORM audit(t,NULL,'vault.created'); RETURN jsonb_build_object('id',v);
 WHEN 'vaults' THEN
   RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',tv.id,'name',tv.name,'revision',tv.revision)) FROM team_vaults tv WHERE tv.team_id=t AND is_member(t,u) AND (is_owner(t,u) OR EXISTS(SELECT FROM acl_entries a WHERE a.vault_id=tv.id AND a.user_id=u AND a.effect='allow'))),'[]');
 WHEN 'search_users' THEN
   IF NOT is_owner(t,u) THEN RAISE EXCEPTION 'Owner required' USING ERRCODE='42501'; END IF;
   IF length(b->>'query')<3 THEN RETURN '[]'; END IF;
   RETURN coalesce((SELECT jsonb_agg(x) FROM (SELECT id,username,email FROM users WHERE active AND (strpos(lower(username),lower(b->>'query'))>0 OR lower(email)=lower(b->>'query')) ORDER BY username LIMIT 25)x),'[]');
 WHEN 'members' THEN
   IF NOT is_member(t,u) THEN RAISE EXCEPTION 'Membership required' USING ERRCODE='42501'; END IF;
   RETURN jsonb_build_object('members',coalesce((SELECT jsonb_agg(jsonb_build_object('userId',m.user_id,'username',us.username,'role',m.role,'active',m.active,'publicKey',encode(us.public_key,'hex'))) FROM team_members m JOIN users us ON us.id=m.user_id WHERE m.team_id=t),'[]'),'acl',coalesce((SELECT jsonb_agg(to_jsonb(a)) FROM acl_entries a WHERE a.team_id=t AND (is_owner(t,u) OR a.user_id=u OR allowed(u,a.vault_id,a.resource_id,'manage'))),'[]'),'delegations',coalesce((SELECT jsonb_agg(to_jsonb(a)) FROM acl_delegations a WHERE a.team_id=t AND (is_owner(t,u) OR a.user_id=u)),'[]'));
 WHEN 'member_set' THEN
   PERFORM 1 FROM teams WHERE id=t FOR UPDATE;
   IF NOT is_owner(t,u) OR target=u OR b->>'role'='owner' THEN RAISE EXCEPTION 'Only owner can assign/remove members, not self' USING ERRCODE='42501'; END IF;
   INSERT INTO team_members VALUES(t,target,b->>'role',coalesce((b->>'active')::boolean,true)) ON CONFLICT(team_id,user_id) DO UPDATE SET role=excluded.role,active=excluded.active;
   IF NOT coalesce((b->>'active')::boolean,true) THEN DELETE FROM record_key_envelopes k USING team_records tr,team_vaults tv WHERE k.record_id=tr.id AND tr.vault_id=tv.id AND tv.team_id=t AND k.user_id=target; END IF;
   UPDATE teams SET acl_revision=acl_revision+1 WHERE id=t; PERFORM audit(t,NULL,'member.changed',jsonb_build_object('userId',target,'role',b->>'role','active',b->'active')); RETURN '{}';
 WHEN 'delegate' THEN
   PERFORM 1 FROM teams WHERE id=t FOR UPDATE;
   IF NOT is_owner(t,u) OR target=u OR NOT is_member(t,target) THEN RAISE EXCEPTION 'Owner required' USING ERRCODE='42501'; END IF;
   INSERT INTO acl_delegations VALUES(t,target,v,r,ARRAY(SELECT jsonb_array_elements_text(b->'permissions'))) ON CONFLICT(user_id,vault_id,resource_id) DO UPDATE SET permissions=excluded.permissions;
   UPDATE teams SET acl_revision=acl_revision+1 WHERE id=t; PERFORM audit(t,r,'delegation.changed',jsonb_build_object('userId',target)); RETURN '{}';
 WHEN 'acl_set' THEN
   PERFORM 1 FROM teams WHERE id=t FOR UPDATE;
   p=b->>'permission'; effect=b->>'effect';
   IF NOT is_member(t,target) OR target=(SELECT owner_id FROM teams WHERE id=t) THEN RAISE EXCEPTION 'Invalid ACL target'; END IF;
   IF NOT is_owner(t,u) THEN
     IF target=u OR p='manage' OR NOT allowed(u,v,r,'manage') OR NOT allowed(u,v,r,p) OR NOT EXISTS(SELECT FROM acl_delegations a WHERE a.user_id=u AND a.vault_id=v AND p=ANY(a.permissions) AND (a.resource_id IS NULL OR a.resource_id IN(SELECT ancestors(v,r)))) THEN RAISE EXCEPTION 'Outside delegated scope' USING ERRCODE='42501'; END IF;
   END IF;
   IF (b->>'aclRevision')::bigint<>(SELECT acl_revision FROM teams WHERE id=t) THEN RAISE EXCEPTION 'TEAM_ACL_CHANGED' USING ERRCODE='40001'; END IF;
   IF effect='inherit' THEN DELETE FROM acl_entries WHERE user_id=target AND vault_id=v AND resource_id IS NOT DISTINCT FROM r AND permission=p;
   ELSE INSERT INTO acl_entries VALUES(t,target,v,r,p,effect,u) ON CONFLICT(user_id,vault_id,resource_id,permission) DO UPDATE SET effect=excluded.effect,granted_by=u; END IF;
   -- Remove revoked envelopes immediately. New envelopes are provided by an authorized
   -- client after re-reading permissions; a missing envelope denies decryption.
   DELETE FROM record_key_envelopes k USING team_records tr WHERE k.record_id=tr.id AND tr.vault_id=v AND (NOT is_member(t,k.user_id) OR (k.purpose='data' AND NOT allowed(k.user_id,v,tr.id,'read')) OR (k.purpose='secret' AND NOT (allowed(k.user_id,v,tr.id,'connect') OR allowed(k.user_id,v,tr.id,'reveal') OR allowed(k.user_id,v,tr.id,'edit') OR connection_dependency(k.user_id,v,tr.id))));
   UPDATE teams SET acl_revision=acl_revision+1 WHERE id=t;
   FOR ev IN SELECT * FROM jsonb_array_elements(coalesce(b->'rewrap','[]')) LOOP
     IF NOT EXISTS(SELECT FROM team_records tr WHERE tr.id=(ev->>'recordId')::uuid AND tr.vault_id=v AND (allowed(u,v,tr.id,'edit') OR allowed(u,v,tr.id,'manage'))) OR NOT EXISTS(SELECT FROM jsonb_array_elements(recipients(v,(ev->>'recordId')::uuid,NULL)) x WHERE x->>'userId'=ev->>'userId' AND (x->>(ev->>'purpose'))::boolean) THEN RAISE EXCEPTION 'Invalid key distribution'; END IF;
     INSERT INTO record_key_envelopes VALUES((ev->>'recordId')::uuid,(ev->>'revision')::bigint,(ev->>'userId')::uuid,ev->>'purpose',decode(ev->>'envelope','hex')) ON CONFLICT(record_id,revision,user_id,purpose) DO UPDATE SET envelope=excluded.envelope;
   END LOOP;
   PERFORM audit(t,r,'permission.changed',jsonb_build_object('userId',target,'permission',p,'effect',effect)); RETURN '{}';
 WHEN 'acl_preview' THEN RETURN preview_acl(token,b);
 WHEN 'move_preview' THEN RETURN preview_move(v,r,nullif(b->>'parentId','')::uuid);
 WHEN 'connection_records' THEN
   PERFORM require_permission(v,r,'connect');
   RETURN coalesce((WITH RECURSIVE deps(id,seen) AS (
      SELECT r,ARRAY[r] UNION ALL SELECT targets.link_target,seen||targets.link_target FROM deps d JOIN LATERAL (
       SELECT l.target_id link_target FROM record_links l WHERE l.record_id=d.id AND l.relation IN ('identity','chain')
       UNION SELECT tr.parent_id FROM team_records tr WHERE tr.id=d.id AND tr.parent_id IS NOT NULL
      ) targets ON NOT targets.link_target=ANY(d.seen))
     SELECT jsonb_agg(jsonb_build_object('id',tr.id,'kind',tr.kind,'revision',tr.revision,'payload',encode(tr.payload,'hex'),'secrets',encode(tr.secrets,'hex'),'envelopes',(SELECT coalesce(jsonb_agg(jsonb_build_object('purpose',k.purpose,'envelope',encode(k.envelope,'hex'))),'[]') FROM record_key_envelopes k WHERE k.record_id=tr.id AND k.revision=tr.revision AND k.user_id=u))) FROM team_records tr WHERE tr.id IN(SELECT id FROM deps) AND tr.vault_id=v AND NOT tr.deleted),'[]');
 WHEN 'dependency_cache' THEN
   IF NOT is_member(t,u) THEN RAISE EXCEPTION 'Membership revoked' USING ERRCODE='42501'; END IF;
   RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',tr.id,'kind',tr.kind,'revision',tr.revision,'payload',encode(tr.payload,'hex'),'secrets',encode(tr.secrets,'hex'),'envelopes',(SELECT coalesce(jsonb_agg(jsonb_build_object('purpose',k.purpose,'envelope',encode(k.envelope,'hex'))),'[]') FROM record_key_envelopes k WHERE k.record_id=tr.id AND k.revision=tr.revision AND k.user_id=u))) FROM team_records tr WHERE tr.vault_id=v AND NOT tr.deleted AND connection_dependency(u,v,tr.id)),'[]');
 WHEN 'records' THEN
   IF NOT is_member(t,u) THEN RAISE EXCEPTION 'Membership revoked' USING ERRCODE='42501'; END IF;
   RETURN jsonb_build_object('aclRevision',(SELECT acl_revision FROM teams WHERE id=t),'serverTime',now(),'rootRecipients',CASE WHEN allowed(u,v,NULL,'edit') THEN recipients(v,NULL,NULL) ELSE '[]'::jsonb END,'records',coalesce((SELECT jsonb_agg(jsonb_build_object('id',tr.id,'kind',tr.kind,'parentId',tr.parent_id,'revision',tr.revision,'deleted',tr.deleted,'recipients',CASE WHEN allowed(u,v,tr.id,'edit') THEN recipients(v,tr.id,tr.parent_id) ELSE '[]'::jsonb END,'payload',encode(tr.payload,'hex'),'secrets',CASE WHEN allowed(u,v,tr.id,'connect') OR allowed(u,v,tr.id,'reveal') OR allowed(u,v,tr.id,'edit') THEN encode(tr.secrets,'hex') ELSE NULL END,'permissions',(SELECT coalesce(jsonb_agg(perm.name),'[]') FROM unnest(ARRAY['read','connect','edit','reveal','export','manage']) AS perm(name) WHERE allowed(u,v,tr.id,perm.name)),'envelopes',(SELECT coalesce(jsonb_agg(jsonb_build_object('purpose',k.purpose,'envelope',encode(k.envelope,'hex'))),'[]') FROM record_key_envelopes k WHERE k.record_id=tr.id AND k.revision=tr.revision AND k.user_id=u))) FROM team_records tr WHERE tr.vault_id=v AND allowed(u,v,tr.id,'read')),'[]'),'paths',coalesce((SELECT jsonb_agg(jsonb_build_object('id',tr.id,'parentId',tr.parent_id,'pathOnly',true)) FROM team_records tr WHERE tr.vault_id=v AND tr.kind='group' AND NOT tr.deleted AND NOT allowed(u,v,tr.id,'read') AND EXISTS(SELECT FROM team_records child WHERE child.vault_id=v AND allowed(u,v,child.id,'read') AND tr.id IN(SELECT ancestors(v,child.id)))),'[]'));
 WHEN 'recipients' THEN
   PERFORM require_permission(v,coalesce(r,nullif(b->>'parentId','')::uuid),'edit');
   RETURN jsonb_build_object('aclRevision',(SELECT acl_revision FROM teams WHERE id=t),'recipients',recipients(v,r,nullif(b->>'parentId','')::uuid));
 WHEN 'apply' THEN RETURN apply_change(b);
 WHEN 'history' THEN
   PERFORM require_permission(v,r,'read');
   RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('revision',rv.revision,'parentRevision',rv.parent_revision,'deleted',rv.deleted,'parentId',rv.parent_id,'kind',rv.kind,'actorId',rv.actor_id,'serverAt',rv.server_at,'clientAt',rv.client_at,'restoreFrom',rv.restore_from,'payload',encode(rv.payload,'hex'),'secrets',CASE WHEN allowed(u,v,r,'connect') OR allowed(u,v,r,'reveal') OR allowed(u,v,r,'edit') THEN encode(rv.secrets,'hex') ELSE NULL END,'envelopes',(SELECT coalesce(jsonb_agg(jsonb_build_object('purpose',k.purpose,'envelope',encode(k.envelope,'hex'))),'[]') FROM record_key_envelopes k WHERE k.record_id=r AND k.revision=rv.revision AND k.user_id=u),'pinned',EXISTS(SELECT FROM revision_pins p WHERE p.record_id=r AND p.revision=rv.revision)) ORDER BY rv.revision DESC) FROM record_versions rv WHERE rv.record_id=r),'[]');
 WHEN 'pin' THEN
   PERFORM require_permission(v,r,'read');
   IF b->>'purpose'<>'temporary' THEN RAISE EXCEPTION 'Only temporary pins are client-selectable'; END IF;
   INSERT INTO revision_pins VALUES(r,(b->>'revision')::bigint,u,'temporary') ON CONFLICT(record_id,user_id,purpose) DO UPDATE SET revision=excluded.revision; RETURN '{}';
 WHEN 'unpin' THEN
   DELETE FROM revision_pins WHERE record_id=r AND user_id=u AND purpose=b->>'purpose'; PERFORM prune_versions(r); RETURN '{}';
 WHEN 'conflicts' THEN
   RETURN coalesce((SELECT jsonb_agg(jsonb_build_object('id',cf.id,'recordId',cf.record_id,'vaultId',cf.vault_id,'authorId',cf.author_id,'reason',cf.reason,'candidate',cf.candidate,'receivedAt',op.received_at)) FROM conflicts cf JOIN change_operations op ON op.id=cf.operation_id WHERE cf.vault_id=v AND cf.status='pending' AND allowed(u,v,coalesce((SELECT id FROM team_records WHERE id=cf.record_id),nullif(cf.candidate->>'parentId','')::uuid),'edit')),'[]');
 WHEN 'decide' THEN
   PERFORM 1 FROM teams WHERE id=t FOR UPDATE;
   SELECT * INTO c FROM conflicts WHERE id=(b->>'conflictId')::uuid AND vault_id=v AND status='pending' FOR UPDATE;
   IF c.id IS NULL THEN RAISE EXCEPTION 'Decision no longer pending'; END IF;
   PERFORM require_permission(v,coalesce((SELECT id FROM team_records WHERE id=c.record_id),nullif(c.candidate->>'parentId','')::uuid),'edit');
   IF (b->>'currentRevision')::bigint<>coalesce((SELECT tr.revision FROM team_records tr WHERE id=c.record_id),0) THEN RAISE EXCEPTION 'TEAM_RECORD_CHANGED: compare again' USING ERRCODE='40001'; END IF;
   IF b->>'choice'='server' THEN response=jsonb_build_object('status','discarded');
   ELSE
     IF b->'change'->>'recordId'<>c.record_id::text OR b->'change'->>'vaultId'<>v::text THEN RAISE EXCEPTION 'Decision record mismatch'; END IF;
     response=apply_change(b->'change');
     IF response->>'status'<>'applied' THEN RAISE EXCEPTION 'New comparison required' USING ERRCODE='40001'; END IF;
   END IF;
   UPDATE conflicts SET status=CASE WHEN b->>'choice'='server' THEN 'discarded' ELSE 'resolved' END,decided_by=u,decided_at=now(),decision_revision=(response->>'revision')::bigint WHERE id=c.id;
   PERFORM audit(t,c.record_id,'conflict.decided',jsonb_build_object('conflictId',c.id,'choice',b->>'choice')); RETURN response;
 WHEN 'audit' THEN
   IF NOT is_member(t,u) THEN RAISE EXCEPTION 'Membership required' USING ERRCODE='42501'; END IF;
   RETURN coalesce((SELECT jsonb_agg(x) FROM (SELECT a.* FROM audit_events a WHERE a.team_id=t AND (is_owner(t,u) OR (a.record_id IS NOT NULL AND EXISTS(SELECT FROM team_records tr WHERE tr.id=a.record_id AND allowed(u,tr.vault_id,tr.id,'read')))) ORDER BY a.id DESC LIMIT 200)x),'[]');
 WHEN 'activity' THEN
   PERFORM require_permission(v,r,'connect');
   IF b->>'event' NOT IN ('connection.opened','connection.closed','sftp.completed','sftp.failed') THEN RAISE EXCEPTION 'Unknown event'; END IF;
   PERFORM audit(t,r,b->>'event',jsonb_build_object('operationId',b->>'operationId','bytes',greatest(0,coalesce((b->>'bytes')::bigint,0)))); RETURN '{}';
 WHEN 'maintenance' THEN
   IF NOT is_owner(t,u) THEN RAISE EXCEPTION 'Owner required' USING ERRCODE='42501'; END IF;
   DELETE FROM audit_events WHERE id IN (SELECT a.id FROM audit_events a JOIN teams tm ON tm.id=a.team_id WHERE a.team_id=t AND a.server_at<now()-make_interval(days=>tm.audit_days) LIMIT 1000);
   RETURN jsonb_build_object('encryptedBytes',(SELECT coalesce(sum(octet_length(rv.payload)+octet_length(rv.secrets)),0) FROM record_versions rv JOIN team_records tr ON tr.id=rv.record_id JOIN team_vaults tv ON tv.id=tr.vault_id WHERE tv.team_id=t));
 ELSE RAISE EXCEPTION 'Unsupported Team operation';
 END CASE;
END $$;

CREATE OR REPLACE FUNCTION preview_acl(token text,b jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE result jsonb; v uuid=(b->>'vaultId')::uuid;
BEGIN
 BEGIN
   PERFORM rpc(token,'acl_set',b-'rewrap');
   SELECT coalesce(jsonb_agg(jsonb_build_object('id',tr.id,'recipients',recipients(v,tr.id,NULL))),'[]') INTO result FROM team_records tr WHERE tr.vault_id=v AND (allowed(actor(),v,tr.id,'edit') OR allowed(actor(),v,tr.id,'manage'));
   RAISE EXCEPTION USING ERRCODE='TT001',MESSAGE='rollback permission preview';
 EXCEPTION WHEN SQLSTATE 'TT001' THEN NULL;
 END;
 RETURN result;
END $$;

CREATE OR REPLACE FUNCTION preview_move(v uuid,r uuid,parent uuid) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE result jsonb; before jsonb; head team_records; t uuid;
BEGIN
 SELECT * INTO head FROM team_records WHERE id=r AND vault_id=v;
 IF head.id IS NULL THEN RAISE EXCEPTION 'Unknown record'; END IF;
 SELECT team_id INTO t FROM team_vaults WHERE id=v;
 PERFORM 1 FROM teams WHERE id=t FOR UPDATE;
 PERFORM require_permission(v,r,'manage'); PERFORM require_permission(v,parent,'manage');
 IF parent IS NOT NULL AND (NOT EXISTS(SELECT FROM team_records WHERE id=parent AND vault_id=v AND kind='group' AND NOT deleted) OR parent=r OR r IN(SELECT ancestors(v,parent))) THEN RAISE EXCEPTION 'Invalid destination folder'; END IF;
 SELECT coalesce(jsonb_agg(jsonb_build_object('id',tr.id,'recipients',recipients(v,tr.id,NULL))),'[]') INTO before FROM team_records tr WHERE tr.vault_id=v AND r IN(SELECT ancestors(v,tr.id));
 BEGIN
   UPDATE team_records SET parent_id=parent WHERE id=r;
   SELECT jsonb_build_object('aclRevision',(SELECT acl_revision FROM teams WHERE id=t),'recordRevision',head.revision,'before',before,'after',coalesce(jsonb_agg(jsonb_build_object('id',tr.id,'recipients',recipients(v,tr.id,NULL))),'[]')) INTO result FROM team_records tr WHERE tr.id IN(SELECT (x->>'id')::uuid FROM jsonb_array_elements(before)x);
   RAISE EXCEPTION USING ERRCODE='TT002',MESSAGE='rollback move preview';
 EXCEPTION WHEN SQLSTATE 'TT002' THEN NULL;
 END;
 RETURN result;
END $$;

-- A revoked/offline client can submit encrypted candidates but cannot read or mutate
-- shared records. Tokens remain bound to their original user and device.
CREATE OR REPLACE FUNCTION deliver_pending(token text,b jsonb) RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=termterm_team,public,pg_temp AS $$
DECLARE s auth_sessions;
BEGIN
 PERFORM set_config('termterm.actor','',true); PERFORM set_config('termterm.device','',true);
 SELECT * INTO s FROM auth_sessions WHERE delivery_hash=digest(token,'sha256') AND refresh_expires>now();
 IF s.id IS NULL THEN RAISE EXCEPTION 'Delivery token expired' USING ERRCODE='28000'; END IF;
 PERFORM set_config('termterm.actor',s.user_id::text,true); PERFORM set_config('termterm.device',s.device_id::text,true);
 IF b->>'event' IN ('connection.opened','connection.closed','sftp.completed','sftp.failed') THEN
   IF NOT EXISTS(SELECT FROM team_members m JOIN team_vaults v ON v.team_id=m.team_id JOIN team_records r ON r.vault_id=v.id WHERE m.user_id=s.user_id AND v.id=(b->>'vaultId')::uuid AND r.id=(b->>'recordId')::uuid) THEN RAISE EXCEPTION 'No previous membership'; END IF;
   PERFORM audit((SELECT team_id FROM team_vaults WHERE id=(b->>'vaultId')::uuid),(b->>'recordId')::uuid,b->>'event',jsonb_build_object('operationId',b->>'operationId','bytes',greatest(0,coalesce((b->>'bytes')::bigint,0)),'deliveredAfterRevocation',true,'clientAt',b->>'clientAt'));
   RETURN jsonb_build_object('status','received');
 END IF;
 RETURN apply_change(b,true);
END $$;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA termterm_team FROM PUBLIC;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA termterm_team TO termterm_team_executor;
ALTER FUNCTION rpc(text,text,jsonb) OWNER TO termterm_team_executor;
ALTER FUNCTION deliver_pending(text,jsonb) OWNER TO termterm_team_executor;
COMMIT;
