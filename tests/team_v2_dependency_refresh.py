"""Real SQL atomic resource/ACL dependency refresh boundaries; ciphertext is synthetic."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,owner=bootstrap('dependency refresh');t=b['teamId'];token=owner['accessToken'];v=str(uuid.uuid4());seq=0
    m=call('api_create_member',token,'late','Late operator','operator');activate(login(t,'late',m['temporaryPassword']));member=login(t,'late',PW)['accessToken']
    call('api_vault_create',token,v,blob(),1,blob())
    def draft(r,expected=0):
        nonlocal seq
        p=call('api_resource_recipients',token,v,r,None);seq+=1
        return dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=r,expectedRevision=expected,aclRevision=p['aclRevision'],
          clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind='host',parentId=None,deleted=False,
          protocol='ssh',port=22,usernamePolicy='personal',encryptedName='ab'*80,encryptedAddress='bc'*80,encryptedUsername=None,snapshot='cd'*100,
          envelopes=[dict(accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='de'*128) for a in p['recipients']],
          dependencyEnvelopes=[dict(rootId=a['rootId'],accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='fe'*128) for a in p['dependencyRecipients']])
    target,jump=[str(uuid.uuid4()) for _ in range(2)]
    for r in [target,jump]:call('api_resource_save',token,json.dumps(draft(r)))
    p=call('api_chain_prepare',token,target,json.dumps([jump]))
    call('api_chain_finish',token,p['planId'],json.dumps([dict(hopId=jump,accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='ef'*128) for a in p['recipients']]))
    def prepare(effect):return call('api_acl_prepare',token,m['accountId'],target,json.dumps([dict(permission='read',effect=effect),dict(permission='connect',effect=effect)]))
    def stage(p):
        cursor=0;purposes=[]
        while (batch:=call('api_acl_keys',token,p['id'],cursor))['keys']:
            purposes += [k['kind'] for k in batch['keys']]
            call('api_acl_stage',token,p['id'],json.dumps([dict(ordinal=k['ordinal'],envelope='ef'*128) for k in batch['keys']]));cursor=batch['nextAfter']
        return purposes
    p=prepare('allow');assert 'dependency' in stage(p)
    call('api_acl_finish',token,p['id'],False)
    assert call('api_connection_plan',member,target)['steps'][0]['dependencyEnvelope']
    denied(lambda:call('api_connection_plan',member,jump))
    body=draft(jump,1);assert len(body['dependencyEnvelopes'])==2
    denied(lambda:call('api_resource_save',token,json.dumps(dict(body,dependencyEnvelopes=[]))))
    denied(lambda:call('api_resource_save',token,json.dumps(dict(body,dependencyEnvelopes=[body['dependencyEnvelopes'][0]]*2))))
    forged=[dict(e,rootId=str(uuid.uuid4())) for e in body['dependencyEnvelopes']]
    denied(lambda:call('api_resource_save',token,json.dumps(dict(body,dependencyEnvelopes=forged))))
    assert call('api_resource_get',token,jump)['revision']==1
    result=call('api_resource_save',token,json.dumps(body));assert result['revision']==2
    replay=call('api_resource_save',token,json.dumps(body))
    assert all(replay[k]==result[k] for k in ['operationId','status','revision'])
    assert len(call('api_resource_history',token,jump))==2
    assert call('api_connection_plan',member,target)['steps'][0]['revision']==2
    denied(lambda:call('api_resource_get',member,jump))
    # A route edit between ACL review and apply must not publish old envelopes.
    stale=prepare('deny');stage(stale)
    p=call('api_chain_prepare',token,target,'[]');call('api_chain_finish',token,p['planId'],'[]')
    denied(lambda:call('api_acl_finish',token,stale['id'],False));call('api_acl_finish',token,stale['id'],True)
    p=prepare('deny');stage(p);call('api_acl_finish',token,p['id'],False)
    denied(lambda:call('api_connection_plan',member,target))
    assert sql('SELECT count(*) FROM termterm_team_v2.connection_dependency_envelopes WHERE team_id='+lit(t)+' AND account_id='+lit(m['accountId']),True)=='0'
    denied(lambda:sql('SELECT termterm_team_v2.resource_dependency_recipients(NULL,NULL)'))
    print('PASS: ACL grant stages hop keys, jump write refreshes atomically, missing/duplicate/forged roots rejected, stale route preview and standalone access denied.')
if __name__=='__main__':main()
