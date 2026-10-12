"""Historical create scopes do not authorize current writes or reads."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,owner=bootstrap('offline creation');t=b['teamId'];token=owner['accessToken'];v=str(uuid.uuid4());folder=str(uuid.uuid4())
    member=call('api_create_member',token,'editor','Offline editor','editor')
    activate(login(t,'editor',member['temporaryPassword']));auth=login(t,'editor',PW)['accessToken']
    call('api_vault_create',token,v,blob(),1,blob())
    p=call('api_resource_recipients',token,v,folder,None)
    body=dict(operationId=str(uuid.uuid4()),sequence=1,vaultId=v,resourceId=folder,expectedRevision=0,aclRevision=p['aclRevision'],
        writeCapability=p['writeCapability'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind='folder',parentId=None,
        deleted=False,encryptedName='ab'*80,snapshot='cd'*100,envelopes=[dict(accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='ab'*128) for a in p['recipients']])
    call('api_resource_save',token,json.dumps(body))
    denied(lambda:call('api_creation_scope',auth,v,None))
    denied(lambda:call('api_creation_scope',auth,v,folder))
    sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,folder,member['accountId'],'edit','allow']))+')',True)
    scope=call('api_creation_scope',auth,v,folder)['capability'];r=str(uuid.uuid4())
    denied(lambda:call('api_creation_claim',token,scope,r,v,folder))
    denied(lambda:call('api_creation_claim',login(t,'editor',PW)['accessToken'],scope,r,v,folder))
    denied(lambda:call('api_creation_claim',auth,scope,r,v,None))
    denied(lambda:call('api_creation_claim',auth,scope,r,str(uuid.uuid4()),folder))
    denied(lambda:call('api_creation_claim',auth,scope,folder,v,folder))
    sql('UPDATE termterm_team_v2.resource_acl SET effect=\'deny\' WHERE team_id='+lit(t)+' AND account_id='+lit(member['accountId']),True)
    denied(lambda:call('api_creation_scope',auth,v,folder))
    receipt=call('api_creation_claim',auth,scope,r,v,folder)
    assert set(receipt)=={'writeCapability'}
    assert call('api_creation_claim',auth,scope,r,v,folder)==receipt
    denied(lambda:call('api_resource_recipients',auth,v,r,folder))
    denied(lambda:call('api_resource_get',auth,folder))
    decision=call('api_offline_decision_prepare',auth,receipt['writeCapability'],r)
    assert {a['accountId'] for a in decision['recipients']}=={b['accountId']}
    assert sql('SELECT count(*) FROM termterm_team_v2.resources WHERE team_id='+lit(t)+' AND id='+lit(r),True)=='0'
    candidate=dict(body,resourceId=r,parentId=folder,kind='host',operationId=str(uuid.uuid4()),
        writeCapability=receipt['writeCapability'],aclRevision=decision['aclRevision'],protocol='ssh',port=22,usernamePolicy='personal',
        encryptedAddress='bc'*80,encryptedUsername=None,
        envelopes=[dict(accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='ab'*128) for a in decision['recipients']])
    denied(lambda:call('api_offline_decision_submit',auth,json.dumps(dict(candidate,parentId=None))))
    denied(lambda:call('api_offline_decision_submit',auth,json.dumps(dict(candidate,envelopes=[]))))
    result=call('api_offline_decision_submit',auth,json.dumps(candidate));d=result['conflictId']
    replay=call('api_offline_decision_submit',auth,json.dumps(candidate));assert replay['conflictId']==d
    denied(lambda:call('api_offline_decision_submit',auth,json.dumps(dict(candidate,snapshot='ef'*100))))
    assert call('api_operation_receipt',auth,candidate['operationId'])['decisionStatus']=='pending'
    assert call('api_conflicts',auth,None,100)['conflicts']==[]
    owner_rows=call('api_conflicts',token,None,100)['conflicts']
    assert len(owner_rows)==1 and owner_rows[0]['creation'] is True
    assert call('api_conflict_current',token,d)['revision']==0
    denied(lambda:call('api_conflict_current',auth,d))
    denied(lambda:call('api_conflict_decide',auth,d,0,'server',None))
    denied(lambda:call('api_conflict_decide',token,d,1,'server',None))
    assert sql('SELECT count(*) FROM termterm_team_v2.resources WHERE team_id='+lit(t)+' AND id='+lit(r),True)=='0'
    current=call('api_resource_recipients',token,v,r,folder)
    replacement=dict(candidate,operationId=str(uuid.uuid4()),sequence=2,writeCapability=current['writeCapability'],aclRevision=current['aclRevision'],
        envelopes=[dict(accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='ab'*128) for a in current['recipients']])
    assert call('api_conflict_decide',token,d,0,'replacement',json.dumps(replacement))['revision']==1
    assert call('api_resource_get',token,r)['revision']==1
    assert call('api_operation_receipt',auth,candidate['operationId'])['decisionStatus']=='resolved'
    assert call('api_conflicts',token,None,100)['conflicts']==[]
    denied(lambda:call('api_resource_get',auth,r))
    assert call('api_offline_decision_submit',auth,json.dumps(candidate))['conflictId']==d
    denied(lambda:call('api_conflict_decide',token,d,1,'server',None))
    denied(lambda:sql('SELECT * FROM termterm_team_v2.creation_decisions'))
    denied(lambda:sql('SELECT * FROM termterm_team_v2.offline_creation_scopes'))
    denied(lambda:sql('SELECT * FROM termterm_team_v2.offline_creation_claims'))
    print('PASS: scoped offline create; revoked author candidate/replay without resource mutation; owner decision creates exactly once; current/revision/SQL boundaries.')
if __name__=='__main__':main()
