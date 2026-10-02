"""Disposable, real PostgreSQL authorization/CAS/history tests (WSL lab only).
Run as root in WSL: python3 tests/team_postgres.py. Never uses user vaults.
SQL is sent via stdin; tokens and passwords are never printed.
"""
import json, subprocess, uuid, datetime, pathlib, os
from concurrent.futures import ThreadPoolExecutor
ROOT=pathlib.Path(__file__).resolve().parents[1]
DB='termterm_team_e2e'
def sql(query, app=True):
    env=os.environ.copy()
    if app:
        profile=json.loads((ROOT/'.lab/team-connection.json').read_text())
        env.update(PGPASSWORD=profile['password'],PGSSLMODE='verify-full',PGSSLROOTCERT=str(ROOT/'.lab/ca.crt'))
        command=['psql','-h','localhost','-U','termterm_team_app']
    else: command=['runuser','-u','postgres','--','psql']
    p=subprocess.run(command+['-X','-qAt','-v','ON_ERROR_STOP=1','-p','55432','-d',DB],input=query,text=True,capture_output=True,env=env)
    if p.returncode: raise RuntimeError(p.stderr)
    return p.stdout.strip()
def lit(value): return "'"+str(value).replace("'","''")+"'"
def rpc(account,action,body={}):
    return json.loads(sql('SELECT termterm_team.rpc(%s,%s,%s::jsonb)'%(lit(account['accessToken']),lit(action),lit(json.dumps(body)))))
def denied(fn):
    try: fn()
    except RuntimeError: return
    raise AssertionError('Forbidden operation unexpectedly succeeded')
def register(role):
    name=role+'_'+uuid.uuid4().hex[:10]
    return json.loads(sql("SELECT termterm_team.register_account(%s,%s,'Disposable-test-1234',decode('%s','hex'),decode('01','hex'),%s,'SQL test')"%(lit(name),lit(name+'@example.invalid'),'ab'*32,lit(uuid.uuid4()))))
def main():
    subprocess.run(['bash',str(ROOT/'scripts/team-lab.sh'),DB],check=True)
    owner,manager,editor,operator,outsider=[register(r) for r in ['owner','manager','editor','operator','outsider']]
    team=rpc(owner,'create_team',{'name':'Disposable ACL test'})['id']
    exposed=set(sql("SELECT p.proname FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='termterm_team' AND has_function_privilege('termterm_team_app',p.oid,'EXECUTE')").splitlines())
    assert exposed=={'register_account','login_account','refresh_session','rpc','deliver_pending'}
    denied(lambda: sql('SET ROLE termterm_team_executor'))
    denied(lambda: sql("CREATE TABLE termterm_team.forbidden(id int)"))
    denied(lambda: sql("SELECT termterm_team.register_account('invalid_pw','invalid@example.invalid',repeat('a',73),decode(repeat('ab',32),'hex'),decode('01','hex'),gen_random_uuid(),'test')"))
    denied(lambda: sql("SELECT termterm_team.register_account('invalid_unicode','invalidu@example.invalid',repeat('ş',37),decode(repeat('ab',32),'hex'),decode('01','hex'),gen_random_uuid(),'test')"))
    vault=rpc(owner,'create_vault',{'teamId':team,'name':'Test records'})['id']
    for user,role in [(manager,'manager'),(editor,'editor'),(operator,'operator')]: rpc(owner,'member_set',{'teamId':team,'userId':user['userId'],'role':role,'active':True})
    def acl(user,record,permission,effect):
        revision=rpc(owner,'overview')['teams'][0]['aclRevision']
        return rpc(owner,'acl_set',dict(vaultId=vault,userId=user['userId'],recordId=record,permission=permission,effect=effect,aclRevision=revision))
    sequence={a['userId']:0 for a in [owner,manager,editor,operator,outsider]}
    def change(account,record,rev=0,kind='host',parent=None,deleted=False):
        sequence[account['userId']]+=1
        recipients=rpc(owner,'recipients',{'vaultId':vault,'recordId':record if rev else None,'parentId':parent})
        envelopes=[dict(userId=x['userId'],purpose=p,envelope='ab'*80) for x in recipients['recipients'] for p in ['data','secret'] if x[p]]
        return dict(operationId=str(uuid.uuid4()),sequence=sequence[account['userId']],vaultId=vault,recordId=record,kind=kind,parentId=parent,expectedRevision=rev,clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),aclRevision=recipients['aclRevision'],payload='ab'*80,secrets='cd'*80,envelopes=envelopes,links=[],deleted=deleted)
    root,child,host,hidden=[str(uuid.uuid4()) for _ in range(4)]
    for rec,kind,parent in [(root,'group',None),(child,'group',root),(host,'host',child),(hidden,'host',root)]: assert rpc(owner,'apply',change(owner,rec,kind=kind,parent=parent))['status']=='applied'
    for p in ['read','connect','edit']: acl(editor,root,p,'allow')
    for p in ['read','connect']: acl(operator,host,p,'allow')
    for p in ['read','connect','edit','manage']: acl(manager,child,p,'allow')
    rpc(owner,'delegate',dict(teamId=team,vaultId=vault,userId=manager['userId'],recordId=child,permissions=['read','connect','edit']))
    def manager_acl(record,permission,target):
        revision=rpc(owner,'overview')['teams'][0]['aclRevision']
        return rpc(manager,'acl_set',dict(vaultId=vault,userId=target['userId'],recordId=record,permission=permission,effect='allow',aclRevision=revision))
    denied(lambda:manager_acl(root,'read',operator))
    denied(lambda:manager_acl(host,'manage',operator))
    denied(lambda:manager_acl(host,'edit',manager))
    denied(lambda:rpc(manager,'member_set',dict(teamId=team,userId=operator['userId'],role='manager',active=True)))
    manager_acl(host,'read',operator)
    assert len(rpc(operator,'records',{'vaultId':vault})['records'])==1
    assert len(rpc(operator,'records',{'vaultId':vault})['paths'])==2
    denied(lambda: rpc(outsider,'records',{'vaultId':vault}))
    denied(lambda: sql('SELECT * FROM termterm_team.users'))
    denied(lambda: sql("SET termterm.actor=%s; SELECT termterm_team.allowed(%s,%s,%s,'read')"%(lit(owner['userId']),lit(owner['userId']),lit(vault),lit(host))))
    denied(lambda: sql("SELECT termterm_team.rpc('forged','overview','{}')"))
    acl(editor,child,'edit','deny')
    candidate=change(editor,host,1,parent=child)
    assert rpc(editor,'apply',candidate)['reason']=='permission'
    acl(editor,child,'edit','inherit')
    body=change(editor,host,1,parent=child)
    assert rpc(editor,'apply',body)['revision']==2
    assert rpc(editor,'apply',body)['revision']==2
    stale=change(owner,host,1,parent=child)
    result=rpc(owner,'apply',stale); assert result['reason']=='revision'
    assert len(rpc(editor,'conflicts',{'vaultId':vault}))==2
    denied(lambda: rpc(operator,'decide',{'vaultId':vault,'conflictId':result['conflictId'],'currentRevision':2,'choice':'server'}))
    assert rpc(editor,'decide',{'vaultId':vault,'conflictId':result['conflictId'],'currentRevision':2,'choice':'server'})['status']=='discarded'
    rpc(owner,'pin',dict(vaultId=vault,recordId=host,revision=1,purpose='temporary'))
    for rev in range(2,16): assert rpc(owner,'apply',change(owner,host,rev,parent=child))['revision']==rev+1
    history=rpc(owner,'history',dict(vaultId=vault,recordId=host)); assert len(history)==12 and history[-1]['revision']==1
    rpc(owner,'unpin',dict(vaultId=vault,recordId=host,purpose='temporary'))
    assert len(rpc(owner,'history',dict(vaultId=vault,recordId=host)))==11
    move_preview=rpc(owner,'move_preview',dict(vaultId=vault,recordId=host,parentId=root))
    assert move_preview['recordRevision']==16
    stored=next(r for r in rpc(owner,'records',dict(vaultId=vault))['records'] if r['id']==host)
    assert stored['parentId']==child
    # A host-scoped editor does not need edit rights on its containing folders.
    isolated=str(uuid.uuid4())
    rpc(owner,'apply',change(owner,isolated,kind='host',parent=child))
    for p in ['read','edit']: acl(operator,isolated,p,'allow')
    assert rpc(operator,'apply',change(operator,isolated,1,parent=child))['revision']==2
    # Two separate TLS database clients hit the same CAS concurrently.
    raced=str(uuid.uuid4());rpc(owner,'apply',change(owner,raced,parent=root))
    with ThreadPoolExecutor(max_workers=2) as pool:
        proposals=[(owner,change(owner,raced,1,parent=root)),(editor,change(editor,raced,1,parent=root))]
        results=list(pool.map(lambda pair:rpc(pair[0],'apply',pair[1]),proposals))
    assert sorted(r['status'] for r in results)==['applied','conflict']
    with ThreadPoolExecutor(max_workers=2) as pool:
        proposals=[(owner,change(owner,raced,2,parent=root,deleted=True)),(editor,change(editor,raced,2,parent=root))]
        results=list(pool.map(lambda pair:rpc(pair[0],'apply',pair[1]),proposals))
    assert sorted(r['status'] for r in results)==['applied','conflict']
    assert rpc(owner,'history',dict(vaultId=vault,recordId=raced))[0]['revision']==3
    rpc(owner,'member_set',dict(teamId=team,userId=editor['userId'],role='editor',active=False))
    denied(lambda: rpc(editor,'records',{'vaultId':vault}))
    body=change(editor,host,16,parent=child)
    delivered=json.loads(sql('SELECT termterm_team.deliver_pending(%s,%s::jsonb)'%(lit(editor['deliveryToken']),lit(json.dumps(body)))))
    assert delivered['status']=='conflict' and delivered['reason']=='permission'
    assert rpc(owner,'history',dict(vaultId=vault,recordId=host))[0]['revision']==16
    assert rpc(owner,'audit',dict(teamId=team))
    event=dict(vaultId=vault,recordId=host,event='connection.closed',operationId=str(uuid.uuid4()),clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),bytes=0)
    for _ in range(2):
        assert json.loads(sql('SELECT termterm_team.deliver_pending(%s,%s::jsonb)'%(lit(editor['deliveryToken']),lit(json.dumps(event)))))['status']=='received'
    audit=rpc(owner,'audit',dict(teamId=team))
    delivered_audit=[e for e in audit if e['summary'].get('operationId')==event['operationId']]
    assert len(delivered_audit)==1 and delivered_audit[0]['client_at']
    # Re-adding a removed member must not revive stale grants or delegations.
    rpc(owner,'member_set',dict(teamId=team,userId=editor['userId'],role='editor',active=True))
    assert rpc(editor,'vaults',dict(teamId=team))==[]
    assert rpc(editor,'records',dict(vaultId=vault))['records']==[]
    acl(editor,host,'read','allow')
    assert len(rpc(editor,'records',dict(vaultId=vault))['records'])==1
    refreshed=json.loads(sql('SELECT termterm_team.refresh_session(%s)'%lit(outsider['refreshToken'])))
    denied(lambda:rpc(outsider,'overview'))
    denied(lambda:sql('SELECT termterm_team.refresh_session(%s)'%lit(outsider['refreshToken'])))
    outsider.update(accessToken=refreshed['accessToken'],refreshToken=refreshed['refreshToken'])
    rpc(outsider,'overview');rpc(outsider,'logout');denied(lambda:rpc(outsider,'overview'))
    print('PASS: accounts/token rotation, SQL denial, ACL/delegation, host-only edits, concurrent edit/delete CAS, idempotency, decisions, history/pins, revoked candidate/audit delivery')
if __name__=='__main__': main()
