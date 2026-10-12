"""Real PostgreSQL shared credential policy and atomic-copy tests; synthetic ciphertext."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,o=bootstrap('sharing');t=b['teamId'];owner=o['accessToken'];v=str(uuid.uuid4())
    def member(name):
        m=call('api_create_member',owner,name,name,'editor');activate(login(t,name,m['temporaryPassword']))
        return m['accountId'],login(t,name,PW)['accessToken']
    creator,ct=member('creator');receiver,rt=member('receiver');stranger,st=member('stranger')
    _,foreign=bootstrap('sharing foreign')
    rk=call('api_recovery_key_set',owner,PW,blob(32))['keyId']
    call('api_vault_create',owner,v,blob(),1,blob());seq=0
    def host():
        nonlocal seq
        seq+=1;r=str(uuid.uuid4());p=call('api_resource_recipients',owner,v,r,None)
        d=dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=r,expectedRevision=0,aclRevision=p['aclRevision'],writeCapability=p['writeCapability'],
          clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind='host',parentId=None,deleted=False,encryptedName='ab'*80,encryptedAddress='bc'*80,
          encryptedUsername=None,protocol='ssh',port=22,usernamePolicy='personal',snapshot='cd'*100,
          envelopes=[dict(accountId=x['accountId'],identityVersion=x['identityVersion'],envelope='de'*120) for x in p['recipients']])
        call('api_resource_save',owner,json.dumps(d));return r
    target,other=host(),host()
    def access(account,scope,effect='allow'):
        p=call('api_acl_prepare',owner,account,scope,json.dumps([dict(permission=p,effect=effect) for p in ['read','connect','edit']]))
        page=call('api_acl_keys',owner,p['id'],0)
        if page['keys']:call('api_acl_stage',owner,p['id'],json.dumps([dict(ordinal=x['ordinal'],envelope='ee'*120) for x in page['keys']]))
        call('api_acl_finish',owner,p['id'],False)
    for account in [creator,receiver]:
        access(account,target);access(account,other)
    private=str(uuid.uuid4())
    def private_save(rev):return call('api_private_credential_put',ct,private,rev,'ssh_key',blob(),blob(),1,rk,blob(),blob(),blob(),blob())
    private_save(0)
    rec=lambda account,**flags:dict(accountId=account,connect=flags.get('connect',True),reveal=flags.get('reveal',False),export=flags.get('export',False))
    def prepare(recipients=None):return call('api_share_prepare',ct,private,target,json.dumps(recipients if recipients is not None else [rec(receiver)]))
    denied(lambda:call('api_share_prepare',owner,private,target,json.dumps([rec(receiver)])))
    denied(lambda:prepare([rec(stranger)]))
    denied(lambda:prepare([rec(receiver),rec(receiver)]))
    p=prepare();cid=p['credentialId']
    assert {r['accountId'] for r in p['recipients']}=={creator,receiver,b['accountId']}
    assert not next(r for r in p['recipients'] if r['accountId']==receiver)['reveal']
    assert call('api_shared_credentials',rt)==[]
    denied(lambda:call('api_share_plan',rt,p['id'],False))
    body=dict(encryptedData='ab'*90,encryptedSecret='cd'*110,recoveryData='ef'*120,recoverySecret='fa'*120,
        envelopes=[dict(accountId=r['accountId'],identityVersion=r['identityVersion'],data='ab'*120,secret='bc'*120) for r in p['recipients']])
    denied(lambda:call('api_share_finish',ct,p['id'],json.dumps(dict(body,envelopes=body['envelopes'][:-1]))))
    assert call('api_shared_credentials',rt)==[]
    result=call('api_share_finish',ct,p['id'],json.dumps(body));assert result['credentialId']==cid
    assert call('api_share_finish',ct,p['id'],json.dumps(body))==result
    denied(lambda:call('api_share_finish',ct,p['id'],json.dumps(dict(body,encryptedSecret='ab'*110))))
    assert call('api_shared_credentials',rt)[0]['permissions']==['connect']
    assert call('api_shared_credentials',st)==[]
    assert call('api_shared_credentials',foreign['accessToken'])==[]
    assert call('api_shared_credential_get',rt,cid,'connect',target)['encryptedSecret']==body['encryptedSecret']
    denied(lambda:call('api_shared_credential_get',rt,cid,'connect',other))
    denied(lambda:call('api_shared_credential_get',rt,cid,'reveal',None))
    denied(lambda:call('api_shared_credential_get',rt,cid,'export',None))
    denied(lambda:call('api_shared_credential_get',rt,cid,'connect',None))
    denied(lambda:call('api_private_credential_get',owner,private))
    assert call('api_shared_credential_get',owner,cid,'reveal',None)['encryptedSecret']==body['encryptedSecret']
    private_save(1)
    assert call('api_shared_credential_get',rt,cid,'connect',target)['revision']==1
    # Update an independent copy in place; current profiles keep its stable id.
    up=prepare();updated=call('api_share_finish',ct,up['id'],json.dumps(body))['credentialId']
    def update_prepare(rev,recipients=None,source=None):
        return call('api_shared_update_prepare',ct,updated,rev,source,json.dumps(recipients if recipients is not None else [rec(receiver)]))
    def update_body(p):
        return dict(body,envelopes=[dict(accountId=r['accountId'],identityVersion=r['identityVersion'],data='ab'*120,secret='bc'*120) for r in p['recipients']])
    assert next(x for x in call('api_shared_credentials',ct) if x['id']==updated)['canUpdate']
    denied(lambda:call('api_shared_update_options',rt,updated))
    denied(lambda:call('api_shared_update_options',foreign['accessToken'],updated))
    denied(lambda:call('api_shared_update_prepare',rt,updated,1,None,json.dumps([])))
    denied(lambda:update_prepare(2))
    denied(lambda:update_prepare(1,[rec(stranger)]))
    p=update_prepare(1,[rec(receiver,reveal=True)])
    denied(lambda:call('api_shared_update_finish',ct,p['id'],json.dumps(dict(update_body(p),envelopes=[]))))
    assert call('api_shared_credential_get',rt,updated,'connect',target)['revision']==1
    changed=call('api_shared_update_finish',ct,p['id'],json.dumps(update_body(p)))
    assert changed==dict(credentialId=updated,revision=2)
    assert call('api_shared_update_finish',ct,p['id'],json.dumps(update_body(p)))==changed
    assert call('api_shared_update_plan',ct,p['id'],False)['status']=='applied'
    denied(lambda:call('api_shared_update_finish',ct,p['id'],json.dumps(dict(update_body(p),encryptedSecret='bb'*100))))
    assert call('api_shared_credential_get',rt,updated,'reveal',None)['revision']==2
    # Delegated managers may not elevate secret/export rights; revoking delegation invalidates plans.
    sql("UPDATE termterm_team_v2.team_memberships SET role='manager' WHERE team_id="+lit(t)+' AND account_id='+lit(receiver),True)
    delegation=call('api_delegations',owner)
    call('api_delegation_set',owner,receiver,target,json.dumps(['connect','edit']),delegation['aclRevision'])
    assert next(x for x in call('api_shared_credentials',rt) if x['id']==updated)['canUpdate']
    denied(lambda:call('api_shared_update_prepare',rt,updated,2,None,json.dumps([rec(receiver,reveal=True,export=True)])))
    managed=call('api_shared_update_prepare',rt,updated,2,None,json.dumps([rec(receiver,reveal=True)]))
    call('api_delegation_set',owner,receiver,target,'[]',call('api_delegations',owner)['aclRevision'])
    denied(lambda:call('api_shared_update_finish',rt,managed['id'],json.dumps(update_body(managed))))
    stale=update_prepare(2);p=update_prepare(2,[],private)
    call('api_shared_update_finish',ct,p['id'],json.dumps(update_body(p)))
    denied(lambda:call('api_shared_update_finish',ct,stale['id'],json.dumps(update_body(stale))))
    denied(lambda:call('api_shared_credential_get',rt,updated,'connect',target))
    assert sql('SELECT count(*) FROM termterm_team_v2.credential_key_envelopes WHERE team_id='+lit(t)+' AND credential_id='+lit(updated)+' AND account_id='+lit(receiver),True)=='0'
    assert call('api_private_credential_get',ct,private)['revision']==2
    for rev in range(3,14):
        p=update_prepare(rev,[]);call('api_shared_update_finish',ct,p['id'],json.dumps(update_body(p)))
    assert sql('SELECT count(*) FROM termterm_team_v2.credential_versions WHERE team_id='+lit(t)+' AND credential_id='+lit(updated),True)=='11'
    p=update_prepare(14);call('api_shared_update_plan',ct,p['id'],True)
    denied(lambda:call('api_shared_update_finish',ct,p['id'],json.dumps(update_body(p))))
    sql("UPDATE termterm_team_v2.credential_update_plans SET expires_at=now()-interval '2 days' WHERE team_id="+lit(t),True)
    assert call('api_maintenance',owner)['updatePlansDeleted']>=1
    assert sql("SELECT count(*) FROM termterm_team_v2.credential_update_plans WHERE team_id="+lit(t)+" AND status='applied'",True)=='13'
    for table in ['credential_update_plans','credential_update_recipients']:
        denied(lambda:sql('SELECT * FROM termterm_team_v2.'+table))
    call('api_profile_put',rt,target,0,'credential',cid,None)
    assert call('api_connection_target',rt,target)['id']==target
    denied(lambda:call('api_shared_credential_remove',rt,cid,1))
    denied(lambda:call('api_shared_credential_remove',ct,cid,2))
    call('api_shared_credential_remove',ct,cid,1)
    assert call('api_shared_credentials',rt)==[]
    denied(lambda:call('api_connection_target',rt,target))
    assert call('api_private_credential_get',ct,private)['revision']==2
    # Stale source, ACL, recipient identity and cancellation do not create a partial copy.
    p=prepare();private_save(2)
    denied(lambda:call('api_share_finish',ct,p['id'],json.dumps(body)))
    call('api_share_plan',ct,p['id'],True)
    denied(lambda:call('api_share_plan',ct,p['id'],False))
    p=prepare();access(receiver,target,'deny')
    denied(lambda:call('api_share_finish',ct,p['id'],json.dumps(body)))
    access(receiver,target)
    p=prepare([rec(receiver,reveal=True,export=True)])
    reset=call('api_reset_member',owner,receiver);activate(login(t,'receiver',reset['temporaryPassword']))
    denied(lambda:call('api_share_finish',ct,p['id'],json.dumps(body)))
    for table in ['credential_share_plans','credential_share_recipients','shared_credential_scopes']:
        denied(lambda:sql('SELECT * FROM termterm_team_v2.'+table))
    # Company-key recovery is owner-only, audited, and uses current recipients.
    p=prepare();fresh_body={**body,'envelopes':[dict(accountId=r['accountId'],identityVersion=r['identityVersion'],data='ab'*120,secret='bc'*120) for r in p['recipients']]}
    recover_id=call('api_share_finish',ct,p['id'],json.dumps(fresh_body))['credentialId']
    denied(lambda:call('api_shared_recovery_begin',ct,recover_id,PW,blob(32),'Creator is not company leader'))
    denied(lambda:call('api_shared_recovery_begin',foreign['accessToken'],recover_id,PW,blob(32),'Different company recovery'))
    denied(lambda:call('api_shared_recovery_begin',owner,recover_id,PW,chr(92)+'x'+'cc'*32,'Wrong company key'))
    access(receiver,target,'deny')
    recovered=call('api_shared_recovery_begin',owner,recover_id,PW,blob(32),'Approved shared credential recovery')
    assert receiver not in {r['accountId'] for r in recovered['plan']['recipients']}
    assert recovered['record']['revision']==1 and len(recovered['record']['envelopes'])==2
    restore_body=update_body(recovered['plan'])
    result=call('api_shared_update_finish',owner,recovered['plan']['id'],json.dumps(restore_body));assert result['revision']==2
    assert call('api_shared_update_finish',owner,recovered['plan']['id'],json.dumps(restore_body))==result
    denied(lambda:sql('SELECT * FROM termterm_team_v2.shared_recovery_plans'))
    print('PASS: private-copy isolation, explicit recipient permissions, reveal/export denial, target scope, atomic/replay/tamper, stale source/ACL/identity, cancellation and unshare transport revocation.')
if __name__=='__main__':main()
