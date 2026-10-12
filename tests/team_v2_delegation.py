"""Real PostgreSQL scoped-manager boundaries; ciphertext is a disposable fixture."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,o=bootstrap('delegation');t=b['teamId'];owner=o['accessToken'];v=str(uuid.uuid4())
    def member(name,role):
        m=call('api_create_member',owner,name,name,role)
        activate(login(t,name,m['temporaryPassword']))
        return m['accountId'],login(t,name,PW)['accessToken']
    manager,mt=member('manager','manager');recipient,rt=member('recipient','operator')
    other_manager,ot=member('second.manager','manager')
    _,foreign=bootstrap('delegation foreign')
    call('api_vault_create',owner,v,blob(),1,blob());seq=0
    def save(r,kind='host',parent=None):
        nonlocal seq
        seq+=1;p=call('api_resource_recipients',owner,v,r,parent)
        d=dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=r,expectedRevision=0,
          aclRevision=p['aclRevision'],writeCapability=p['writeCapability'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind=kind,parentId=parent,
          deleted=False,encryptedName='ab'*80,encryptedAddress='ab'*80,encryptedUsername=None,protocol='ssh',port=22,usernamePolicy='personal',snapshot='cd'*100,
          envelopes=[dict(accountId=x['accountId'],identityVersion=x['identityVersion'],envelope='de'*120) for x in p['recipients']])
        assert call('api_resource_save',owner,json.dumps(d))['status']=='applied'
    outer,folder,host,hidden=[str(uuid.uuid4()) for _ in range(4)]
    save(outer,'folder');save(folder,'folder',outer);save(host,parent=folder);save(hidden)
    def prepare(who,target,scope,permissions=('read','connect'),effect='allow'):
        return call('api_acl_prepare',who,target,scope,json.dumps([dict(permission=p,effect=effect) for p in permissions]))
    def stage(who,p):
        cursor=0
        while True:
            page=call('api_acl_keys',who,p['id'],cursor)
            if not page['keys']:break
            call('api_acl_stage',who,p['id'],json.dumps([dict(ordinal=r['ordinal'],envelope='ef'*120) for r in page['keys']]))
            cursor=page['nextAfter']
    def grant(who,target,scope,permissions=('read','connect'),effect='allow'):
        p=prepare(who,target,scope,permissions,effect);stage(who,p)
        return call('api_acl_finish',who,p['id'],False)
    def delegate(target,scope,permissions,revision=None,who=owner):
        rev=call('api_delegations',owner)['aclRevision'] if revision is None else revision
        return call('api_delegation_set',who,target,scope,json.dumps(permissions),rev)
    denied(lambda:delegate(manager,folder,['read'])) # access must precede delegation
    grant(owner,manager,folder,('read','connect','edit'))
    denied(lambda:prepare(mt,recipient,folder)) # role alone grants nothing
    initial=call('api_delegations',owner)['aclRevision']
    delegate(manager,folder,['read','connect'])
    denied(lambda:delegate(manager,folder,[],initial)) # stale owner screen
    denied(lambda:delegate(recipient,folder,['read'])) # no role escalation
    denied(lambda:delegate(manager,folder,['manage']))
    denied(lambda:delegate(manager,folder,['read','read']))
    for who in [mt,rt,foreign['accessToken']]:
        denied(lambda:delegate(other_manager,folder,['read'],who=who))
    assert len(call('api_delegations',mt)['delegations'])==1
    assert call('api_delegations',rt)['delegations']==[]
    assert call('api_delegations',foreign['accessToken'])['delegations']==[]
    assert any(m['accountId']==recipient for m in call('api_members',mt))
    denied(lambda:call('api_create_member',mt,'attack','attack','manager'))
    denied(lambda:call('api_reset_member',mt,recipient))
    denied(lambda:call('api_disable_member',mt,recipient))
    for target,scope,permissions in [(manager,folder,('read',)),(b['accountId'],folder,('read',)),(recipient,hidden,('read',)),(recipient,outer,('read',)),(recipient,folder,('edit',))]:
        denied(lambda:prepare(mt,target,scope,permissions))
    grant(mt,recipient,host)
    assert call('api_resource_get',rt,host)['envelope']
    visible=call('api_resources',rt,v,None,100)['records']
    assert {x['id'] for x in visible}=={outer,folder,host}
    assert all(x['navigationOnly'] for x in visible if x['id']!=host)
    grant(mt,recipient,host,('read','connect'),'deny')
    denied(lambda:call('api_resource_get',rt,host))
    grant(mt,recipient,host,('read','connect'),'clear')
    grant(mt,recipient,folder)
    new=str(uuid.uuid4());save(new,parent=folder)
    assert call('api_resource_get',rt,new)['envelope']
    # Manager child deny cannot be bypassed by granting or clearing a parent rule.
    grant(owner,manager,host,('connect',),'deny')
    denied(lambda:prepare(mt,other_manager,folder))
    denied(lambda:prepare(mt,recipient,folder,('connect',),'clear'))
    grant(owner,manager,host,('connect',),'clear')
    p=prepare(mt,other_manager,folder);stage(mt,p)
    # Revocation after key staging cancels authority at every remaining API boundary.
    delegate(manager,folder,[])
    for fn,args in [('api_acl_keys',(0,)),('api_acl_impact',(None,)),('api_acl_finish',(False,))]:
        denied(lambda:call(fn,mt,p['id'],*args))
    denied(lambda:call('api_resource_get',ot,host))
    delegate(manager,folder,['read','connect'])
    denied(lambda:call('api_acl_finish',mt,p['id'],False)) # old revision stays stale
    call('api_acl_finish',mt,p['id'],True)
    assert sql("SELECT count(*) FROM termterm_team_v2.audit_events WHERE team_id="+lit(t)+" AND action='delegation.set'",True)=='3'
    for table in ['acl_delegations','delegation_audit_rules']:
        denied(lambda:sql('SELECT * FROM termterm_team_v2.'+table))
    print('PASS: delegation CAS/audit, manager scope and child-deny boundaries, no self/escalation/cross-team access, staged revocation, inherited future hosts and navigation-only parents.')
if __name__=='__main__':main()
