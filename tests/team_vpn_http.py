"""Fresh PostgreSQL + verified HTTPS + real Rust VPN encryption/cache/outbox test.

All credentials and TLS keys stay in a new private disposable fixture directory.
The loopback control server changes only this test proxy, never the installed app.
"""
import hashlib,http.client,http.server,json,os,re,secrets,socket,ssl,struct,subprocess,threading,time,uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
PRIVATE=Path('/var/lib/termterm-team-upgrade-tests')
OWNER='termterm_team_upgrade_owner';SERVICE='termterm_team_upgrade_service'
CA='/etc/postgresql/18/termterm/tls/ca.crt'
def run(args,**kw):
 p=subprocess.run(args,capture_output=True,text=True,**kw)
 if p.returncode:raise RuntimeError('Disposable VPN fixture command failed: '+args[0])
 return p.stdout.strip()
def sql(db,text):return run(['runuser','-u','postgres','--','psql','-X','-qAt','-v','ON_ERROR_STOP=1','-p','55432','-d',db],input=text)
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def main():
 assert os.geteuid()==0
 case=PRIVATE/('vpn-crypto-'+uuid.uuid4().hex);case.mkdir(mode=0o700,parents=True)
 passwords=json.loads((PRIVATE/'roles.json').read_text())
 db='termterm_vpn_crypto_'+uuid.uuid4().hex[:10]
 sql('postgres',f'CREATE DATABASE {db} OWNER {OWNER}')
 sql(db,f'REVOKE ALL ON DATABASE {db} FROM PUBLIC;GRANT CONNECT ON DATABASE {db} TO {SERVICE}')
 pgpass=case/'pgpass';pgpass.write_text(f'localhost:55432:*:{OWNER}:{passwords[OWNER]}\n');pgpass.chmod(0o600)
 env={**os.environ,'PGHOST':'localhost','PGPORT':'55432','PGDATABASE':db,'PGUSER':OWNER,'PGSSLROOTCERT':CA,'PGPASSFILE':str(pgpass),'PATH':'/root/.cargo/bin:'+os.environ['PATH']}
 run(['python3',str(ROOT/'team-service/deploy/manage.py'),'migrate','--service-role',SERVICE],env=env)
 b=json.loads(sql(db,"SET ROLE "+OWNER+";SELECT termterm_team_v2.bootstrap_team('Disposable VPN crypto','leader','VPN leader')"))
 backend,tls_port,control_port=port(),port(),port();assert len({backend,tls_port,control_port})==3
 cfg=case/'service.json';cfg.write_text(json.dumps({'listen':f'127.0.0.1:{backend}','database':{'host':'localhost','port':55432,'database':db,'username':SERVICE,'password':passwords[SERVICE],'caPath':CA}}));cfg.chmod(0o600)
 key=case/'localhost.key';cert=case/'localhost.crt'
 run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(key),'-out',str(cert),'-days','2','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost']);key.chmod(0o600)
 tls=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);tls.load_cert_chain(cert,key)
 state={'applyRequests':0,'dropReply':False,'proxy':None,'thread':None};lock=threading.Lock()
 class Proxy(http.server.BaseHTTPRequestHandler):
  def log_message(self,*args):pass
  def do_GET(self):self.forward()
  def do_POST(self):self.forward()
  def do_PUT(self):self.forward()
  def forward(self):
   upstream=http.client.HTTPConnection('127.0.0.1',backend,timeout=25)
   try:
    count=int(self.headers.get('Content-Length',0));assert count<=4*1024*1024
    body=self.rfile.read(count);headers={k:v for k,v in self.headers.items() if k.lower() not in ['host','connection','content-length']}
    applying=self.command=='POST' and re.fullmatch(r'/v1/vpn/operations/[0-9a-f-]{36}',self.path)
    if applying:state['applyRequests']+=1
    upstream.request(self.command,self.path,body,headers);response=upstream.getresponse();data=response.read()
    if applying and state['dropReply'] and response.status==200:
     state['dropReply']=False;self.close_connection=True;self.connection.setsockopt(socket.SOL_SOCKET,socket.SO_LINGER,struct.pack('ii',1,0));self.connection.close();return
    self.send_response(response.status)
    for k,v in response.getheaders():
     if k.lower() not in ['content-length','transfer-encoding','connection']:self.send_header(k,v)
    self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
   finally:upstream.close()
 def start_proxy():
  server=http.server.ThreadingHTTPServer(('127.0.0.1',tls_port),Proxy);server.socket=tls.wrap_socket(server.socket,server_side=True)
  thread=threading.Thread(target=lambda:server.serve_forever(poll_interval=.05),daemon=True);thread.start();state['proxy']=server;state['thread']=thread
 def stop_proxy():
  if state['proxy'] is not None:state['proxy'].shutdown();state['proxy'].server_close();state['thread'].join(timeout=3);state['proxy']=None;state['thread']=None
 class Control(http.server.BaseHTTPRequestHandler):
  def log_message(self,*args):pass
  def do_POST(self):
   with lock:
    if self.path=='/offline':stop_proxy()
    elif self.path=='/online':
     if state['proxy'] is None:start_proxy()
    elif self.path=='/drop-reply':state['dropReply']=True
    elif self.path!='/stats':self.send_error(404);return
    data=json.dumps({'ok':True,'applyRequests':state['applyRequests']}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 controller=http.server.ThreadingHTTPServer(('127.0.0.1',control_port),Control);control_thread=threading.Thread(target=controller.serve_forever,daemon=True);control_thread.start()
 service=Path(os.environ.get('TEAM_SERVICE_BINARY','/root/termterm-vpn-team-build/debug/termterm-team-service'))
 process=subprocess.Popen([str(service),str(cfg)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 fixture=case/'rust-fixture.json'
 fixture.write_text(json.dumps({'profile':{'serviceUrl':f'https://localhost:{tls_port}','teamId':b['teamId'],'caPem':cert.read_text()},'username':'leader','password':b['temporaryPassword'],'controlUrl':f'http://127.0.0.1:{control_port}'}));fixture.chmod(0o600)
 try:
  for _ in range(100):
   assert process.poll() is None,'Fixture service exited before readiness'
   try:
    c=http.client.HTTPConnection('127.0.0.1',backend,timeout=.2);c.request('GET','/ready');response=c.getresponse();ready=response.status==200;response.read();c.close()
    if ready:break
   except OSError:pass
   time.sleep(.1)
  else:raise AssertionError('Fixture service not ready')
  start_proxy();env['TERMTERM_TEAM_VPN_FIXTURE']=str(fixture)
  result=subprocess.run(['/root/.cargo/bin/cargo','test','--locked','--manifest-path',str(ROOT/'team-client/Cargo.toml'),'--target-dir','/root/termterm-vpn-client-build','--test','vpn_service','--','--ignored','--nocapture'],env=env)
  assert result.returncode==0,'Real encrypted VPN lifecycle test failed'
  report={'result':'passed','schema':66,'cache':9,'test':'encrypted_team_vpn_lifecycle_and_offline_replay','migrationSha256':hashlib.sha256((ROOT/'team-service/migrations/066_vpn.sql').read_bytes()).hexdigest(),'realTunnels':False}
  (case/'public-report.json').write_text(json.dumps(report,indent=2));print('PASS: real HTTPS/AEAD Team VPN, independent copy/ACL, lost apply reply, offline edit/restart/replay, explicit revoke purge',flush=True)
 finally:
  stop_proxy();controller.shutdown();controller.server_close();control_thread.join(timeout=3);process.terminate()
  try:process.wait(timeout=5)
  except subprocess.TimeoutExpired:process.kill();process.wait()
  fixture.unlink(missing_ok=True)
if __name__=='__main__':main()
