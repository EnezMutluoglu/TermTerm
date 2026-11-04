"""Real PostgreSQL Team VPN qualification. Creates fresh private databases only."""
import importlib.util,json,subprocess,uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
OWNER='termterm_team_upgrade_owner'
def command(args,stdin=None):
 p=subprocess.run(args,input=stdin,text=True,capture_output=True)
 if p.returncode:raise RuntimeError(p.stderr.splitlines()[0] if p.stderr else 'database command failed')
 return p.stdout.strip()
def sql(db,text):return command(['runuser','-u','postgres','--','psql','-X','-qAt','-v','ON_ERROR_STOP=1','-p','55432','-d',db],text)
def quoted(x):return 'NULL' if x is None else "'"+str(x).replace("'","''")+"'"
def call(db,name,*args):return json.loads(sql(db,'SET ROLE '+OWNER+';SELECT termterm_team_v2.'+name+'('+','.join(map(quoted,args))+')'))
def denied(fn):
 try:fn()
 except RuntimeError:return
 raise AssertionError('forbidden VPN request succeeded')
def main():
 for baseline in [0,65]:
  db='termterm_vpn_'+uuid.uuid4().hex[:12]
  sql('postgres',f'CREATE DATABASE {db} OWNER {OWNER};')
  sql(db,f'REVOKE ALL ON DATABASE {db} FROM PUBLIC;')
  migrations=sorted((ROOT/'team-service/migrations').glob('[0-9][0-9][0-9]_*.sql'))
  for file in migrations:
   if int(file.name[:3])>65:continue
   sql(db,'SET ROLE '+OWNER+';'+file.read_text())
  if baseline==0:sql(db,'SET ROLE '+OWNER+';'+(ROOT/'team-service/migrations/066_vpn.sql').read_text())
  b=call(db,'bootstrap_team','Disposable VPN qualification','leader','Leader')
  pw='Disposable-VPN-Fixture-Password-123!'
  devices={}
  def login(user,password):
   device=str(uuid.uuid4());devices[user]=device;return call(db,'api_login',b['teamId'],user,password,device,'VPN fixture')
  first=login('leader',b['temporaryPassword'])
  call(db,'api_activate',first['accessToken'],pw,chr(92)+'x'+'ab'*32,chr(92)+'x'+'cd'*128)
  leader=login('leader',pw)['accessToken']
  member=call(db,'api_create_member',leader,'member','Member','operator')
  first=login('member',member['temporaryPassword'])
  call(db,'api_activate',first['accessToken'],pw,chr(92)+'x'+'de'*32,chr(92)+'x'+'ef'*128)
  member_token=login('member',pw)['accessToken']
  retained=sql(db,"SELECT md5(string_agg(row_to_json(x)::text,'' ORDER BY id)) FROM termterm_team_v2.team_accounts x")
  if baseline==65:sql(db,'SET ROLE '+OWNER+';'+(ROOT/'team-service/migrations/066_vpn.sql').read_text())
  assert retained==sql(db,"SELECT md5(string_agg(row_to_json(x)::text,'' ORDER BY id)) FROM termterm_team_v2.team_accounts x")
  context=call(db,'api_crypto_context',leader)
  recovery=call(db,'api_recovery_key_set',leader,pw,chr(92)+'x'+'aa'*32)['keyId']
  def private(token,kind,credential=None,expected=0):
   c=credential or str(uuid.uuid4())
   call(db,'api_private_credential_put',token,c,expected,kind,chr(92)+'x'+'ab'*80,chr(92)+'x'+'bc'*80,1,recovery,*[chr(92)+'x'+'cd'*120]*4)
   return c
  vpn=private(leader,'vpn');auth=private(leader,'vpn_auth')
  denied(lambda:private(leader,'password',vpn,1))
  denied(lambda:private(leader,'vpn',auth,1))
  denied(lambda:call(db,'api_private_credential_get',member_token,vpn))
  vault=str(uuid.uuid4());folder=str(uuid.uuid4());host=str(uuid.uuid4());other=str(uuid.uuid4())
  call(db,'api_vault_create',leader,vault,chr(92)+'x'+'aa'*100,1,chr(92)+'x'+'bb'*120)
  seq=0
  def resource(id,kind,parent=None):
   nonlocal seq;seq+=1
   p=call(db,'api_resource_recipients',leader,vault,id,parent)
   body=dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=vault,resourceId=id,expectedRevision=0,aclRevision=p['aclRevision'],writeCapability=p['writeCapability'],clientAt='2026-10-29T00:00:00Z',kind=kind,parentId=parent,deleted=False,encryptedName='ab'*80,encryptedAddress='bc'*80 if kind=='host' else None,encryptedUsername=None,protocol='ssh' if kind=='host' else None,port=22 if kind=='host' else None,usernamePolicy='personal' if kind=='host' else None,snapshot='cd'*100,envelopes=[dict(accountId=x['accountId'],identityVersion=x['identityVersion'],envelope='de'*120) for x in p['recipients']])
   call(db,'api_resource_save',leader,json.dumps(body))
  resource(folder,'folder');resource(host,'host',folder);resource(other,'host')
  sql(db,'SET ROLE '+OWNER+"; INSERT INTO termterm_team_v2.resource_acl(team_id,resource_id,account_id,permission,effect) SELECT "+','.join(map(quoted,[b['teamId'],folder,member['accountId']]))+",p,'allow' FROM unnest(ARRAY['read','connect','edit'])p;")
  recipient=dict(accountId=member['accountId'],view=True,connect=True,edit=False,reveal=False,export=False)
  plan=call(db,'api_share_prepare',leader,vpn,folder,json.dumps([recipient]))
  body=dict(encryptedData='ab'*80,encryptedSecret='bc'*80,recoveryData='cd'*120,recoverySecret='de'*120,envelopes=[dict(accountId=r['accountId'],identityVersion=r['identityVersion'],data='ef'*120,secret='fa'*120) for r in plan['recipients']])
  shared=call(db,'api_share_finish',leader,plan['id'],json.dumps(body))['credentialId']
  denied(lambda:call(db,'api_share_prepare',leader,auth,folder,json.dumps([recipient])))
  assert call(db,'api_vpn_binding',leader,folder,0,shared,True)['vpnId']==shared
  denied(lambda:call(db,'api_vpn_legacy_connection',member_token,host))
  assert call(db,'api_vpn_required',member_token,host)['vpnId']==shared
  assert call(db,'api_vpn_required_for_plan',member_token,host,host)['vpnId']==shared
  denied(lambda:call(db,'api_vpn_required_for_plan',member_token,host,other))

  # A dependency may be consumed through its authorized root despite an explicit standalone host denial.
  hop=str(uuid.uuid4());resource(hop,'host',folder)
  sql(db,'SET ROLE '+OWNER+';INSERT INTO termterm_team_v2.host_hops(team_id,host_id,position,jump_host_id) VALUES('+','.join(map(quoted,[b['teamId'],host,0,hop]))+');INSERT INTO termterm_team_v2.connection_dependency_envelopes VALUES('+','.join(map(quoted,[b['teamId'],host,hop,1,member['accountId'],1]))+",decode('"+'ab'*120+"','hex'));INSERT INTO termterm_team_v2.resource_acl VALUES("+','.join(map(quoted,[b['teamId'],hop,member['accountId'],'connect','deny']))+');')
  denied(lambda:call(db,'api_vpn_required',member_token,hop))
  assert call(db,'api_vpn_required_for_plan',member_token,host,hop)['vpnId']==shared
  # Revision one recorded an empty route; the current hop must not leak into that selection.
  call(db,'api_history_select',member_token,host,1)
  denied(lambda:call(db,'api_vpn_required_for_plan',member_token,host,hop))
  assert call(db,'api_vpn_required_for_plan',member_token,host,host)['vpnId']==shared
  call(db,'api_history_select',member_token,host,None)
  assert call(db,'api_vpn_required_for_plan',member_token,host,hop)['vpnId']==shared

  denied(lambda:call(db,'api_vpn_binding',leader,host,0,vpn,True))
  denied(lambda:call(db,'api_shared_credential_get',member_token,shared,'reveal',None))
  denied(lambda:call(db,'api_shared_credential_get',member_token,shared,'export',None))
  assert call(db,'api_shared_credential_get',member_token,shared,'vpn',None)['kind']=='vpn'
  private_member=private(member_token,'vpn')
  call(db,'api_vpn_personal_binding',member_token,host,0,private_member,True)
  assert call(db,'api_vpn_required',member_token,host)['vpnId']==private_member
  assert call(db,'api_vpn_required',leader,host)['vpnId']==shared
  call(db,'api_vpn_personal_binding',member_token,host,1,None,True)
  sql(db,'SET ROLE '+OWNER+'; DELETE FROM termterm_team_v2.resource_acl WHERE team_id='+quoted(b['teamId'])+' AND resource_id='+quoted(shared)+' AND account_id='+quoted(member['accountId'])+" AND permission='connect';")
  denied(lambda:call(db,'api_vpn_required',member_token,host))
  denied(lambda:call(db,'api_vpn_authorize',member_token,shared))
  # A typed config+personal-auth operation commits atomically and has a durable exact retry receipt.
  authority=call(db,'api_vpn_create_authority',leader)['authority']
  sequence=1000
  def payload(kind,expected,recovery_id=None):return dict(expectedRevision=expected,kind=kind,encryptedData='ab'*80,encryptedSecret='bc'*80,identityVersion=1,recoveryKeyId=recovery_id or recovery,userData='cd'*120,userSecret='de'*120,recoveryData='ef'*120,recoverySecret='fa'*120)
  def operation(vpn_id,base,config,auth_body=None,auth_id=None,auth_base=0,cap=None):
   nonlocal sequence;sequence+=1
   return dict(operationId=str(uuid.uuid4()),deviceId=devices['leader'],sequence=sequence,vpnId=vpn_id,baseRevision=base,creationAuthority=cap,config=config,auth=auth_body,authId=auth_id,authBase=auth_base,sharedPlan=None)
  auth_id=str(uuid.uuid4())
  op=operation(vpn,1,payload('vpn',1),payload('vpn_auth',0),auth_id)
  assert call(db,'api_vpn_write_prepare',leader,json.dumps(op))['status']=='prepared'
  first=call(db,'api_vpn_write_apply',leader,op['operationId']);assert first['status']=='applied' and first['revision']==2 and first['authRevision']==1
  assert call(db,'api_vpn_write_receipt',leader,op['operationId'])==first
  assert call(db,'api_vpn_write_prepare',leader,json.dumps(op))==first
  assert call(db,'api_vpn_write_apply',leader,op['operationId'])==first
  changed={**op,'config':{**op['config'],'encryptedSecret':'aa'*80}}
  denied(lambda:call(db,'api_vpn_write_prepare',leader,json.dumps(changed)))
  denied(lambda:call(db,'api_vpn_write_prepare',member_token,json.dumps(op)))
  assert call(db,'api_private_credential_get',leader,vpn)['revision']==2
  bad=operation(vpn,2,payload('vpn',2),payload('password',1),auth_id,1)
  call(db,'api_vpn_write_prepare',leader,json.dumps(bad));assert call(db,'api_vpn_write_apply',leader,bad['operationId'])['status']=='conflict'
  assert call(db,'api_private_credential_get',leader,vpn)['revision']==2
  assert call(db,'api_private_credential_get',leader,auth_id)['revision']==1
  new_id=str(uuid.uuid4());new=operation(new_id,0,payload('vpn',0),cap=authority)
  assert call(db,'api_vpn_write_prepare',leader,json.dumps(new))['status']=='prepared'
  assert call(db,'api_vpn_write_apply',leader,new['operationId'])['revision']==1
  missing=operation(str(uuid.uuid4()),0,payload('vpn',0))
  denied(lambda:call(db,'api_vpn_write_prepare',leader,json.dumps(missing)))
  missing.pop('config');denied(lambda:call(db,'api_vpn_write_prepare',leader,json.dumps(missing)))
  stale=operation(vpn,2,payload('vpn',2));call(db,'api_vpn_write_prepare',leader,json.dumps(stale))
  sql(db,'SET ROLE '+OWNER+';UPDATE termterm_team_v2.teams SET acl_revision=acl_revision+1 WHERE id='+quoted(b['teamId']))
  assert call(db,'api_vpn_write_apply',leader,stale['operationId'])['status']=='conflict'
  assert call(db,'api_private_credential_get',leader,vpn)['revision']==2
  stale=operation(vpn,2,payload('vpn',2));call(db,'api_vpn_write_prepare',leader,json.dumps(stale))
  sql(db,'SET ROLE '+OWNER+';UPDATE termterm_team_v2.team_recovery_keys SET active=false WHERE team_id='+quoted(b['teamId'])+';INSERT INTO termterm_team_v2.team_recovery_keys(team_id,public_key) VALUES('+quoted(b['teamId'])+",decode('"+'fe'*32+"','hex'));")
  assert call(db,'api_vpn_write_apply',leader,stale['operationId'])['status']=='conflict'
  assert call(db,'api_private_credential_get',leader,vpn)['revision']==2
  print('PASS: Team VPN '+('clean66' if baseline==0 else '65-to-66 retained records')+' account isolation, independent copy, private auth, ACL, bindings, root/hop, revoke, atomic config/auth, durable replay, authority, stale ACL/recovery',flush=True)
if __name__=='__main__':main()
