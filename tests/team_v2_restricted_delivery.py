"""Disabled accounts can hand in old drafts, never establish a session/read/apply."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,owner=bootstrap('restricted delivery');t=b['teamId'];token=owner['accessToken'];v=str(uuid.uuid4());r=str(uuid.uuid4())
    member=call('api_create_member',token,'member','Disabled draft author','editor')
    activate(login(t,'member',member['temporaryPassword']));auth=login(t,'member',PW)['accessToken']
    call('api_vault_create',token,v,blob(),1,blob());p=call('api_resource_recipients',token,v,r,None)
    def body(prep,resource=r,parent=None,seq=1,expected=0,kind='folder'):
        return dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=resource,expectedRevision=expected,
            aclRevision=prep['aclRevision'],writeCapability=prep['writeCapability'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),
            kind=kind,parentId=parent,deleted=False,encryptedName='ab'*80,encryptedAddress='bc'*80,encryptedUsername=None,protocol='ssh',port=22,usernamePolicy='personal',snapshot='cd'*100,
            envelopes=[dict(accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='ab'*128) for a in prep['recipients']])
    call('api_resource_save',token,json.dumps(body(p)))
    for permission in ['read','edit']:
        sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,r,member['accountId'],permission,'allow']))+')',True)
    cap=call('api_resource_recipients',auth,v,r,None)['writeCapability']
    scope=call('api_creation_scope',auth,v,r)['capability']
    proof=call('api_delivery_token',auth)['proof'];other=call('api_delivery_token',token)['proof']
    assert len(proof)==64
    denied(lambda:call('api_me',proof));denied(lambda:call('api_delivery_token',proof))
    denied(lambda:call('candidate_actor',proof))
    call('api_disable_member',token,member['accountId'])
    denied(lambda:call('api_me',auth));denied(lambda:call('api_resource_get',proof,r))
    denied(lambda:call('api_private_credentials',proof));denied(lambda:call('api_conflicts',proof,None,100))
    denied(lambda:call('api_resource_recipients',proof,v,r,None))
    denied(lambda:call('api_offline_decision_prepare',other,cap,r))
    prep=call('api_offline_decision_prepare',proof,cap,r)
    assert {a['accountId'] for a in prep['recipients']}=={b['accountId']}
    candidate=body(prep,expected=1)
    result=call('api_offline_decision_submit',proof,json.dumps(candidate))
    assert call('api_offline_decision_submit',proof,json.dumps(candidate))['conflictId']==result['conflictId']
    denied(lambda:call('api_resource_save',proof,json.dumps(candidate)))
    assert call('api_operation_receipt',proof,candidate['operationId'])['status']=='conflict'
    assert call('api_operation_receipt',other,candidate['operationId'])=={'found':False}
    assert call('api_resource_get',token,r)['revision']==1
    call('api_conflict_decide',token,result['conflictId'],1,'server',None)
    assert call('api_operation_receipt',proof,candidate['operationId'])['decisionStatus']=='resolved'
    new=str(uuid.uuid4());receipt=call('api_creation_claim',proof,scope,new,v,r)['writeCapability']
    prep=call('api_offline_decision_prepare',proof,receipt,new);candidate=body(prep,new,r,2,0,'host')
    result=call('api_offline_decision_submit',proof,json.dumps(candidate))
    assert call('api_conflict_current',token,result['conflictId'])['revision']==0
    denied(lambda:call('api_conflict_current',proof,result['conflictId']))
    call('api_conflict_decide',token,result['conflictId'],0,'server',None)
    assert call('api_operation_receipt',proof,candidate['operationId'])['decisionStatus']=='rejected'
    assert sql('SELECT count(*) FROM termterm_team_v2.resources WHERE team_id='+lit(t)+' AND id='+lit(new),True)=='0'
    assert sql('SELECT count(*) FROM termterm_team_v2.offline_delivery_tokens WHERE encode(token_hash,\'hex\')='+lit(proof),True)=='0'
    denied(lambda:sql('SELECT * FROM termterm_team_v2.offline_delivery_tokens'))
    sql('UPDATE termterm_team_v2.offline_delivery_tokens SET expires_at=now()-interval \'1 second\' WHERE team_id='+lit(t)+' AND account_id='+lit(member['accountId']),True)
    denied(lambda:call('api_operation_receipt',proof,candidate['operationId']))
    print('PASS: disabled-account proof only delivers prior scoped drafts/receipts; no session/read/apply/admin; owner decisions, hashed storage and expiry.')
if __name__=='__main__':main()
