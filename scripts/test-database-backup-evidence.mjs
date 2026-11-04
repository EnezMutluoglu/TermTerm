import {readFile,writeFile} from 'node:fs/promises';
import {randomBytes} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import assert from 'node:assert/strict';
import {helper} from './database-rpc.mjs';
const profile=JSON.parse(await readFile('.lab/database/profiles.json','utf8')).find(p=>p.engine==='mssql');
const {rpc,close}=helper(),sessions=[],tests=[],name='tt_ev_backup_'+randomBytes(5).toString('hex'),restored=name+'_restored';
const files=['bak','diff','trn'].map(ext=>`/var/opt/mssql/data/${name}.${ext}`);
const opened=async database=>{const r=await rpc({op:'open',profile:{...profile,database}});sessions.push(r.sessionId);return r.sessionId;};
const admin=await opened('master'),q=(sql,sessionId=admin)=>rpc({op:'query',sessionId,sql,timeout:30});let created=false,didRestore=false,error;
try{
 await q(`CREATE DATABASE [${name}]`);created=true;await q(`ALTER DATABASE [${name}] SET RECOVERY FULL`);
 const id=await opened(name);await q('CREATE TABLE dbo.restore_probe(id INT PRIMARY KEY,value INT);INSERT INTO dbo.restore_probe VALUES(1,42)',id);
 for(const [i,kind] of ['DATABASE','DATABASE','LOG'].entries())await q(`BACKUP ${kind} [${name}] TO DISK=N'${files[i]}' WITH INIT,CHECKSUM${i===1?',DIFFERENTIAL':''}`);
 const e=await rpc({op:'evidence',sessionId:id,panel:'backups'}),part=e.parts.find(p=>p.id==='backups'),names=part.columns.map(c=>c.name.toLowerCase()),rows=part.rows.map(r=>Object.fromEntries(names.map((n,i)=>[n,r[i]])));
 assert.equal(part.status,'available');assert.deepEqual(rows.map(r=>r.type).sort(),['D','I','L']);assert.ok(rows.every(r=>Number(r.age_seconds)>=0&&Number(r.age_seconds)<120&&r.has_backup_checksums==='1'));tests.push('real-full-differential-log-backups-and-server-age');
 const listing=(await q(`RESTORE FILELISTONLY FROM DISK=N'${files[0]}'`)).results[0];
 const logicalIndex=listing.columns.findIndex(c=>c.name==='LogicalName'),typeIndex=listing.columns.findIndex(c=>c.name==='Type');
 const moves=listing.rows.map((r,i)=>`MOVE N'${r[logicalIndex].replaceAll("'","''")}' TO N'/var/opt/mssql/data/${restored}_${i}.${r[typeIndex]==='L'?'ldf':'mdf'}'`);
 await q(`RESTORE DATABASE [${restored}] FROM DISK=N'${files[0]}' WITH ${moves.join(',')},RECOVERY`);didRestore=true;
 const restoredId=await opened(restored);assert.equal((await q('SELECT value FROM dbo.restore_probe',restoredId)).results[0].rows[0][0],'42');
 const r=await rpc({op:'evidence',sessionId:restoredId,panel:'backups'});assert.equal(r.parts.find(p=>p.id==='restores').status,'available');assert.ok(r.parts.find(p=>p.id==='restores').rows.length>0);tests.push('real-disposable-restore-and-visible-history');
 // Deliberately absent destination must not appear as a successful backup.
 await assert.rejects(()=>q(`BACKUP DATABASE [${name}] TO DISK=N'/termterm_missing_${name}/failed.bak' WITH INIT`));
 const after=await rpc({op:'evidence',sessionId:id,panel:'backups'});assert.equal(after.parts.find(p=>p.id==='backups').rows.length,3);assert.equal(after.parts.find(p=>p.id==='failures').status,'unsupported');tests.push('failed-backup-not-promoted-to-success-complete-failure-log-unavailable');
}catch(e){error=e.message.replaceAll(profile.password,'[redacted]');}
finally{
 for(const sessionId of sessions.filter(s=>s!==admin))await rpc({op:'close',sessionId}).catch(()=>{});
 for(const [db,exists] of [[restored,didRestore],[name,created]])if(exists)await q(`ALTER DATABASE [${db}] SET SINGLE_USER WITH ROLLBACK IMMEDIATE;DROP DATABASE [${db}]`).catch(()=>{});
 await rpc({op:'close',sessionId:admin}).catch(()=>{});close();
 if(files.every(f=>f.startsWith('/var/opt/mssql/data/tt_ev_backup_')&&!f.includes('..'))){const args=['docker','exec','termterm-db-mssql','rm','-f','--',...files];if(process.platform==='linux')execFileSync(args[0],args.slice(1),{stdio:'pipe'});else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--',...args],{stdio:'pipe',windowsHide:true});}
}
const report={date:new Date().toISOString(),engine:'mssql',status:error?'failed':'passed',tests,...error&&{error}};await writeFile('artifacts/database-workspace/backup-evidence-tests.json',JSON.stringify(report,null,2));console.log(report);process.exitCode=error?1:0;
