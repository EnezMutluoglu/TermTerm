"""Real PostgreSQL receipt and revoked-author delivery boundaries.
Synthetic encrypted payloads; Rust HTTPS test separately verifies encryption.
"""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,leader=bootstrap('offline delivery');t=b['teamId'];token=leader['accessToken']
    member=call('api_create_member',token,'editor','Offline editor','editor')
    activate(login(t,'editor',member['temporaryPassword']));editor=login(t,'editor',PW);auth=editor['accessToken']
    v=str(uuid.uuid4());r=str(uuid.uuid4());call('api_vault_create',token,v,blob(),1,blob())
    initial=call('api_resource_recipients',token,v,r,None)
    def body(preparation,sequence=1,expected=0):
        return dict(operationId=str(uuid.uuid4()),sequence=sequence,vaultId=v,resourceId=r,expectedRevision=expected,
            aclRevision=preparation['aclRevision'],writeCapability=preparation['writeCapability'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),
            kind='host',parentId=None,deleted=False,encryptedName='ab'*80,encryptedAddress='bc'*80,encryptedUsername=None,
            protocol='ssh',port=22,usernamePolicy='personal',snapshot='cd'*100,
            envelopes=[dict(accountId=x['accountId'],identityVersion=x['identityVersion'],envelope='de'*120) for x in preparation['recipients']])
    first=body(initial);call('api_resource_save',token,json.dumps(first))
    for permission in ['read','edit','connect']:
        sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,r,member['accountId'],permission,'allow']))+')',True)
    prepared=call('api_resource_recipients',auth,v,r,None)
    cap=prepared['writeCapability']
    assert call('api_operation_receipt',auth,first['operationId'])=={'found':False}
    assert call('api_operation_receipt',token,first['operationId'])['revision']==1
    assert call('api_operation_receipt',login(t,'leader',PW)['accessToken'],first['operationId'])=={'found':False}
    sql('UPDATE termterm_team_v2.resource_acl SET effect=\'deny\' WHERE team_id='+lit(t)+' AND account_id='+lit(member['accountId'])+'; UPDATE termterm_team_v2.teams SET acl_revision=acl_revision+1 WHERE id='+lit(t),True)
    denied(lambda:call('api_resource_get',auth,r))
    denied(lambda:call('api_resource_recipients',auth,v,r,None))
    denied(lambda:call('api_offline_decision_prepare',auth,str(uuid.uuid4()),r))
    denied(lambda:call('api_offline_decision_prepare',token,cap,r))
    denied(lambda:call('api_offline_decision_prepare',auth,cap,str(uuid.uuid4())))
    decision=call('api_offline_decision_prepare',auth,cap,r)
    assert {x['accountId'] for x in decision['recipients']}=={b['accountId']}
    assert set(decision)=={'recipients','writeCapability','aclRevision'}
    candidate=body(decision,1,1)
    # Old recipients, wrong capabilities, missing envelopes and tampered scope fail closed.
    denied(lambda:call('api_offline_decision_submit',auth,json.dumps(dict(candidate,envelopes=body(prepared)['envelopes']))))
    denied(lambda:call('api_offline_decision_submit',auth,json.dumps(dict(candidate,writeCapability=None))))
    denied(lambda:call('api_offline_decision_submit',auth,json.dumps(dict(candidate,envelopes=[]))))
    denied(lambda:call('api_offline_decision_submit',auth,json.dumps(dict(candidate,vaultId=str(uuid.uuid4())))))
    result=call('api_offline_decision_submit',auth,json.dumps(candidate))
    replay=call('api_offline_decision_submit',auth,json.dumps(candidate))
    assert result['status']=='conflict' and all(replay[k]==result[k] for k in ['operationId','status','conflictId'])
    assert replay.get('revision') is None and result.get('revision') is None
    denied(lambda:call('api_offline_decision_submit',auth,json.dumps(dict(candidate,snapshot='ef'*100))))
    assert call('api_operation_receipt',auth,candidate['operationId'])['conflictId']==result['conflictId']
    assert call('api_resource_get',token,r)['revision']==1
    denied(lambda:call('api_resource_get',auth,r))
    assert call('api_conflicts',auth,None,100)['conflicts']==[]
    queue=call('api_conflicts',token,None,100)['conflicts']
    assert len(queue)==1 and queue[0]['envelope'] and queue[0]['reason']=='permission'
    assert sql('SELECT count(*) FROM termterm_team_v2.change_operations WHERE team_id='+lit(t)+' AND id='+lit(candidate['operationId']),True)=='1'
    call('api_disable_member',token,member['accountId'])
    denied(lambda:call('api_offline_decision_prepare',auth,cap,r))
    denied(lambda:sql('SELECT * FROM termterm_team_v2.resource_edit_receipts'))
    print('PASS: private device receipts; revoked editor delivers current-recipient candidate only; no read/apply; replay, invalid scopes/envelopes and disabled account denied.')

if __name__=='__main__':main()
