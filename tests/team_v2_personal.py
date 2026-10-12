"""Database enforcement only; real cryptographic tests live in team-client.
All records are disposable in the isolated Team v2 database.
"""
import json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
def blob(n=128):return chr(92)+'x'+'ab'*n
def main():
    b,leader=bootstrap('personal');other,other_leader=bootstrap('other personal');t=b['teamId'];owner=leader['accessToken']
    m=call('api_create_member',owner,'operator','Operatör','operator');activate(login(t,'operator',m['temporaryPassword']));user=login(t,'operator',PW);token=user['accessToken']
    rk=call('api_recovery_key_set',owner,PW,blob(32))['keyId']
    c=str(uuid.uuid4())
    def save(expected=0,who=token,ud=None):return call('api_private_credential_put',who,c,expected,'ssh_key',blob(),blob(),1,rk,ud if ud is not None else blob(),blob(),blob(),blob())
    assert save()['revision']==1
    assert len(call('api_private_credentials',token))==1
    assert call('api_private_credentials',owner)==[]
    denied(lambda:call('api_private_credential_get',owner,c))
    denied(lambda:call('api_private_credential_get',other_leader['accessToken'],c))
    denied(lambda:save(1,owner))
    denied(lambda:save(1,ud=blob(1)))
    assert call('api_private_credential_get',token,c)['revision']==1
    denied(lambda:save(0))
    for r in range(1,13): assert save(r)['revision']==r+1
    assert sql('SELECT count(*) FROM termterm_team_v2.credential_versions WHERE team_id='+lit(t)+' AND credential_id='+lit(c),True)=='11'
    identity_version=1
    def profile(host,rev,method,credential,username):return call('api_profile_protected_put',token,host,rev,method,credential,username,identity_version,rk,blob(),blob())
    denied(lambda:call('api_profile_put',token,None,0,'credential',c,blob()))
    assert profile(None,0,'credential',c,blob())['revision']==1
    denied(lambda:call('api_profile_put',owner,None,0,'credential',c,None))
    denied(lambda:call('api_profile_put',other_leader['accessToken'],None,0,'credential',c,None))
    assert call('api_profiles',owner)['default'] is None
    assert call('api_profiles',token)['default']['credentialId']==c
    # Typed host/folder fixtures, created by the migrator solely for boundary tests.
    v,f,f2,h,h2=[str(uuid.uuid4()) for _ in range(5)]
    sql('INSERT INTO termterm_team_v2.vaults(team_id,id,encrypted_name) VALUES('+lit(t)+','+lit(v)+','+lit(blob())+');'+
        ''.join('INSERT INTO termterm_team_v2.resources(team_id,vault_id,id,kind) VALUES('+','.join(map(lit,[t,v,i,k]))+');' for i,k in [(f,'folder'),(f2,'folder'),(h,'host'),(h2,'host')])+
        'INSERT INTO termterm_team_v2.folders VALUES('+','.join(map(lit,[t,v,f]))+',NULL,'+lit(blob())+'),('+','.join(map(lit,[t,v,f2,f]))+','+lit(blob())+');'+
        ''.join('INSERT INTO termterm_team_v2.hosts VALUES('+','.join(map(lit,[t,v,x,f,'ssh',22,policy,blob(),blob(),blob()]))+');' for x,policy in [(h,'shared_required'),(h2,'personal')])+
        'INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,f,m['accountId'],'connect','allow']))+');',True)
    denied(lambda:call('api_profile_put',token,h,0,'credential',c,blob()))
    assert call('api_profile_put',token,h,0,'credential',c,None)['revision']==1
    assert profile(h2,0,'prompt',None,blob())['revision']==1
    denied(lambda:call('api_profile_put',token,h2,0,'prompt',None,None))
    denied(lambda:sql('UPDATE termterm_team_v2.folders SET parent_id='+lit(f2)+' WHERE team_id='+lit(t)+' AND id='+lit(f),True))
    sql('INSERT INTO termterm_team_v2.host_hops VALUES('+','.join(map(lit,[t,h,0,h2]))+')',True)
    denied(lambda:sql('INSERT INTO termterm_team_v2.host_hops VALUES('+','.join(map(lit,[t,h2,0,h]))+')',True))
    sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,h2,m['accountId'],'connect','deny']))+')',True)
    assert len(call('api_profiles',token)['hosts'])==1
    denied(lambda:call('api_profile_put',token,h2,1,'prompt',None,None))
    denied(lambda:call('api_recovery_begin',token,PW,m['accountId'],rk,'Unauthorized attempt'))
    assert call('api_recovery_begin',owner,'wrong-password',m['accountId'],rk,'Testing password denial')['error']=='invalid_credentials'
    denied(lambda:call('api_recovery_begin',other_leader['accessToken'],PW,m['accountId'],rk,'Wrong team request'))
    reset=call('api_reset_member',owner,m['accountId']);activate(login(t,'operator',reset['temporaryPassword']));user=login(t,'operator',PW);token=user['accessToken']
    assert call('api_private_credentials',token)[0]['dataEnvelope'] is None
    op=call('api_recovery_begin',owner,PW,m['accountId'],rk,'Approved company credential recovery')['operationId']
    assert call('api_recovery_pending',token)[0]['id']==op
    assert call('api_recovery_pending',owner)==[]
    assert call('api_recovery_manifest',owner,op)['identityVersion']==2
    assert call('api_recovery_manifest',token,op)['credentials']==[c]
    denied(lambda:call('api_recovery_manifest',other_leader['accessToken'],op))
    denied(lambda:call('api_recovery_verify',owner,op,c))
    denied(lambda:call('api_recovery_cancel',token,op))
    data=call('api_recovery_credential',owner,op,c);assert len(data['versions'])==11
    denied(lambda:call('api_recovery_credential',other_leader['accessToken'],op,c))
    denied(lambda:call('api_recovery_complete',owner,op))
    denied(lambda:call('api_recovery_complete',token,op))
    for version in data['versions']:assert call('api_recovery_rewrap',owner,op,c,version['revision'],2,blob(),blob())['verificationRequired']
    assert len(call('api_recovery_verify',token,op,c)['versions'])==11
    for item in call('api_recovery_manifest',owner,op)['profiles']:
        denied(lambda:call('api_recovery_profile',other_leader['accessToken'],op,item['hostKey'],item['revision'],False))
        call('api_recovery_profile',owner,op,item['hostKey'],item['revision'],False)
        call('api_recovery_profile_rewrap',owner,op,item['hostKey'],item['revision'],2,blob())
        assert call('api_recovery_profile',token,op,item['hostKey'],item['revision'],True)['envelope']
    assert call('api_recovery_complete',token,op)['completed']
    assert call('api_recovery_complete',token,op)['completed']
    assert call('api_recovery_pending',token)==[]
    denied(lambda:call('api_recovery_credential',owner,op,c))
    assert sql("SELECT count(*) FROM termterm_team_v2.audit_events WHERE team_id="+lit(t)+" AND action='recovery.read_envelopes'",True)=='1'
    identity_version=2
    for revision in range(1,14):
        profile(None,revision,'credential',c,blob(130+revision))
    assert sql("SELECT count(*) FROM termterm_team_v2.profile_versions WHERE team_id="+lit(t)+" AND account_id="+lit(m['accountId'])+" AND host_key='default'",True)=='11'
    assert sql("SELECT count(*) FROM termterm_team_v2.profile_username_archives WHERE team_id="+lit(t)+" AND account_id="+lit(m['accountId'])+" AND host_key='default'",True)=='10'
    denied(lambda:sql('SELECT * FROM termterm_team_v2.profile_username_archives'))
    assert len(call('api_profile_history',token,None))==10
    assert call('api_profile_history',owner,None)==[]
    assert call('api_profile_history',other_leader['accessToken'],None)==[]
    denied(lambda:call('api_profile_history',token,h2))
    denied(lambda:call('profile_put_internal',token,None,14,'prompt',None,blob()))
    cancelled=call('api_recovery_begin',owner,PW,m['accountId'],rk,'Cancelled recovery fixture')['operationId']
    assert call('api_recovery_cancel',owner,cancelled)['cancelled']
    denied(lambda:call('api_recovery_manifest',owner,cancelled))
    denied(lambda:call('api_recovery_complete',token,cancelled))
    print('PASS: private-secret isolation, CAS/atomic rollback, 11 retained versions, own/default/host profiles, inherited deny, graph cycles, reset preservation, audited recovery/rewrap/user confirmation.')
if __name__=='__main__':main()
