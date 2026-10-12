"""Real isolated PostgreSQL restore, protected return point and replay tests."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
    b,a=bootstrap('history');t=b['teamId'];token=a['accessToken'];v=str(uuid.uuid4());r=str(uuid.uuid4())
    call('api_vault_create',token,v,blob(),1,blob())
    seq=0
    def change(revision):
        nonlocal seq
        seq+=1;p=call('api_resource_recipients',token,v,r,None)
        return dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=r,expectedRevision=revision,
            aclRevision=p['aclRevision'],writeCapability=p['writeCapability'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),
            kind='host',parentId=None,deleted=False,encryptedName='ab'*80,encryptedAddress='bc'*80,encryptedUsername=None,
            protocol='ssh',port=22,usernamePolicy='personal',snapshot='cd'*100,
            envelopes=[dict(accountId=x['accountId'],identityVersion=x['identityVersion'],envelope='de'*120) for x in p['recipients']])
    def save(rev):return call('api_resource_save',token,json.dumps(change(rev)))
    for rev in range(3):assert save(rev)['revision']==rev+1
    assert call('api_history_select',token,r,1)['revision']==1
    assert call('api_connection_target',token,r)['temporaryRevision']==1
    assert call('api_history_selections',token)==[dict(resourceId=r,revision=1)]
    request=change(3)
    restored=call('api_resource_restore',token,1,json.dumps(request));assert restored['revision']==4 and restored['returnRevision']==3
    replay=call('api_resource_restore',token,1,json.dumps(request))
    assert all(replay[k]==restored[k] for k in ['status','revision','returnRevision','sourceRevision','operationId'])
    denied(lambda:call('api_resource_restore',token,2,json.dumps(request)))
    denied(lambda:call('api_resource_restore',token,1,json.dumps(dict(request,snapshot='ff'*80))))
    assert { (p['revision'],p['purpose']) for p in call('api_history_pins',token,r,False)}=={(1,'temporary'),(3,'return_point')}
    call('api_history_select',token,r,None)
    assert 'temporaryRevision' not in call('api_connection_target',token,r)
    assert call('api_history_selections',token)==[]
    denied(lambda:call('api_resource_restore',token,1,json.dumps(change(3))))
    for rev in range(4,17):assert save(rev)['revision']==rev+1
    rows=call('api_resource_history',token,r)
    assert len(rows)==12 and 3 in {x['revision'] for x in rows} and 1 not in {x['revision'] for x in rows}
    denied(lambda:call('api_resource_restore',token,1,json.dumps(change(17))))
    member=call('api_create_member',token,'viewer','Viewer','observer');activate(login(t,'viewer',member['temporaryPassword']));viewer=login(t,'viewer',PW)
    denied(lambda:call('api_resource_history',viewer['accessToken'],r))
    sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,r,member['accountId'],'read','allow']))+')',True)
    assert len(call('api_resource_history',viewer['accessToken'],r))==12
    denied(lambda:call('api_history_select',viewer['accessToken'],r,3))
    assert call('api_history_selections',viewer['accessToken'])==[]
    denied(lambda:call('connection_target_current',token,r))
    denied(lambda:call('api_resource_restore',viewer['accessToken'],3,json.dumps(change(17))))
    assert call('api_history_pins',viewer['accessToken'],r,True)==[]
    assert call('api_history_pins',token,r,False)==[dict(revision=3,purpose='return_point')]
    # Return to previous current as a NEW revision; protect the new return point.
    again=call('api_resource_restore',token,3,json.dumps(change(17)));assert again['revision']==18 and again['returnRevision']==17
    assert call('api_history_pins',token,r,True)==[]
    assert len(call('api_resource_history',token,r))==11
    # An abandoned old temporary revision is pruned immediately, without another write.
    call('api_history_select',token,r,18)
    for rev in range(18,31):assert save(rev)['revision']==rev+1
    assert len(call('api_resource_history',token,r))==12
    sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,r,member['accountId'],'connect','allow']))+')',True)
    call('api_history_select',viewer['accessToken'],r,18)
    call('api_history_select',token,r,None)
    assert len(call('api_resource_history',token,r))==12 # Other account still uses it.
    call('api_history_select',viewer['accessToken'],r,None)
    assert len(call('api_resource_history',token,r))==11
    assert 18 not in {x['revision'] for x in call('api_resource_history',token,r)}
    call('api_history_select',token,r,31);call('api_history_select',viewer['accessToken'],r,31)
    for rev in range(31,44):assert save(rev)['revision']==rev+1
    pins=call('api_pins_review',token,None)['items'];assert len(pins)==2
    denied(lambda:call('api_pins_review',viewer['accessToken'],None))
    denied(lambda:call('api_pins_prepare',viewer['accessToken'],json.dumps([pins[0]['id']])))
    mine=next(p for p in pins if p['accountId']==b['accountId']);theirs=next(p for p in pins if p['accountId']==member['accountId'])
    stale=call('api_pins_prepare',token,json.dumps([mine['id']]))
    call('api_history_select',token,r,None);call('api_history_select',token,r,31)
    denied(lambda:call('api_pins_finish',token,stale['id'],False))
    pins=call('api_pins_review',token,None)['items'];mine=next(p for p in pins if p['accountId']==b['accountId'])
    cancelled=call('api_pins_prepare',token,json.dumps([mine['id']]))
    call('api_pins_finish',token,cancelled['id'],True)
    denied(lambda:call('api_pins_finish',token,cancelled['id'],False))
    first=call('api_pins_prepare',token,json.dumps([mine['id']]))
    assert call('api_pins_finish',token,first['id'],False)==dict(released=1,pruned=0)
    assert len(call('api_resource_history',token,r))==12
    second=call('api_pins_prepare',token,json.dumps([theirs['id']]))
    assert call('api_pins_finish',token,second['id'],False)==dict(released=1,pruned=1)
    assert call('api_pins_finish',token,second['id'],False)==dict(released=1,pruned=1)
    assert call('api_history_selections',viewer['accessToken'])==[]
    assert len(call('api_resource_history',token,r))==11
    assert int(call('api_storage',token)['databasePhysicalBytes'])>0
    denied(lambda:sql('SELECT * FROM termterm_team_v2.pin_release_items'))
    denied(lambda:sql('SELECT * FROM termterm_team_v2.resource_restorations'))
    print('PASS: restore/new revision, exact replay, altered source/body rejection, stale/expired history, protected return pruning, readonly and account isolation, return to latest.')

if __name__=='__main__':main()
