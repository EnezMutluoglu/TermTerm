"""Real PostgreSQL account isolation tests, disposable fixed v2 lab only.
Synthetic identity blobs test storage/auth boundaries, not cryptography or SSH.
"""
import datetime, json, os, pathlib, subprocess, uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
DB='termterm_team_v2_e2e'
OWNER='termterm_team_v2_lab_owner'
CONFIG=json.loads((ROOT/'.lab/team-v2-service.json').read_text())['database']
def lit(x): return "'"+str(x).replace("'","''")+"'"
def sql(q,admin=False):
    env=os.environ.copy()
    if admin:
        cmd=['runuser','-u','postgres','--','psql']; q='SET ROLE '+OWNER+';\n'+q
    else:
        cmd=['psql','-h','localhost','-U',CONFIG['username']]
        env.update(PGPASSWORD=CONFIG['password'],PGSSLMODE='verify-full',PGSSLROOTCERT=CONFIG['caPath'])
    p=subprocess.run(cmd+['-X','-qAt','-v','ON_ERROR_STOP=1','-p','55432','-d',DB],input=q,text=True,capture_output=True,env=env)
    if p.returncode: raise RuntimeError(p.stderr)
    return p.stdout.strip()
def call(name,*args,admin=False):
    return json.loads(sql('SELECT termterm_team_v2.'+name+'('+','.join('NULL' if x is None else lit(x) for x in args)+')',admin))
def denied(fn):
    try: fn()
    except RuntimeError: return
    raise AssertionError('Forbidden call succeeded')
PW='Test-Only-Long-Password-24!'
def login(team,user,password): return call('api_login',team,user,password,uuid.uuid4(),'Disposable integration test')
def bootstrap(label):
    b=call('bootstrap_team','Disposable '+label+' '+uuid.uuid4().hex,'leader','Team leader',admin=True)
    a=login(b['teamId'],'leader',b['temporaryPassword'])
    assert a['activationRequired']
    denied(lambda:call('api_me',a['accessToken']))
    # Password byte limits and activation-only authorization are enforced by DB.
    for invalid in ['short','ş'*37,b['temporaryPassword']]:
        denied(lambda:call('api_activate',a['accessToken'],invalid,(chr(92)+'x')+'ab'*32,(chr(92)+'x')+'cd'*128))
    activate(a)
    normal=login(b['teamId'],'leader',PW)
    assert not normal['activationRequired']
    assert normal['identity']['version']==1
    return b,normal
def activate(a):
    # bytea textual hex uses one backslash in psql SQL string.
    answer=call('api_activate',a['accessToken'],PW,(chr(92)+'x')+'ab'*32,(chr(92)+'x')+'cd'*128)
    assert answer['loginRequired']
    denied(lambda:call('api_me',a['accessToken']))
    return answer
def main():
    b,leader=bootstrap('A'); other,leader_b=bootstrap('B'); token=leader['accessToken']
    assert call('api_me',token)['teamId']==b['teamId']
    m=call('api_create_member',token,'same.user','Üye İğüş','operator')
    assert len(m['temporaryPassword'])==21
    assert any(c.islower() for c in m['temporaryPassword']) and any(c.isupper() for c in m['temporaryPassword'])
    assert any(c.isdigit() for c in m['temporaryPassword']) and any(not c.isalnum() for c in m['temporaryPassword'])
    m2=call('api_create_member',leader_b['accessToken'],'same.user','Other team','observer')
    denied(lambda:call('api_create_member',token,'SAME.USER','Duplicate','operator'))
    denied(lambda:call('api_create_member',token,'bad-owner','No escalation','owner'))
    assert login(other['teamId'],'same.user',m['temporaryPassword'])['error']=='invalid_credentials'
    denied(lambda:call('api_reset_member',token,m2['accountId']))
    denied(lambda:call('api_disable_member',token,m2['accountId']))
    member_activation=login(b['teamId'],'same.user',m['temporaryPassword'])
    denied(lambda:call('api_members',member_activation['accessToken']))
    activate(member_activation)
    member=login(b['teamId'],'SAME.USER',PW)
    denied(lambda:call('api_members',member['accessToken']))
    denied(lambda:call('api_create_member',member['accessToken'],'intruder','No','operator'))
    assert sql('SELECT count(*) FROM termterm_team_v2.resource_acl WHERE team_id='+lit(b['teamId']),True)=='0'
    # Rotation is single-use; replay revokes the entire family, including new token.
    rotated=call('api_refresh',member['refreshToken']); assert 'accessToken' in rotated
    denied(lambda:call('api_me',member['accessToken']))
    assert call('api_refresh',member['refreshToken'])['error']=='refresh_replayed'
    denied(lambda:call('api_me',rotated['accessToken']))
    member=login(b['teamId'],'same.user',PW)
    reset=call('api_reset_member',token,m['accountId'])
    denied(lambda:call('api_me',member['accessToken']))
    assert login(b['teamId'],'same.user',PW)['error']=='invalid_credentials'
    replacement=call('api_reset_member',token,m['accountId'])
    assert login(b['teamId'],'same.user',reset['temporaryPassword'])['error']=='invalid_credentials'
    activation=login(b['teamId'],'same.user',replacement['temporaryPassword'])
    assert activate(activation)['recoveryRequired']
    assert sql('SELECT count(*) FROM termterm_team_v2.account_crypto_identities WHERE team_id='+lit(b['teamId'])+' AND account_id='+lit(m['accountId']),True)=='2'
    member=login(b['teamId'],'same.user',PW)
    call('api_disable_member',token,m['accountId'])
    denied(lambda:call('api_me',member['accessToken']))
    assert login(b['teamId'],'same.user',PW)['error']=='invalid_credentials'
    # Expiry by moving fixture timestamps; no long waits.
    exp=call('api_create_member',token,'expired','Expired','observer')
    sql('UPDATE termterm_team_v2.activation_tokens SET expires_at=now()-interval \'1 second\' WHERE team_id='+lit(b['teamId'])+' AND account_id='+lit(exp['accountId']),True)
    assert login(b['teamId'],'expired',exp['temporaryPassword'])['error']=='invalid_credentials'
    for i in range(5): assert login(b['teamId'],'unknown','invalid-password-123')['error']=='invalid_credentials'
    assert login(b['teamId'],'unknown','invalid-password-123')['error']=='rate_limited'
    denied(lambda:sql('SELECT * FROM termterm_team_v2.team_accounts'))
    denied(lambda:sql('SET ROLE '+OWNER))
    denied(lambda:call('bootstrap_team','Forbidden','forbidden','Forbidden'))
    denied(lambda:sql('SELECT termterm_team_v2.temporary_password()'))
    denied(lambda:sql("SET termterm.actor='forged'; SELECT * FROM termterm_team_v2.credentials"))
    denied(lambda:sql('CREATE TABLE termterm_team_v2.evil(id int)'))
    exposed=set(sql("SELECT p.proname FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='termterm_team_v2' AND has_function_privilege(current_user,p.oid,'EXECUTE')").splitlines())
    assert exposed=={'api_move_prepare','api_move_keys','api_move_stage','api_move_impact','api_move_finish','api_shared_recovery_begin','api_rotation_begin','api_rotation_next','api_rotation_stage','api_rotation_finish','api_delivery_token','api_conflict_current','api_creation_scope','api_creation_claim','api_chain_configuration','api_chain_prepare','api_chain_finish','api_connection_plan','api_connection_credential','api_pins_review','api_pins_prepare','api_pins_finish','api_shared_update_options','api_shared_update_prepare','api_shared_update_plan','api_shared_update_finish','maintenance_tick','api_storage','api_audit','api_maintenance','api_version','api_login','api_activate','api_refresh','api_logout','api_me','api_members','api_create_member','api_reset_member','api_disable_member',
      'api_recovery_key_set','api_crypto_context','api_private_credential_put','api_private_credentials','api_private_credential_get',
      'api_profile_history','api_profile_protected_put','api_recovery_profile','api_recovery_profile_rewrap','api_profile_put','api_profiles','api_recovery_begin','api_recovery_credential','api_recovery_rewrap','api_recovery_complete','api_recovery_manifest','api_recovery_pending','api_recovery_verify','api_recovery_cancel',
      'api_vault_create','api_vaults','api_resource_recipients','api_resource_save','api_operation_receipt','api_offline_decision_prepare','api_offline_decision_submit','api_resource_get','api_resources','api_resource_history','api_conflicts','api_conflict_decide','api_acl_prepare','api_acl_keys','api_acl_stage','api_acl_finish','api_acl_impact',
      'api_share_candidates','api_share_prepare','api_share_plan','api_share_finish','api_shared_credentials','api_shared_credential_get','api_shared_credential_remove','api_delegations','api_delegation_set','api_connection_target','api_resource_restore','api_history_pins','api_history_select','api_history_selections'}, exposed
    assert sql("SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='termterm_team_v2' AND c.relkind='r' AND NOT c.relrowsecurity",True)=='0'
    assert sql('SELECT count(*) FROM termterm_team_v2.team_accounts WHERE password_hash IS NOT NULL AND password_hash NOT LIKE \'$2a$12$%\'',True)=='0'
    assert sql('SELECT count(*) FROM termterm_team_v2.team_accounts WHERE team_id='+lit(other['teamId']),True)=='2'
    print('PASS: Team-scoped account/auth/activation/reset/replay/expiry/rate-limit/RLS tests on PostgreSQL 18 with verified TLS. Synthetic identity payloads; SSH/crypto not covered.')
if __name__=='__main__': main()
