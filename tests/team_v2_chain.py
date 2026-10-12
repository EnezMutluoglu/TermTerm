"""Real PostgreSQL chain authorization/CAS. Synthetic blobs are not crypto evidence."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,owner=bootstrap('chain');t=b['teamId'];token=owner['accessToken'];v=str(uuid.uuid4())
    account=call('api_create_member',token,'operator','Chain operator','operator')
    activate(login(t,'operator',account['temporaryPassword']));member=login(t,'operator',PW)['accessToken']
    call('api_vault_create',token,v,blob(),1,blob())
    seq=0
    def save(r,expected=0):
        nonlocal seq
        seq+=1;p=call('api_resource_recipients',token,v,r,None)
        body=dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=r,expectedRevision=expected,
            aclRevision=p['aclRevision'],writeCapability=p['writeCapability'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),
            kind='host',parentId=None,deleted=False,protocol='ssh',port=22,usernamePolicy='personal',encryptedName='ab'*80,
            encryptedAddress='ab'*80,encryptedUsername=None,snapshot='ab'*80,
            envelopes=[dict(accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='ab'*128) for a in p['recipients']])
        return call('api_resource_save',token,json.dumps(body))
    target,jump,jump2=[str(uuid.uuid4()) for _ in range(3)]
    for r in [target,jump,jump2]: assert save(r)['status']=='applied'
    sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,target,account['accountId'],'connect','allow']))+')',True)
    def prepare(hops): return call('api_chain_prepare',token,target,json.dumps(hops))
    def envelopes(p): return [dict(hopId=h['id'],accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='ab'*128) for h in p['hops'] for a in p['recipients']]
    denied(lambda:call('api_chain_prepare',member,target,json.dumps([jump])))
    denied(lambda:prepare([target]))
    denied(lambda:prepare([jump,jump]))
    denied(lambda:prepare([str(uuid.uuid4())]))
    p=prepare([jump,jump2]);e=envelopes(p)
    denied(lambda:call('api_chain_finish',token,p['planId'],json.dumps(e[:-1])))
    answer=call('api_chain_finish',token,p['planId'],json.dumps(e));assert answer['status']=='applied'
    assert call('api_chain_finish',token,p['planId'],json.dumps(e))==answer
    altered=[dict(x) for x in e];altered[0]['envelope']='cd'*128
    denied(lambda:call('api_chain_finish',token,p['planId'],json.dumps(altered)))
    result=call('api_connection_plan',member,target)
    assert [h['id'] for h in result['steps']]==[jump,jump2,target]
    assert 'dependencyEnvelope' in result['steps'][0] and 'envelope' not in result['steps'][0]
    denied(lambda:call('api_connection_plan',member,jump))
    denied(lambda:call('api_connection_target',member,jump))
    denied(lambda:call('api_resource_get',member,jump))
    denied(lambda:call('api_chain_configuration',member,target))
    call('api_profile_put',member,jump,0,'prompt',None,None)
    assert any(p['hostId']==jump for p in call('api_profiles',member)['hosts'])
    call('api_profile_put',member,jump,1,'keyboard_interactive',None,None)
    assert next(p for p in call('api_profiles',member)['hosts'] if p['hostId']==jump)['method']=='keyboard_interactive'
    denied(lambda:call('api_profile_put',member,str(uuid.uuid4()),0,'prompt',None,None))
    denied(lambda:call('api_connection_plan',member,jump))
    call('api_history_select',token,target,1)
    assert call('api_connection_plan',token,target)['steps'][-1]['temporaryRevision']==1
    call('api_history_select',token,target,None)
    assert call('api_chain_configuration',token,target)['hops']==[jump,jump2]
    p=prepare([jump]);save(jump,1)
    denied(lambda:call('api_chain_finish',token,p['planId'],json.dumps(envelopes(p))))
    denied(lambda:call('api_connection_plan',member,target)) # stale dependency ciphertext never used
    p=prepare([jump]);call('api_chain_finish',token,p['planId'],json.dumps(envelopes(p)))
    assert len(call('api_connection_plan',member,target)['steps'])==2
    abandoned=prepare([jump2])
    p=prepare([]);call('api_chain_finish',token,p['planId'],'[]')
    sql('UPDATE termterm_team_v2.connection_change_plans SET expires_at=now()-interval \'2 days\' WHERE team_id='+lit(t),True)
    assert call('api_maintenance',token)['chainPlansDeleted']>=1
    assert call('api_chain_finish',token,p['planId'],'[]')['status']=='applied'
    denied(lambda:call('api_chain_finish',token,abandoned['planId'],json.dumps(envelopes(abandoned))))
    assert len(call('api_connection_plan',member,target)['steps'])==1
    assert sql('SELECT count(*) FROM termterm_team_v2.connection_dependency_envelopes WHERE team_id='+lit(t),True)=='0'
    sql('DELETE FROM termterm_team_v2.resource_acl WHERE team_id='+lit(t)+' AND resource_id='+lit(target),True)
    denied(lambda:call('api_connection_plan',member,target))
    for table in ['connection_configurations','connection_dependency_envelopes','connection_change_plans']:
        denied(lambda table=table:sql('SELECT * FROM termterm_team_v2.'+table))
    denied(lambda:sql('SELECT termterm_team_v2.connection_manifest(NULL,NULL,NULL)'))
    print('PASS chain order, target-only dependency authority, stale keys/plans, replay, cancellation by clearing, RLS/function grants')

if __name__=='__main__':main()
