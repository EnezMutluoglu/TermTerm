import {readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),report=[];
try {for(const p of JSON.parse(await readFile('.lab/database/profiles.json','utf8')).filter(p=>p.engine!=='postgresql')){
 const profile={...p,host:'localhost',port:p.engine==='oracle'?41522:p.port,tls:'verify-full',caPath:path.resolve('.lab/database/tls/ca.pem')};
 const tests=[];let sessionId;
 try {
  ({sessionId}=await rpc({op:'open',profile}));await rpc({op:'query',sessionId,sql:p.engine==='oracle'?'SELECT 1 FROM dual':'SELECT 1'});await rpc({op:'close',sessionId});sessionId=null;tests.push('custom-ca-positive');
  await assert.rejects(()=>rpc({op:'open',profile:{...profile,caPath:path.resolve('.lab/ca.crt')}}));tests.push('wrong-ca-rejected');
  await assert.rejects(()=>rpc({op:'open',profile:{...profile,host:'wrong-host.invalid',transportHost:'127.0.0.1',transportPort:profile.port,serverDn:'CN=wrong-host.invalid'}}));tests.push('wrong-server-identity-rejected');
  ({sessionId}=await rpc({op:'open',profile:{...profile,transportHost:'127.0.0.1',transportPort:profile.port,serverDn:'CN=localhost'}}));tests.push('logical-host-preserved-over-relay');
  report.push({engine:p.engine,status:'passed',tests});
 }catch(e){report.push({engine:p.engine,status:'failed',tests,error:e.message.replaceAll(p.password,'[redacted]')});}
 finally {if(sessionId)await rpc({op:'close',sessionId}).catch(()=>{});}
}} finally {close();}
await writeFile('artifacts/database-workspace/tls-tests.json',JSON.stringify({date:new Date().toISOString(),report},null,2));console.log(JSON.stringify(report,null,2));process.exitCode=report.some(r=>r.status==='failed')?1:0;
