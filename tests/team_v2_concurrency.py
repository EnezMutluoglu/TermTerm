"""Short simultaneous refresh/reset/CAS exercises; no soak or sleeps for policy expiry."""
from concurrent.futures import ThreadPoolExecutor
import uuid
from team_v2_accounts import bootstrap,call,denied,activate,login,PW
from team_v2_personal import blob
def together(a,b):
    import threading
    barrier=threading.Barrier(2)
    def run(fn):barrier.wait();return fn()
    with ThreadPoolExecutor(max_workers=2) as pool:
        first=pool.submit(run,a);second=pool.submit(run,b)
        return first.result(),second.result()
def main():
    b,leader=bootstrap('concurrency');owner=leader['accessToken'];t=b['teamId']
    m=call('api_create_member',owner,'race-user','Concurrency test','operator')
    activation=login(t,'race-user',m['temporaryPassword'])
    def activating():
        try:return call('api_activate',activation['accessToken'],PW,blob(32),blob())
        except RuntimeError as e:
            assert 'invalid_session' in str(e);return {'invalidSession':True}
    results=together(activating,activating)
    assert sum(x.get('activated',False) for x in results)==1 and sum(x.get('invalidSession',False) for x in results)==1
    u=login(t,'race-user',PW)
    first,second=together(lambda:call('api_refresh',u['refreshToken']),lambda:call('api_refresh',u['refreshToken']))
    results=[first,second]
    assert sum('accessToken' in x for x in results)==1 and sum(x.get('error')=='refresh_replayed' for x in results)==1
    denied(lambda:call('api_me',next(x['accessToken'] for x in results if 'accessToken' in x)))
    u=login(t,'race-user',PW)
    refreshed,reset=together(lambda:call('api_refresh',u['refreshToken']),lambda:call('api_reset_member',owner,m['accountId']))
    if 'accessToken' in refreshed:denied(lambda:call('api_me',refreshed['accessToken']))
    else:assert refreshed['error']=='invalid_session'
    activate(login(t,'race-user',reset['temporaryPassword']));u=login(t,'race-user',PW)
    rk=call('api_recovery_key_set',owner,PW,blob(32))['keyId'];c=str(uuid.uuid4())
    def save():
        try:return call('api_private_credential_put',u['accessToken'],c,0,'ssh_key',blob(),blob(),2,rk,blob(),blob(),blob(),blob())
        except RuntimeError as e:
            assert 'revision_conflict' in str(e);return {'conflict':True}
    a,b=together(save,save)
    assert sum(x.get('revision')==1 for x in [a,b])==1 and sum(x.get('conflict',False) for x in [a,b])==1
    print('PASS: concurrent activation, refresh replay, refresh versus reset without deadlock, and simultaneous credential creation CAS.')
if __name__=='__main__':main()
