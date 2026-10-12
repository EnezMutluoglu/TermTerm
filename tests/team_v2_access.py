"""Owner ACL preview/staging/commit against real PostgreSQL; no SSH claim."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob
from team_v2_concurrency import together

def main():
    b,owner=bootstrap('access');t=b['teamId'];token=owner['accessToken'];v=str(uuid.uuid4())
    m=call('api_create_member',token,'reader','Reader','operator');activate(login(t,'reader',m['temporaryPassword']));reader=login(t,'reader',PW)['accessToken']
    _,outsider=bootstrap('access outsider')
    call('api_vault_create',token,v,blob(),1,blob())
    seq=0
    def save(r,kind='host',parent=None,revision=0):
        nonlocal seq
        seq+=1;p=call('api_resource_recipients',token,v,r,parent)
        d=dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=r,expectedRevision=revision,
          aclRevision=p['aclRevision'],writeCapability=p['writeCapability'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind=kind,parentId=parent,
          deleted=False,encryptedName='ab'*80,encryptedAddress='ab'*80,encryptedUsername=None,protocol='ssh',port=22,usernamePolicy='personal',snapshot='cd'*100,
          envelopes=[dict(accountId=x['accountId'],identityVersion=x['identityVersion'],envelope='de'*120) for x in p['recipients']])
        return call('api_resource_save',token,json.dumps(d))
    folder,hidden=[str(uuid.uuid4()) for _ in range(2)]
    save(folder,'folder');save(hidden)
    hosts=[str(uuid.uuid4()) for _ in range(22)]
    for host in hosts:save(host,parent=folder)
    save(hosts[0],parent=folder,revision=1)
    rules=lambda effect:json.dumps([dict(permission=p,effect=effect) for p in ['read','connect']])
    def prepare(scope=folder,effect='allow'):
        return call('api_acl_prepare',token,m['accountId'],scope,rules(effect))
    def stage(plan):
        cursor=0;count=0;pages=0
        while True:
            page=call('api_acl_keys',token,plan,cursor);rows=page['keys'];assert len(rows)<=20
            assert page['targetAccountId']==m['accountId']
            if not rows:break
            items=json.dumps([dict(ordinal=r['ordinal'],envelope='ef'*120) for r in rows])
            assert call('api_acl_stage',token,plan,items)['staged']==len(rows)
            count+=len(rows);pages+=1;cursor=page['nextAfter']
        return count,pages
    for who in [reader,outsider['accessToken']]:
        denied(lambda:call('api_acl_prepare',who,m['accountId'],folder,rules('allow')))
    for invalid in ['[]','[{"permission":"manage","effect":"allow"}]','[{"permission":"read","effect":"bad"}]',
      '[{"permission":"read","effect":"allow"},{"permission":"read","effect":"deny"}]']:
        denied(lambda:call('api_acl_prepare',token,m['accountId'],folder,invalid))
    denied(lambda:call('api_acl_prepare',token,b['accountId'],folder,rules('deny')))
    p=prepare();plan=p['id'];assert p['affectedResources']==23 and p['requiredKeys']==25
    changes=call('api_acl_impact',token,plan,None);assert len(changes['records'])==20
    assert all(x['before']==[] and x['after']==['read','connect'] for x in changes['records'])
    rest=call('api_acl_impact',token,plan,changes['nextAfter']);assert len(rest['records'])==3
    assert hidden not in {x['id'] for x in changes['records']+rest['records']}
    # Partial batches do not grant access; incomplete/invalid commits leave ACL untouched.
    for who in [reader,outsider['accessToken']]:
        denied(lambda:call('api_acl_impact',who,plan,None))
        denied(lambda:call('api_acl_keys',who,plan,0))
        denied(lambda:call('api_acl_finish',who,plan,False))
    denied(lambda:call('api_acl_finish',token,plan,False))
    denied(lambda:call('api_acl_stage',token,plan,json.dumps([dict(ordinal=1,envelope='ef'*120),dict(ordinal=99999,envelope='ef'*120)])))
    assert not call('api_acl_keys',token,plan,0)['keys'][0]['staged']
    assert call('api_vaults',reader)==[]
    assert stage(plan)==(25,2)
    denied(lambda:call('api_resource_get',reader,hosts[0]))
    result,repeat=together(lambda:call('api_acl_finish',token,plan,False),lambda:call('api_acl_finish',token,plan,False))
    assert result==repeat and result['status']=='applied'
    assert call('api_acl_finish',token,plan,False)==result
    assert sql("SELECT count(*) FROM termterm_team_v2.audit_events WHERE team_id="+lit(t)+" AND subject_id="+lit(m['accountId'])+" AND action='acl.apply'",True)=='1'
    assert sql("SELECT count(*) FROM termterm_team_v2.acl_audit_rules r JOIN termterm_team_v2.audit_events a ON a.id=r.audit_id WHERE a.team_id="+lit(t)+" AND r.effect='allow'",True)=='2'
    assert len(call('api_vaults',reader))==1 and call('api_vaults',reader)[0]['envelope']
    assert call('api_resource_get',reader,hosts[0])['envelope']
    assert len(call('api_resource_history',reader,hosts[0]))==2
    assert all(x['envelope'] for x in call('api_resource_history',reader,hosts[0]))
    denied(lambda:call('api_resource_get',reader,hidden))
    # Folder permission covers future children; individual deny wins over parent allow.
    new=str(uuid.uuid4());save(new,parent=folder);assert call('api_resource_get',reader,new)['envelope']
    p=prepare(hosts[0],'deny');stage(p['id']);call('api_acl_finish',token,p['id'],False)
    denied(lambda:call('api_resource_get',reader,hosts[0]))
    assert sql('SELECT count(*) FROM termterm_team_v2.resource_key_envelopes WHERE team_id='+lit(t)+' AND account_id='+lit(m['accountId'])+' AND resource_id='+lit(hosts[0]),True)=='0'
    p=prepare(hosts[0],'clear');assert p['requiredKeys']==2;stage(p['id']);call('api_acl_finish',token,p['id'],False)
    assert call('api_resource_get',reader,hosts[0])['envelope']
    # Source revision, ACL, expiry and recipient key rotation each invalidate prepared commits.
    p=prepare(hidden);stage(p['id']);save(hidden,revision=1)
    denied(lambda:call('api_acl_impact',token,p['id'],None))
    denied(lambda:call('api_acl_finish',token,p['id'],False));denied(lambda:call('api_resource_get',reader,hidden))
    call('api_acl_finish',token,p['id'],True)
    p=prepare(hidden);stage(p['id'])
    q=prepare(hosts[0],'deny');stage(q['id']);call('api_acl_finish',token,q['id'],False)
    denied(lambda:call('api_acl_finish',token,p['id'],False));call('api_acl_finish',token,p['id'],True)
    p=prepare(hidden);stage(p['id']);sql('UPDATE termterm_team_v2.acl_change_plans SET expires_at=now()-interval \'1 second\' WHERE team_id='+lit(t)+' AND id='+lit(p['id']),True)
    denied(lambda:call('api_acl_finish',token,p['id'],False));call('api_acl_finish',token,p['id'],True)
    # Two different approved previews cannot silently overwrite each other's ACL baseline.
    left=prepare(hosts[1],'deny');right=prepare(hosts[2],'deny');stage(left['id']);stage(right['id'])
    def finishing(plan):
        try:return call('api_acl_finish',token,plan,False)['status']
        except RuntimeError as error:
            assert 'preview_changed' in str(error);return 'stale'
    outcomes=together(lambda:finishing(left['id']),lambda:finishing(right['id']))
    assert sorted(outcomes)==['applied','stale']
    for item,outcome in zip([left,right],outcomes):
        if outcome=='stale':call('api_acl_finish',token,item['id'],True)
    # Revoke all parent rights; no vault label or record keys remain accessible.
    p=prepare(folder,'deny');assert p['requiredKeys']==0;call('api_acl_finish',token,p['id'],False)
    assert call('api_vaults',reader)==[] and call('api_resources',reader,v,None,100)['records']==[]
    # Single-host access exposes only current ancestor folder names, not siblings/history.
    outer,inner,leaf,sibling=[str(uuid.uuid4()) for _ in range(4)]
    save(outer,'folder');save(outer,'folder',revision=1)
    save(inner,'folder',parent=outer);save(leaf,parent=inner);save(sibling,parent=inner)
    p=prepare(leaf);assert p['affectedResources']==1 and p['requiredKeys']==4
    stage(p['id']);call('api_acl_finish',token,p['id'],False)
    visible=call('api_resources',reader,v,None,100)['records']
    assert {x['id'] for x in visible}=={outer,inner,leaf}
    assert all(x['navigationOnly'] and x['permissions']==[] for x in visible if x['id'] in [outer,inner])
    assert not next(x for x in visible if x['id']==leaf)['navigationOnly']
    denied(lambda:call('api_resource_get',reader,outer));denied(lambda:call('api_resource_history',reader,outer))
    assert sql('SELECT count(*) FROM termterm_team_v2.resource_key_envelopes WHERE team_id='+lit(t)+' AND account_id='+lit(m['accountId'])+' AND resource_id='+lit(outer),True)=='1'
    save(outer,'folder',revision=2)
    assert next(x for x in call('api_resources',reader,v,None,100)['records'] if x['id']==outer)['envelope']
    p=prepare(leaf,'deny');stage(p['id']);call('api_acl_finish',token,p['id'],False)
    assert call('api_resources',reader,v,None,100)['records']==[]
    p=prepare(hidden);stage(p['id']);reset=call('api_reset_member',token,m['accountId'])
    activate(login(t,'reader',reset['temporaryPassword']))
    denied(lambda:call('api_acl_finish',token,p['id'],False))
    for table in ['acl_change_plans','acl_plan_keys','acl_audit_rules']:
        denied(lambda:sql('SELECT * FROM termterm_team_v2.'+table))
    print('PASS: owner-only ACL preview, paged atomic key staging, inherited/future access, explicit deny/clear, retained history, private scope, replay, cancellation, stale revision/ACL/expiry/identity and direct SQL denial.')
if __name__=='__main__':main()
