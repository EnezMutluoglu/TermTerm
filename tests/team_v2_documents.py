"""Real PostgreSQL document isolation/CAS/relations. Synthetic ciphertext, not crypto evidence."""
import datetime,json,uuid
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW
from team_v2_personal import blob

def main():
 b,owner=bootstrap('documents');t=b['teamId'];token=owner['accessToken'];v=str(uuid.uuid4());seq=0
 m=call('api_create_member',token,'docs','Document operator','operator');activate(login(t,'docs',m['temporaryPassword']));member=login(t,'docs',PW)['accessToken']
 call('api_vault_create',token,v,blob(),1,blob())
 folder,host,snippet,ws=[str(uuid.uuid4()) for _ in range(4)]
 def body(r,kind,parent=None,expected=0,targets=None):
  nonlocal seq
  seq+=1;p=call('api_resource_recipients',token,v,r,parent)
  return dict(operationId=str(uuid.uuid4()),sequence=seq,vaultId=v,resourceId=r,expectedRevision=expected,
   aclRevision=p['aclRevision'],writeCapability=p['writeCapability'],clientAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind=kind,parentId=parent,deleted=False,
   protocol='ssh' if kind=='host' else None,port=22 if kind=='host' else None,usernamePolicy='personal' if kind=='host' else None,
   encryptedName=blob()[2:],encryptedAddress=blob()[2:] if kind=='host' else None,encryptedUsername=None,
   encryptedDocument=blob()[2:] if kind in ('snippet','workspace') else None,workspaceHostIds=targets,snapshot=blob()[2:],
   envelopes=[dict(accountId=a['accountId'],identityVersion=a['identityVersion'],envelope='ab'*128) for a in p['recipients']])
 def save(b):return call('api_resource_save',token,json.dumps(b))
 for r,k,p,ids in [(folder,'folder',None,None),(host,'host',folder,None),(snippet,'snippet',folder,None),(ws,'workspace',folder,[host])]:assert save(body(r,k,p,targets=ids))['status']=='applied'
 a=body(snippet,'snippet',folder,1);result=save(a);replay=save(a);assert replay["status"]==result["status"] and replay["revision"]==result["revision"]
 assert len(call('api_resource_history',token,snippet))==2
 assert call('api_resource_get',token,ws)['workspaceHostIds']==[host]
 assert {x['id'] for x in call('api_resources',token,v,None,100)['records']}=={folder,host,snippet,ws}
 denied(lambda:call('api_resource_get',member,snippet))
 sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,folder,m['accountId'],'read','allow']))+')',True)
 assert call('api_resource_get',member,snippet)['permissions']==['read']
 denied(lambda:call('api_resource_recipients',member,v,snippet,folder))
 # Workspace read cannot authorize its targets.
 denied(lambda:call('api_connection_plan',member,host))
 sql('INSERT INTO termterm_team_v2.resource_acl VALUES('+','.join(map(lit,[t,snippet,m['accountId'],'read','deny']))+')',True)
 denied(lambda:call('api_resource_get',member,snippet))
 assert {x['id'] for x in call('api_resources',member,v,None,100)['records']}=={folder,host,ws}
 for targets in [[],[host,host],[str(uuid.uuid4())],None]:denied(lambda targets=targets:save(body(ws,'workspace',folder,1,targets)))
 for r,k in [(folder,'folder'),(host,'host')]:
  d=body(r,k,None if k=='folder' else folder,1);d['deleted']=True;denied(lambda d=d:save(d))
 # Other-team host IDs and folder IDs cannot be referenced; no partial revision on failure.
 other,otherowner=bootstrap('documents-other');ov=str(uuid.uuid4());call('api_vault_create',otherowner['accessToken'],ov,blob(),1,blob())
 bad=body(ws,'workspace',folder,1,[host]);bad['parentId']=str(uuid.uuid4());denied(lambda:save(bad))
 assert call('api_resource_get',token,ws)['revision']==1
 for _ in range(12):
  rev=call('api_resource_get',token,snippet)['revision'];save(body(snippet,'snippet',folder,rev))
 assert len(call('api_resource_history',token,snippet))==11
 for table in ['snippets','workspaces','workspace_entries']:denied(lambda table=table:sql('SELECT * FROM termterm_team_v2.'+table))
 print('PASS typed documents: lists, inherited/explicit ACL, target non-authority, replay, bounded history, relations, atomic rejection and direct SQL denial')
if __name__=='__main__':main()
