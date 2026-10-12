"""Disposable SQL move/ACL/history/chain boundaries. Real crypto lives in Rust."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,owner=bootstrap('move');t=b['teamId'];token=owner['accessToken'];v=str(uuid.uuid4())
    _,other=bootstrap('move outsider')
    people=[]
    for name in ['before','after','manager']:
        m=call('api_create_member',token,name,name,'manager' if name=='manager' else 'operator')
        activate(login(t,name,m['temporaryPassword']));people.append((m['accountId'],login(t,name,PW)['accessToken']))
    call('api_vault_create',token,v,blob(),1,blob());seq=0
    def draft(r,kind='host',parent=None,expected=0,prep=None):
        nonlocal seq
        p=prep or call('api_resource_recipients',token,v,r,parent);seq+=1
        return dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=r,expectedRevision=expected,
          aclRevision=p['aclRevision'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind=kind,parentId=parent,
          deleted=False,protocol='ssh',port=22,usernamePolicy='personal',encryptedName='ab'*80,encryptedAddress='bc'*80,
          encryptedUsername=None,snapshot='cd'*100,envelopes=[dict(accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='de'*128) for a in p['recipients']])
    def save(*args,**kw):return call('api_resource_save',token,json.dumps(draft(*args,**kw)))
    top,a,d,folder,host,jump=[str(uuid.uuid4()) for _ in range(6)]
    save(top,'folder');save(a,'folder',top);save(d,'folder',top)
    for account,scope in [(people[0][0],a),(people[1][0],d),(people[2][0],top)]:
        for perm in ['read','connect','edit']:
            sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,scope,account,perm,'allow']))+')',True)
    save(folder,'folder',a);save(host,parent=folder);save(jump)
    # Source history includes the oldest version that the move's new revision prunes.
    for revision in range(1,11):save(folder,'folder',a,revision)
    chain=call('api_chain_prepare',token,host,json.dumps([jump]))
    call('api_chain_finish',token,chain['planId'],json.dumps([dict(hopId=jump,accountId=x['accountId'],identityVersion=x['identityVersion'],envelope='ea'*128) for x in chain['recipients']]))
    denied(lambda:call('api_move_prepare',people[0][1],folder,d))
    denied(lambda:call('api_move_prepare',other['accessToken'],folder,d))
    denied(lambda:call('api_move_prepare',token,top,folder))
    denied(lambda:call('api_move_prepare',people[2][1],folder,d))
    p=call('api_move_prepare',token,folder,d);pid=p['planId']
    assert p['affectedAccess']>=4
    impact=call('api_move_impact',token,pid,0);assert impact['chainReferences']==1
    assert any(x['accountId']==people[0][0] and not x['after'] for x in impact['items'])
    assert any(x['accountId']==people[1][0] and not x['before'] for x in impact['items'])
    body=draft(folder,'folder',d,11,p)
    denied(lambda:call('api_move_finish',token,pid,json.dumps(body),False))
    assert call('api_resource_get',token,folder)['parentId']==a
    def stage(plan,actor=token):
        cursor=0
        while (batch:=call('api_move_keys',actor,plan,cursor))['keys']:
            items=[dict(ordinal=k['ordinal'],envelope='ef'*128) for k in batch['keys']]
            call('api_move_stage',actor,plan,json.dumps(items));cursor=batch['nextAfter']
    stage(pid)
    denied(lambda:call('api_move_keys',people[0][1],pid,0))
    result=call('api_move_finish',token,pid,json.dumps(body),False);assert result['revision']==12
    assert call('api_move_finish',token,pid,json.dumps(body),False)==result
    denied(lambda:call('api_move_finish',token,pid,json.dumps(dict(body,snapshot='ff'*100)),False))
    assert call('api_resource_get',token,folder)['parentId']==d
    denied(lambda:call('api_resource_get',people[0][1],host))
    assert call('api_resource_get',people[1][1],host)['envelope']
    assert len(call('api_resource_history',people[1][1],folder))==11
    assert call('api_connection_plan',people[1][1],host)['steps'][0]['dependencyEnvelope']
    denied(lambda:call('api_connection_plan',people[1][1],jump))
    # Moving a jump rewraps its new revision for dependent roots too.
    q=call('api_move_prepare',token,jump,d);stage(q['planId'])
    assert call('api_move_finish',token,q['planId'],json.dumps(draft(jump,parent=d,expected=1,prep=q)),False)['revision']==2
    assert call('api_connection_plan',people[1][1],host)['steps'][0]['revision']==2
    stale=call('api_move_prepare',token,folder,a);stage(stale['planId']);save(host,parent=folder,expected=1)
    denied(lambda:call('api_move_finish',token,stale['planId'],json.dumps(draft(folder,'folder',a,12,stale)),False))
    assert call('api_move_finish',token,stale['planId'],None,True)['status']=='cancelled'
    # Explicit common-scope delegation enables manager preview, but root remains forbidden.
    for perm in ['read','connect','edit']:
        sql('INSERT INTO termterm_team_v2.acl_delegations VALUES('+','.join(map(lit,[t,top,people[2][0],perm]))+')',True)
    manager=people[2][1]
    manager_plan=call('api_move_prepare',manager,folder,a)
    stage(manager_plan['planId'],manager)
    managed=call('api_move_finish',manager,manager_plan['planId'],json.dumps(draft(folder,'folder',a,12,manager_plan)),False)
    assert managed['revision']==13
    assert call('api_resource_get',token,folder)['parentId']==a
    # A permission recipient change invalidates reviewed material, even when
    # neither the source's revision nor the destination changes.
    revoked=call('api_move_prepare',token,folder,d);stage(revoked['planId'])
    sql('UPDATE termterm_team_v2.account_crypto_identities SET active=false WHERE team_id='+lit(t)+' AND account_id='+lit(people[1][0]),True)
    denied(lambda:call('api_move_finish',token,revoked['planId'],json.dumps(draft(folder,'folder',d,13,revoked)),False))
    assert call('api_resource_get',token,folder)['parentId']==a
    call('api_move_finish',token,revoked['planId'],None,True)
    sql('UPDATE termterm_team_v2.account_crypto_identities SET active=true WHERE team_id='+lit(t)+' AND account_id='+lit(people[1][0]),True)
    # Independent chain edits do not bump the folder's revision; the route hash
    # must still prevent publishing stale dependency envelopes.
    route=call('api_move_prepare',token,folder,d);stage(route['planId'])
    clear=call('api_chain_prepare',token,host,'[]');call('api_chain_finish',token,clear['planId'],'[]')
    denied(lambda:call('api_move_finish',token,route['planId'],json.dumps(draft(folder,'folder',d,13,route)),False))
    call('api_move_finish',token,route['planId'],None,True)
    denied(lambda:call('api_move_prepare',people[2][1],folder,None))
    for table in ['resource_move_plans','move_plan_keys','move_access_changes']:
        denied(lambda table=table:sql('SELECT * FROM termterm_team_v2.'+table))
    print('PASS: move preview, recipient loss/gain, cycle, scoped delegation, stale/replay, history pruning, chain rewrap and cross-team SQL isolation.')
if __name__=='__main__':main()
