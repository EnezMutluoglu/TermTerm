"""SQL isolation/CAS and rotation coverage; cryptography is tested by Rust HTTPS."""
import json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,PW,activate,login
def blob(byte='ab',n=128):return '\\x'+byte*n
def main():
    b,leader=bootstrap('rotation');other,outsider=bootstrap('rotation other');t=b['teamId'];token=leader['accessToken']
    old=call('api_recovery_key_set',token,PW,blob('ac',32))['keyId']
    member=call('api_create_member',token,'member','Member','operator');activate(login(t,'member',member['temporaryPassword']));m=login(t,'member',PW)['accessToken']
    c=str(uuid.uuid4())
    def save(rev,key=old):return call('api_private_credential_put',m,c,rev,'password',blob(),blob(),1,key,blob(),blob(),blob(),blob())
    save(0)
    denied(lambda:call('api_rotation_begin',m,PW,blob('ac',32),blob('ad',32),'not an owner'))
    assert call('api_rotation_begin',token,'wrong password',blob('ac',32),blob('ad',32),'password denial')['error']=='invalid_credentials'
    denied(lambda:call('api_rotation_begin',token,PW,blob('ae',32),blob('ad',32),'wrong old key'))
    job=call('api_rotation_begin',token,PW,blob('ac',32),blob('ad',32),'Scheduled disposable rotation')
    op=job['operationId'];new=job['newKeyId']
    denied(lambda:call('api_rotation_next',m,op));denied(lambda:call('api_rotation_next',outsider['accessToken'],op))
    denied(lambda:call('api_rotation_finish',token,op))
    first=call('api_rotation_next',token,op);assert first['remaining']==2
    item=first['item'];body={k:item[k] for k in ['credentialId','revision','purpose','sourceHash']};body['envelope']='ef'*128
    denied(lambda:call('api_rotation_stage',token,op,json.dumps({**body,'sourceHash':'aa'*32})))
    assert call('api_rotation_stage',token,op,json.dumps(body))['staged']
    assert call('api_rotation_stage',token,op,json.dumps(body))['staged']
    denied(lambda:call('api_rotation_stage',token,op,json.dumps({**body,'envelope':'ee'*128})))
    # A fresh credential version during rotation still uses the old active key.
    save(1);assert call('api_rotation_next',token,op)['remaining']==3
    sql('UPDATE termterm_team_v2.recovery_rotations SET authorized_until=\'-infinity\' WHERE id='+lit(op),True)
    denied(lambda:call('api_rotation_next',token,op))
    resumed=call('api_rotation_begin',token,PW,blob('ac',32),blob('ad',32),'Resume after interruption');assert resumed['operationId']==op
    denied(lambda:call('api_rotation_begin',token,PW,blob('ac',32),blob('ae',32),'Cannot replace pending file'))
    while (row:=call('api_rotation_next',token,op))['remaining']:
        item=row['item'];body={k:item[k] for k in ['credentialId','revision','purpose','sourceHash']};body['envelope']='ef'*128
        call('api_rotation_stage',token,op,json.dumps(body))
    # Missing even one staged envelope prevents retiring the old key.
    sql('DELETE FROM termterm_team_v2.credential_key_envelopes WHERE team_id='+lit(t)+' AND recovery_key_id='+lit(new)+" AND revision=1 AND purpose='data'",True)
    denied(lambda:call('api_rotation_finish',token,op))
    item=call('api_rotation_next',token,op)['item'];body={k:item[k] for k in ['credentialId','revision','purpose','sourceHash']};body['envelope']='ef'*128;call('api_rotation_stage',token,op,json.dumps(body))
    assert call('api_rotation_finish',token,op)['completed'];assert call('api_rotation_finish',token,op)['completed']
    assert call('api_crypto_context',token)['recovery']['keyId']==new
    receipt=call('api_rotation_begin',token,PW,blob('ac',32),blob('ad',32),'Inspect completed operation');assert receipt['status']=='completed' and receipt['operationId']==op
    denied(lambda:save(2));save(2,new)
    denied(lambda:sql('SELECT * FROM termterm_team_v2.recovery_rotations'))
    print('PASS: rotation reauthentication, isolation, source CAS, retry, resume, concurrent new version, missing envelope and atomic retirement.')
if __name__=='__main__':main()
