"""Real Linux service -> verified-TLS PostgreSQL test, behind a disposable TLS proxy.
The Python proxy is a test fixture only. Production deployment uses Caddy.
"""
import http.client,http.server,json,os,pathlib,socket,ssl,subprocess,threading,time,uuid,sys
from team_v2_accounts import ROOT,call,sql,lit,PW
LAB=ROOT/'.lab/team-v2-http'
def desktop_fixture(cert):
    b=call('bootstrap_team','Masaüstü Team laboratuvarı '+uuid.uuid4().hex[:6],'leader','Takım lideri',admin=True)
    fixture=LAB/'windows-native.json'
    fixture.write_text(json.dumps({'profile':{'serviceUrl':'https://localhost:41842','teamId':b['teamId'],'caPem':cert.read_text()},'username':'leader','password':b['temporaryPassword']}));fixture.chmod(0o600)
    return fixture
def main():
    LAB.mkdir(parents=True,exist_ok=True)
    key=LAB/'localhost.key';cert=LAB/'localhost.crt'
    if '--renew-desktop' in sys.argv:
        desktop_fixture(cert);print('New disposable desktop fixture ready');return
    expired=not cert.exists() or subprocess.run(['openssl','x509','-checkend','3600','-noout','-in',str(cert)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode!=0
    if not key.exists() or expired:
        subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(key),'-out',str(cert),'-days','2',
        '-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        key.chmod(0o600)
    class Proxy(http.server.BaseHTTPRequestHandler):
        def log_message(self,*args):pass
        def do_GET(self):self.forward()
        def do_POST(self):self.forward()
        def do_PUT(self):self.forward()
        def forward(self):
            upstream=http.client.HTTPConnection('127.0.0.1',41841,timeout=25)
            try:
                n=int(self.headers.get('Content-Length',0));body=self.rfile.read(n)
                headers={k:v for k,v in self.headers.items() if k.lower() not in ['host','connection','content-length']}
                upstream.request(self.command,self.path,body,headers);r=upstream.getresponse();data=r.read()
                self.send_response(r.status)
                for k,v in r.getheaders():
                    if k.lower() not in ['content-length','transfer-encoding','connection']:self.send_header(k,v)
                self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
            finally:upstream.close()
    server=http.server.ThreadingHTTPServer(('127.0.0.1',41842),Proxy)
    context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(cert,key)
    server.socket=context.wrap_socket(server.socket,server_side=True)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    retention=call('bootstrap_team','Automatic retention fixture '+uuid.uuid4().hex,'leader','Retention leader',admin=True)['teamId']
    sql("INSERT INTO termterm_team_v2.team_maintenance_state(team_id,next_due) VALUES("+lit(retention)+",'-infinity'); INSERT INTO termterm_team_v2.audit_events(team_id,action,outcome,server_at) VALUES("+lit(retention)+",'fixture.expired','succeeded',now()-interval '100 days')",True)
    proc=subprocess.Popen(['/root/termterm-team-v2-build/team-service/target/debug/termterm-team-service',str(ROOT/'.lab/team-v2-service.json')],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
    trusted=ssl.create_default_context(cafile=str(cert))
    def request(path,body=None,token=None,method=None,ctx=trusted,host='localhost',client_version='0.4.1',api_version='1'):
        c=http.client.HTTPSConnection(host,41842,context=ctx,timeout=30)
        try:
            headers={'Content-Type':'application/json'}
            if client_version is not None:headers['X-TermTerm-Client-Version']=client_version
            if api_version is not None:headers['X-TermTerm-API-Version']=api_version
            if token:headers['Authorization']='Bearer '+token
            c.request(method or ('GET' if body is None else 'POST'),path,None if body is None else json.dumps(body),headers)
            r=c.getresponse();data=r.read();return r.status,dict(r.getheaders()),json.loads(data) if data else None
        finally:c.close()
    try:
        for _ in range(100):
            if proc.poll() is not None:raise RuntimeError('Service exited before becoming ready')
            try:
                c=http.client.HTTPConnection('127.0.0.1',41841,timeout=.1);c.request('GET','/health');c.getresponse().read();c.close();break
            except OSError:time.sleep(.05)
        assert request('/ready')[0]==200
        assert request('/client-policy')[2]['minimumClientVersion']=='0.4.1'
        for version,api in [(None,None),('0.4.0','1'),('0.4.1-dev.2','1'),('not-semver','1'),('0.4.1','2')]:
            status,headers,body=request('/v1/auth/login',{},client_version=version,api_version=api)
            assert status==426 and body['error']=='team_update_required' and headers.get('cache-control')=='no-store'
        print('PASS: HTTPS client compatibility rejects missing/old/invalid/API-mismatched versions before login.',flush=True)
        for _ in range(50):
            if sql("SELECT last_completed IS NOT NULL FROM termterm_team_v2.team_maintenance_state WHERE team_id="+lit(retention),True)=='t':break
            time.sleep(.1)
        else:raise AssertionError('Automatic retention worker did not complete its startup batch')
        assert sql("SELECT count(*) FROM termterm_team_v2.audit_events WHERE team_id="+lit(retention)+" AND action='fixture.expired'",True)=='0'
        assert request('/v1/maintenance/tick',{})[0]==404

        if '--desktop' in sys.argv:
            fixture=None if '--service-only' in sys.argv else desktop_fixture(cert);stop=LAB/'desktop.stop';stop.unlink(missing_ok=True)
            print('Desktop TLS fixture ready; private data under .lab/team-v2-http',flush=True)
            try:
                while not stop.exists():time.sleep(.5)
            finally:
                if fixture:fixture.unlink(missing_ok=True)
                stop.unlink(missing_ok=True)
            return
        for ctx,host in [(ssl.create_default_context(),'localhost'),(trusted,'127.0.0.1')]:
            try:request('/health',ctx=ctx,host=host)
            except ssl.SSLCertVerificationError:pass
            else:raise AssertionError('Untrusted CA/wrong hostname unexpectedly accepted')
        assert request('/v1/me')[0]==401
        assert request('/v1/register',{})[0]==404
        b=call('bootstrap_team','HTTP disposable '+uuid.uuid4().hex,'leader','HTTP leader',admin=True)
        login=dict(teamId=b['teamId'],username='leader',password=b['temporaryPassword'],deviceId=str(uuid.uuid4()),deviceName='HTTP lab')
        status,headers,a=request('/v1/auth/login',login);assert status==200 and headers.get('cache-control')=='no-store'
        assert a['activationRequired']
        assert request('/v1/members',token=a['accessToken'])[0]==401
        assert request('/v1/auth/activate',dict(password=PW,publicKey='ab'*32,encryptedPrivateKey='cd'*128),a['accessToken'])[0]==200
        login['password']=PW;status,_,a=request('/v1/auth/login',login);assert status==200
        status,_,m=request('/v1/members',dict(username='api-user',displayName='API üyesi',role='operator'),a['accessToken']);assert status==200
        assert len(m['temporaryPassword'])==21
        assert len(request('/v1/members',token=a['accessToken'])[2])==2
        # Request-supplied actor/team spoofing is rejected before reaching SQL.
        assert request('/v1/members',dict(username='forged',displayName='forged',role='operator',actorId=str(uuid.uuid4())),a['accessToken'])[0]==422
        assert request('/v1/auth/logout',{},a['accessToken'])[0]==200
        assert request('/v1/me',token=a['accessToken'])[0]==401
        # Real Rust HTTPS client + Argon2/AEAD/sealed keys, separate accounts/devices.
        bootstrap=call('bootstrap_team','Rust client disposable '+uuid.uuid4().hex,'leader','Rust leader',admin=True)
        fixture=LAB/'rust-client.json'
        fixture.write_text(json.dumps({'profile':{'serviceUrl':'https://localhost:41842','teamId':bootstrap['teamId'],'caPem':cert.read_text()},
            'username':'leader','password':bootstrap['temporaryPassword']}));fixture.chmod(0o600)
        env=os.environ.copy();env['TERMTERM_TEAM_V2_FIXTURE']=str(fixture)
        env['PATH']='/root/.cargo/bin:'+env['PATH']
        try:
            subprocess.run(['cargo','test','--locked','--manifest-path','team-client/Cargo.toml','--test','live_service','--','--ignored'],
                cwd='/root/termterm-team-v2-build',env=env,check=True)
        finally:fixture.unlink(missing_ok=True)
        print('PASS: Linux Rust service HTTPS/auth/member endpoints -> PostgreSQL 18 TLS; trusted CA, wrong CA/hostname, no-store, forged actor, revoked session.')
    finally:
        proc.terminate()
        try:proc.wait(timeout=5)
        except subprocess.TimeoutExpired:proc.kill();proc.wait()
        server.shutdown();server.server_close();thread.join(timeout=2)
if __name__=='__main__':main()
