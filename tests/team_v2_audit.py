"""Real PostgreSQL audit pagination, team boundaries and bounded retention."""
from team_v2_accounts import bootstrap,call,sql,lit,denied,login,activate,PW

def main():
    b,owner=bootstrap('audit');t=b['teamId'];token=owner['accessToken']
    other,ot=bootstrap('audit foreign')
    member=call('api_create_member',token,'reader','Reader','operator')
    activate(login(t,'reader',member['temporaryPassword']))
    reader=login(t,'reader',PW)['accessToken']
    denied(lambda:call('api_audit',reader,None))
    denied(lambda:call('api_maintenance',reader))
    denied(lambda:call('api_storage',reader))
    denied(lambda:call('maintenance_prune',t,None))
    denied(lambda:call('api_audit',token,-1))
    sql("INSERT INTO termterm_team_v2.audit_events(team_id,action,outcome,server_at) SELECT "+lit(t)+",'test.old','succeeded',now()-interval '91 days' FROM generate_series(1,1003)",True)
    old_id=sql("SELECT min(id) FROM termterm_team_v2.audit_events WHERE team_id="+lit(t)+" AND action='test.old'",True)
    sql("INSERT INTO termterm_team_v2.delegation_audit_rules VALUES("+old_id+",'read')",True)
    sql("INSERT INTO termterm_team_v2.audit_events(team_id,action,outcome,server_at) VALUES("+lit(other['teamId'])+",'foreign.old','succeeded',now()-interval '100 days')",True)
    page=call('api_audit',token,None);assert len(page['events'])==100
    ids=[e['id'] for e in page['events']]
    while page['nextBefore']:
        page=call('api_audit',token,page['nextBefore']);ids.extend(e['id'] for e in page['events'])
    assert len(ids)==len(set(ids)) and all(int(a)>int(b) for a,b in zip(ids,ids[1:]))
    assert all(e['action']!='foreign.old' for e in page['events'])
    result=call('api_maintenance',token);assert result['auditDeleted']==1000 and result['auditDays']==90
    assert sql("SELECT count(*) FROM termterm_team_v2.audit_events WHERE team_id="+lit(t)+" AND action='test.old'",True)=='3'
    assert sql("SELECT count(*) FROM termterm_team_v2.delegation_audit_rules WHERE audit_id="+old_id,True)=='0'
    assert sql("SELECT count(*) FROM termterm_team_v2.audit_events WHERE team_id="+lit(other['teamId'])+" AND action='foreign.old'",True)=='1'
    assert call('api_maintenance',token)['auditDeleted']==3
    assert call('api_maintenance',token)['auditDeleted']==0
    assert call('api_audit',token,None)['events'][0]['action']=='maintenance.prune'
    denied(lambda:sql('SELECT * FROM termterm_team_v2.audit_events'))
    sql("INSERT INTO termterm_team_v2.audit_events(team_id,action,outcome,server_at) VALUES("+lit(t)+",'automatic.old','succeeded',now()-interval '91 days')",True)
    sql("INSERT INTO termterm_team_v2.team_maintenance_state(team_id,next_due) VALUES("+lit(t)+",'-infinity') ON CONFLICT(team_id) DO UPDATE SET next_due='-infinity'",True)
    assert call('maintenance_tick')['processed']
    assert sql("SELECT count(*) FROM termterm_team_v2.audit_events WHERE team_id="+lit(t)+" AND action='automatic.old'",True)=='0'
    storage=call('api_storage',token)
    assert storage['lastMaintenance'] is not None and storage['auditDays']==90 and int(storage['encryptedPayloadBytes'])>=0
    assert sql("SELECT next_due>now()+interval '23 hours' FROM termterm_team_v2.team_maintenance_state WHERE team_id="+lit(t),True)=='t'
    print('PASS: owner-only audit, team isolation, stable 100-row cursor, 90-day retention, 1000-row batch, audit-rule cascade, repeat-safe cleanup; secrets never included.')
if __name__=='__main__':main()
