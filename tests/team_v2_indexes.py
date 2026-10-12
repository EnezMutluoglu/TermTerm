"""Real EXPLAIN ANALYZE over disposable 10k-host fixture, no planner toggles."""
import json,pathlib,uuid
from team_v2_accounts import bootstrap,sql,lit,ROOT
def walk(plan):
    yield plan
    for child in plan.get('Plans',[]):yield from walk(child)
def main():
    b,leader=bootstrap('query plans');team=b['teamId'];account=b['accountId'];vault=str(uuid.uuid4());folder=str(uuid.uuid4())
    sql("INSERT INTO termterm_team_v2.vaults(team_id,id,encrypted_name) VALUES("+lit(team)+','+lit(vault)+",decode('abcd','hex'));"+
        "INSERT INTO termterm_team_v2.resources(team_id,vault_id,id,kind) VALUES("+','.join(map(lit,[team,vault,folder,'folder']))+");"+
        "INSERT INTO termterm_team_v2.folders VALUES("+','.join(map(lit,[team,vault,folder]))+",NULL,decode('abcd','hex'));"+
        "WITH new_hosts AS (INSERT INTO termterm_team_v2.resources(team_id,vault_id,id,kind) SELECT "+lit(team)+','+lit(vault)+",gen_random_uuid(),'host' FROM generate_series(1,10000) RETURNING *) "+
        "INSERT INTO termterm_team_v2.hosts SELECT team_id,vault_id,id,CASE WHEN row_number() over()<=100 THEN "+lit(folder)+"::uuid END,'ssh',22,'personal',decode('abcd','hex'),decode('abcd','hex'),NULL FROM new_hosts;"+
        "INSERT INTO termterm_team_v2.resource_acl SELECT team_id,id,"+lit(account)+",'connect','allow' FROM termterm_team_v2.hosts WHERE team_id="+lit(team)+";"+
        "INSERT INTO termterm_team_v2.audit_events(team_id,actor_id,action,outcome,server_at) SELECT "+lit(team)+','+lit(account)+",'fixture','succeeded',now()-n*interval '1 second' FROM generate_series(1,10000)n;"+
        "ANALYZE termterm_team_v2.hosts; ANALYZE termterm_team_v2.resource_acl; ANALYZE termterm_team_v2.audit_events;",True)
    credential=str(uuid.uuid4());recovery=str(uuid.uuid4())
    sql("INSERT INTO termterm_team_v2.resource_versions(team_id,resource_id,revision,encrypted_snapshot,actor_id) SELECT team_id,id,1,decode(repeat('ab',80),'hex'),"+lit(account)+" FROM termterm_team_v2.hosts WHERE team_id="+lit(team)+";"+
        "INSERT INTO termterm_team_v2.revision_pins(team_id,resource_id,revision,account_id,purpose) SELECT team_id,id,1,"+lit(account)+",'temporary' FROM termterm_team_v2.hosts WHERE team_id="+lit(team)+";"+
        "INSERT INTO termterm_team_v2.credentials(team_id,id,visibility,owner_account_id,kind) VALUES("+lit(team)+','+lit(credential)+",'private',"+lit(account)+",'password');"+
        "INSERT INTO termterm_team_v2.team_recovery_keys(team_id,id,public_key) VALUES("+lit(team)+','+lit(recovery)+",decode(repeat('ab',32),'hex'));"+
        "INSERT INTO termterm_team_v2.credential_update_plans(team_id,actor_id,credential_id,expected_revision,scope_id,scope_revision,acl_revision,recovery_id,expires_at) SELECT "+','.join(map(lit,[team,account,credential]))+",1,"+lit(folder)+",1,1,"+lit(recovery)+",now()+(100-n)*interval '1 second' FROM generate_series(1,10000)n;"+
        "ANALYZE termterm_team_v2.revision_pins; ANALYZE termterm_team_v2.credential_update_plans;",True)
    move=str(uuid.uuid4())
    sql("INSERT INTO termterm_team_v2.resource_move_plans(team_id,id,actor_id,vault_id,resource_id,source_revision,acl_revision,vault_revision,identity_hash,route_hash) VALUES("+
        ','.join(map(lit,[team,move,account,vault,folder]))+",1,1,1,decode('ab','hex'),decode('cd','hex'));"+
        "INSERT INTO termterm_team_v2.resource_move_plans(team_id,actor_id,vault_id,resource_id,source_revision,acl_revision,vault_revision,identity_hash,route_hash,expires_at) SELECT "+
        ','.join(map(lit,[team,account,vault,folder]))+",1,1,1,decode('ab','hex'),decode('cd','hex'),now()+(100-n)*interval '1 second' FROM generate_series(1,10000)n;"+
        "INSERT INTO termterm_team_v2.move_plan_keys(team_id,plan_id,ordinal,kind,object_id,revision,account_id,identity_version,source_envelope) SELECT "+
        ','.join(map(lit,[team,move]))+",n,'resource',"+lit(folder)+",1,"+lit(account)+",1,decode(repeat('ab',128),'hex') FROM generate_series(1,10000)n;"+
        "INSERT INTO termterm_team_v2.move_access_changes(team_id,plan_id,resource_id,account_id,before_permissions,after_permissions) SELECT team_id,"+lit(move)+",id,"+lit(account)+",ARRAY['read'],ARRAY[]::text[] FROM termterm_team_v2.hosts WHERE team_id="+lit(team)+";"+
        "ANALYZE termterm_team_v2.resource_move_plans; ANALYZE termterm_team_v2.move_plan_keys; ANALYZE termterm_team_v2.move_access_changes;",True)
    cases={
      'pending_moves':("SELECT id FROM termterm_team_v2.resource_move_plans WHERE team_id="+lit(team)+' AND actor_id='+lit(account)+" AND status='pending' AND expires_at>now() ORDER BY expires_at LIMIT 20",{'move_plans_pending'}),
      'move_key_cursor':("SELECT ordinal FROM termterm_team_v2.move_plan_keys WHERE team_id="+lit(team)+' AND plan_id='+lit(move)+" AND ordinal>5000 ORDER BY ordinal LIMIT 20",{'move_plan_keys_pkey'}),
      'move_impact_page':("SELECT resource_id,account_id FROM termterm_team_v2.move_access_changes WHERE team_id="+lit(team)+' AND plan_id='+lit(move)+" ORDER BY resource_id,account_id OFFSET 100 LIMIT 100",{'move_access_changes_pkey'}),
      'pin_cursor':("SELECT id FROM termterm_team_v2.revision_pins WHERE team_id="+lit(team)+" AND id>0 ORDER BY id LIMIT 20",{'revision_pin_id'}),
      'pending_shared_updates':("SELECT id FROM termterm_team_v2.credential_update_plans WHERE team_id="+lit(team)+' AND actor_id='+lit(account)+" AND status='pending' AND expires_at>now() ORDER BY expires_at LIMIT 20",{'credential_updates_pending'}),
      'folder_hosts':("SELECT id FROM termterm_team_v2.hosts WHERE team_id="+lit(team)+' AND vault_id='+lit(vault)+' AND folder_id='+lit(folder),{'hosts_folder'}),
      'member_acl':("SELECT effect FROM termterm_team_v2.resource_acl WHERE team_id="+lit(team)+' AND account_id='+lit(account)+" AND permission='connect' ORDER BY resource_id LIMIT 100",{'acl_account','resource_acl_pkey'}),
      'audit_cursor':("SELECT action FROM termterm_team_v2.audit_events WHERE team_id="+lit(team)+" AND id<9223372036854775807 ORDER BY id DESC LIMIT 100",{'audit_team_cursor','audit_events_pkey'}),
      'audit_history':("SELECT action FROM termterm_team_v2.audit_events WHERE team_id="+lit(team)+" ORDER BY server_at DESC,id DESC LIMIT 20",{'audit_team_time'}),
    }
    report={}
    for name,(query,expected) in cases.items():
        result=json.loads(sql('EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) '+query,True))[0]
        indexes={node.get('Index Name') for node in walk(result['Plan'])}
        assert indexes & expected,(name,indexes)
        report[name]={'executionMs':result['Execution Time'],'planningMs':result['Planning Time'],'indexes':sorted(x for x in indexes if x),'rows':result['Plan']['Actual Rows']}
    (ROOT/'.lab/team-v2-index-results.json').write_text(json.dumps(report,indent=2))
    print('PASS: real 10,000-host EXPLAIN ANALYZE uses folder, member ACL and audit indexes. '+json.dumps(report))
if __name__=='__main__':main()
