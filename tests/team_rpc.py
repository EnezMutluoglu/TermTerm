"""Second real PostgreSQL client for native revocation tests; no credentials in output."""
import sys,json,uuid
from team_postgres import sql,lit
request=json.load(sys.stdin)
assert request['action'] in ['member_set','overview','acl_set']
account=json.loads(sql("SELECT termterm_team.login_account(%s,%s,%s,'Native test second client')"%(lit(request['login']),lit(request['password']),lit(uuid.uuid4()))))
assert 'accessToken' in account, 'Second client authentication failed'
try:
    result=sql("SELECT termterm_team.rpc(%s,%s,%s::jsonb)"%(lit(account['accessToken']),lit(request['action']),lit(json.dumps(request.get('body',{})))))
    print(result)
finally:
    sql("SELECT termterm_team.rpc(%s,'logout','{}')"%lit(account['accessToken']))
