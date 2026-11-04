import {readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const {rpc,close}=helper(),report=[];
try{for(const p of JSON.parse(await readFile('.lab/database/profiles.json','utf8'))){
  if(p.caPath)p.caPath=path.resolve('.lab/ca.crt');
  const opened=await rpc({op:'open',profile:p});
  try{
    const sql='SELECT CAST(:city AS '+(p.engine==='oracle'?'NVARCHAR2(100)':p.engine==='mssql'?'NVARCHAR(100)':'text')+') AS city'+(p.engine==='oracle'?' FROM dual':'');
    const result=await rpc({op:'query',sessionId:opened.sessionId,sql,parameters:{city:'İstanbul — Linux'}});
    assert.equal(result.results[0].rows[0][0],'İstanbul — Linux');
    assert.ok((await rpc({op:'schemas',sessionId:opened.sessionId})).columns.length);
    report.push({engine:p.engine,version:opened.version,passed:['linux-connect','unicode-bind','metadata']});
  }finally{await rpc({op:'close',sessionId:opened.sessionId});}
}}finally{close();}
await writeFile('artifacts/database-workspace/linux-helper-tests.json',JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
