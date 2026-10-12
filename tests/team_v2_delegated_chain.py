"""Real PostgreSQL manager chain grants and revocation; synthetic key envelopes."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,owner=bootstrap('delegated chain');t=b['teamId'];token=owner['accessToken'];v=str(uuid.uuid4())
    m=call('api_create_member',token,'manager','Chain manager','manager')
    activate(login(t,'manager',m['temporaryPassword']));manager=login(t,'manager',PW)['accessToken']
    call('api_vault_create',token,v,blob(),1,blob());seq=0
    def save(r,kind='host',parent=None):
        nonlocal seq
        p=call('api_resource_recipients',token,v,r,parent);seq+=1
        body=dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=r,expectedRevision=0,aclRevision=p['aclRevision'],
            clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind=kind,parentId=parent,deleted=False,
            protocol='ssh',port=22,usernamePolicy='personal',encryptedName='ab'*80,encryptedAddress='bc'*80,
            encryptedUsername=None,snapshot='cd'*100,envelopes=[dict(accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='de'*128) for a in p['recipients']])
        return call('api_resource_save',token,json.dumps(body))
    top,target,jump,outside=[str(uuid.uuid4()) for _ in range(4)]
    save(top,'folder')
    for p in ['read','connect','edit']:
        sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,top,m['accountId'],p,'allow']))+')',True)
    save(target,parent=top);save(jump,parent=top);save(outside)
    denied(lambda:call('api_chain_prepare',manager,target,json.dumps([jump])))
    call('api_delegation_set',token,m['accountId'],top,json.dumps(['connect']),1)
    def prepare(hops):return call('api_chain_prepare',manager,target,json.dumps(hops))
    def envelopes(plan):return [dict(hopId=h['id'],accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='ef'*128) for h in plan['hops'] for a in plan['recipients']]
    p=prepare([jump]);answer=call('api_chain_finish',manager,p['planId'],json.dumps(envelopes(p)))
    assert answer['status']=='applied'
    assert call('api_chain_configuration',manager,target)['hops']==[jump]
    assert call('api_chain_finish',manager,p['planId'],json.dumps(envelopes(p)))==answer
    denied(lambda:prepare([outside]))
    denied(lambda:call('api_chain_configuration',manager,outside))
    stale=prepare([])
    revision=sql('SELECT acl_revision FROM termterm_team_v2.teams WHERE id='+lit(t),True)
    call('api_delegation_set',token,m['accountId'],top,'[]',revision)
    denied(lambda:call('api_chain_finish',manager,stale['planId'],'[]'))
    assert call('api_chain_configuration',token,target)['hops']==[jump]
    # Read/connect alone cannot grant or rewrite a route after delegation removal.
    denied(lambda:call('api_chain_configuration',manager,target))
    denied(lambda:sql('SELECT termterm_team_v2.require_chain_scope(NULL,NULL,NULL)'))
    print('PASS: scoped manager chain prepare/apply/replay, outside-hop denial, revoked delegation and private function isolation.')
if __name__=='__main__':main()
