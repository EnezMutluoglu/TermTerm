import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import path from 'node:path';
export function helper({timeoutMs=65000}={}){
 const root=path.resolve(process.env.TERMTERM_DB_HELPER||'database-helper'),pending=new Map();let seq=0;
 const child=spawn(path.join(root,'runtime/bin/java'+(process.platform==='win32'?'.exe':'')),['-cp',[path.join(root,'out'),path.join(root,'lib/*')].join(process.platform==='win32'?';':':'),'Main'],{stdio:['pipe','pipe','pipe'],windowsHide:true});
 createInterface({input:child.stdout}).on('line',line=>{const r=JSON.parse(line),p=pending.get(r.id);if(p){pending.delete(r.id);clearTimeout(p.timer);r.error?p.reject(Object.assign(new Error(r.error.message),r.error)):p.resolve(r.result);}});
 child.on('exit',()=>{for(const p of pending.values()){clearTimeout(p.timer);p.reject(new Error('Helper exited'));}pending.clear();});
 return {close:()=>child.stdin.end(),rpc:r=>new Promise((resolve,reject)=>{const id=String(++seq);const timer=setTimeout(()=>{pending.delete(id);reject(new Error('Test RPC timeout'));},timeoutMs);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({...r,id})+'\n');})};
}
