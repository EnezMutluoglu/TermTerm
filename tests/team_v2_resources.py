"""Real PostgreSQL shared-resource CRUD, replay, CAS, recipients and history.
Synthetic ciphertext tests database boundaries; not a claim of SSH validation.
"""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob
from team_v2_concurrency import together
def main():
    b,owner=bootstrap('resources');t=b['teamId'];token=owner['accessToken'];v=str(uuid.uuid4())
    member=call('api_create_member',token,'editor','Editor','editor');activate(login(t,'editor',member['temporaryPassword']));editor=login(t,'editor',PW)
    other_b,other=bootstrap('resources outsider')
    call('api_vault_create',token,v,blob(),1,blob())
    assert len(call('api_vaults',token))==1
    assert call('api_vaults',editor['accessToken'])==[]
    denied(lambda:call('api_vault_create',editor['accessToken'],uuid.uuid4(),blob(),1,blob()))
    sequence={token:0,editor['accessToken']:0,other['accessToken']:0}
    def draft(id,kind='host',parent=None,expected=0,who=token,deleted=False):
        preparation=call('api_resource_recipients',who,v,id,parent)
        sequence[who]+=1
        return dict(operationId=str(uuid.uuid4()),sequence=sequence[who],vaultId=v,resourceId=id,expectedRevision=expected,
            aclRevision=preparation['aclRevision'],writeCapability=preparation['writeCapability'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind=kind,parentId=parent,
            deleted=deleted,encryptedName='ab'*80,encryptedAddress='bc'*80,encryptedUsername=None,protocol='ssh',port=22,usernamePolicy='personal',
            snapshot='cd'*100,envelopes=[dict(accountId=x['accountId'],identityVersion=x['identityVersion'],envelope='de'*120) for x in preparation['recipients']])
    def save(d,who=token):return call('api_resource_save',who,json.dumps(d))
    folder,hidden,host=[str(uuid.uuid4()) for _ in range(3)]
    assert save(draft(folder,'folder'))['status']=='applied'
    assert save(draft(hidden))['revision']==1
    # ACL administration is not yet exposed; isolated fixture grants simulate a leader grant.
    for permission in ['read','edit','connect']:
        sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,folder,member['accountId'],permission,'allow']))+')',True)
    d=draft(host,parent=folder,who=editor['accessToken']);assert len(d['envelopes'])==2
    result=save(d,editor['accessToken']);assert result['status']=='applied' and result['revision']==1
    assert save(d,editor['accessToken'])['revision']==1
    denied(lambda:save(dict(d,snapshot='ff'*100),editor['accessToken']))
    assert sql('SELECT count(*) FROM termterm_team_v2.change_operations WHERE team_id='+lit(t)+' AND resource_id='+lit(host),True)=='1'
    denied(lambda:call('api_resource_get',editor['accessToken'],hidden))
    denied(lambda:call('api_resource_get',other['accessToken'],host))
    denied(lambda:call('api_resource_recipients',other['accessToken'],v,host,folder))
    assert call('api_vaults',other['accessToken'])==[]
    rows=call('api_resources',editor['accessToken'],v,None,100)
    assert {x['id'] for x in rows['records']}=={folder,host} and 'total' not in rows
    page=call('api_resources',token,v,None,1);assert len(page['records'])==1 and page['nextAfter']
    next_page=call('api_resources',token,v,page['nextAfter'],100);assert not {x['id'] for x in next_page['records']} & {x['id'] for x in page['records']}
    denied(lambda:call('api_resources',token,v,None,101))
    # Envelope mismatch or invalid host fields must roll back the whole save and outbox receipt.
    wrong=draft(host,parent=folder,expected=1)
    denied(lambda:save(dict(wrong,envelopes=wrong['envelopes'][:1])))
    denied(lambda:save(dict(wrong,port=65536)))
    denied(lambda:save(dict(wrong,parentId=None)))
    assert call('api_resource_get',token,host)['revision']==1
    assert sql('SELECT count(*) FROM termterm_team_v2.change_operations WHERE team_id='+lit(t)+' AND id='+lit(wrong['operationId']),True)=='0'
    a=draft(host,parent=folder,expected=1);c=draft(host,parent=folder,expected=1,who=editor['accessToken'])
    results=together(lambda:save(a),lambda:save(c,editor['accessToken']))
    assert sorted(x['status'] for x in results)==['applied','conflict']
    assert call('api_resource_get',token,host)['revision']==2
    assert sql('SELECT count(*) FROM termterm_team_v2.conflicts WHERE team_id='+lit(t),True)=='1'
    # Prepared edit loses permission: no data overwritten; only an encrypted candidate is kept.
    revoked=draft(host,parent=folder,expected=2,who=editor['accessToken'])
    sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,host,member['accountId'],'edit','deny']))+')',True)
    denied(lambda:save(dict(revoked,writeCapability=None),editor['accessToken']))
    assert save(revoked,editor['accessToken'])['status']=='conflict'
    assert call('api_resource_get',token,host)['revision']==2
    denied(lambda:save(draft(folder,'folder',expected=1,deleted=True)))
    for rev in range(2,14):assert save(draft(host,parent=folder,expected=rev))['revision']==rev+1
    history=call('api_resource_history',token,host);assert len(history)==11 and history[0]['revision']==14
    assert sql('SELECT count(*) FROM termterm_team_v2.resource_key_envelopes WHERE team_id='+lit(t)+' AND resource_id='+lit(host),True)=='22'
    queue=call('api_conflicts',token,None,100)['conflicts'];assert len(queue)==2 and all(x['envelope'] for x in queue)
    assert call('api_conflicts',editor['accessToken'],None,100)['conflicts']==[]
    assert call('api_conflicts',other['accessToken'],None,100)['conflicts']==[]
    denied(lambda:call('api_conflict_decide',editor['accessToken'],queue[0]['id'],14,'server',None))
    denied(lambda:call('api_conflict_decide',token,queue[0]['id'],13,'server',None))
    assert call('api_conflict_decide',token,queue[0]['id'],14,'server',None)['revision']==14
    denied(lambda:call('api_conflict_decide',token,queue[0]['id'],14,'server',None))
    replacement=draft(host,parent=folder,expected=14)
    result=call('api_conflict_decide',token,queue[1]['id'],14,'replacement',json.dumps(replacement))
    assert result['revision']==15 and result['status']=='applied'
    assert call('api_conflicts',token,None,100)['conflicts']==[]
    assert save(draft(host,parent=folder,expected=15,deleted=True))['status']=='applied'
    denied(lambda:call('api_resource_get',token,host))
    assert host not in {x['id'] for x in call('api_resources',token,v,None,100)['records']}
    assert save(draft(folder,'folder',expected=1,deleted=True))['status']=='applied'
    # Large encrypted snapshots page by bytes as well as row count; no skipped IDs.
    big_vault=str(uuid.uuid4());call('api_vault_create',token,big_vault,blob(),1,blob())
    sql('WITH created AS (INSERT INTO termterm_team_v2.resources(team_id,vault_id,id,kind) SELECT '+lit(t)+','+lit(big_vault)+",gen_random_uuid(),'host' FROM generate_series(1,20) RETURNING *) INSERT INTO termterm_team_v2.hosts SELECT team_id,vault_id,id,NULL,'ssh',22,'personal',decode('"+'ab'*80+"','hex'),decode('"+'ab'*80+"','hex'),NULL FROM created;"+
        'INSERT INTO termterm_team_v2.resource_versions(team_id,resource_id,revision,encrypted_snapshot,actor_id) SELECT team_id,id,1,decode(repeat(\'ab\',100000),\'hex\'),'+lit(b['accountId'])+' FROM termterm_team_v2.resources WHERE team_id='+lit(t)+' AND vault_id='+lit(big_vault),True)
    seen=set();cursor=None;pages=0
    while True:
        part=call('api_resources',token,big_vault,cursor,100);pages+=1
        assert len(json.dumps(part))<3*1024*1024
        ids={x['id'] for x in part['records']};assert not seen&ids;seen|=ids
        cursor=part['nextAfter']
        if cursor is None:break
        assert pages<=10
    assert len(seen)==20 and pages>=3
    denied(lambda:sql('SELECT * FROM termterm_team_v2.resource_key_envelopes'))
    print('PASS: shared vault/folder/host CRUD, recipients, privacy, bounded pagination, atomic validation, replay, concurrent CAS, revoked edit receipts, encrypted decision queue, 11 revisions and leaf deletion.')
if __name__=='__main__':main()
