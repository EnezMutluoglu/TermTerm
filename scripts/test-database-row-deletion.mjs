import { readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import path from 'node:path';
import assert from 'node:assert/strict';
import { helper } from './database-rpc.mjs';
const { rpc, close } = helper(), report = [];
try { for (const original of JSON.parse(await readFile('.lab/database/profiles.json', 'utf8'))) {
 const profile = { ...original }; if (process.platform === 'linux' && profile.caPath) profile.caPath = path.resolve('.lab/ca.crt');
 const engine = profile.engine, schema = engine === 'postgresql' ? 'public' : engine === 'mssql' ? 'dbo' : 'SYSTEM';
 const table = 'tt_delete_' + randomBytes(4).toString('hex'), quote = n => engine === 'mssql' ? '[' + n + ']' : '"' + n + '"', target = quote(schema) + '.' + quote(table), child = quote(schema) + '.' + quote(table + '_child');
 const tests = [], sessions = []; let owner;
 const open = async () => { const s = (await rpc({ op: 'open', profile })).sessionId; sessions.push(s); return s; };
 try {
  owner = await open(); const second = await open(), q = (sql, sessionId = owner) => rpc({ op: 'query', sessionId, sql });
  await q(`CREATE TABLE ${target}(id DECIMAL(20,0) PRIMARY KEY,label ${engine === 'oracle' ? 'NVARCHAR2' : engine === 'mssql' ? 'NVARCHAR' : 'VARCHAR'}(100),note INT)`);
  await q(`INSERT INTO ${target}(id,label) VALUES(9007199254740993,${engine === 'mssql' ? 'N' : ''}'İstanbul')`); await q(`INSERT INTO ${target}(id,label) VALUES(2,'keep')`);
  const key = engine === 'oracle' ? 'ID' : 'id', preview = (id = '9007199254740993', sessionId = owner) => rpc({ op: 'rowDeletePreview', sessionId, schema, table, keys: { [key]: id } });
  const apply = (p, sessionId = owner) => rpc({ op: 'rowDeleteApply', sessionId, planId: p.planId, hash: p.hash });
  const count = async () => (await q(`SELECT COUNT(*) FROM ${target}`)).results[0].rows[0][0];
  let p = await preview(); assert.equal(p.original[key], '9007199254740993'); assert.equal(p.original[engine === 'oracle' ? 'LABEL' : 'label'], 'İstanbul'); assert.equal(p.original[engine === 'oracle' ? 'NOTE' : 'note'], null); assert.equal(await count(), '2'); assert.ok(p.sql.includes('?')); tests.push('read-only-preview-exact-large-key-null');
  assert.equal((await apply(p, second)).state, 'failed'); assert.equal((await apply({ ...p, hash: 'forged' })).state, 'failed');
  await assert.rejects(() => rpc({ op: 'rowDeleteApply', sessionId: owner, planId: p.planId, hash: p.hash, sql: 'DELETE FROM x' })); tests.push('cross-session-hash-and-renderer-sql-rejected');
  await q(`UPDATE ${target} SET note=7 WHERE id=9007199254740993`, second); assert.equal((await apply(p)).state, 'conflict'); assert.equal(await count(), '2'); tests.push('concurrent-edit-not-deleted');
  p = await preview(); await rpc({ op: 'rowDeleteDiscard', sessionId: owner, planId: p.planId }); assert.equal((await apply(p)).state, 'failed'); tests.push('discard-invalidates-plan');
  p = await preview(); await q(`ALTER TABLE ${target} ADD extra_value INT`); assert.equal((await apply(p)).state, 'conflict'); tests.push('schema-change-invalidates-plan');
  p = await preview(); await rpc({ op: 'transaction', sessionId: second, action: 'autocommit', enabled: false }); await q(`UPDATE ${target} SET note=8 WHERE id=2`, second);
  const started = Date.now(), locked = await apply(p); assert.equal(locked.state, 'failed', JSON.stringify(locked)); assert.equal(locked.rolledBack, true); assert.ok(Date.now() - started < 20000);
  await rpc({ op: 'transaction', sessionId: second, action: 'rollback' }); await rpc({ op: 'transaction', sessionId: second, action: 'autocommit', enabled: true }); tests.push('bounded-table-lock-wait-and-rollback');
  await q(`CREATE TABLE ${child}(id INT PRIMARY KEY,parent_id DECIMAL(20,0) REFERENCES ${target}(id) ON DELETE CASCADE)`);
  await assert.rejects(() => preview(), /CASCADE/); await q(`DROP TABLE ${child}`); tests.push('empty-cascade-relationship-blocked-before-write');
  await q(`CREATE TABLE ${child}(id INT PRIMARY KEY,parent_id DECIMAL(20,0) REFERENCES ${target}(id))`); await q(`INSERT INTO ${child}(id,parent_id) VALUES(1,9007199254740993)`);
  p = await preview(); const fk = await apply(p); assert.equal(fk.state, 'failed'); assert.equal(fk.rolledBack, true); assert.equal(await count(), '2'); await q(`DROP TABLE ${child}`); tests.push('restrictive-foreign-key-failure-preserves-row');
  const trigger = quote(schema) + '.' + quote(table + '_tr');
  if (engine === 'postgresql') { await q(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN OLD; END'`); await q(`CREATE TRIGGER tt_delete_guard BEFORE DELETE ON ${target} FOR EACH ROW EXECUTE FUNCTION ${trigger}()`); }
  else if (engine === 'mssql') await q(`CREATE TRIGGER ${trigger} ON ${target} AFTER DELETE AS SELECT 1`);
  else await q(`CREATE TRIGGER ${trigger} BEFORE DELETE ON ${target} BEGIN NULL; END;`);
  await assert.rejects(() => preview(), /Trigger/);
  if (engine === 'postgresql') { await q(`DROP TRIGGER tt_delete_guard ON ${target}`); await q(`DROP FUNCTION ${trigger}()`); } else await q(`DROP TRIGGER ${trigger}`); tests.push('trigger-rejected');
  {
   const user='tt_delete_reader_'+randomBytes(5).toString('hex'),password='Tt9!'+randomBytes(20).toString('hex');let reader;
   try{
    if(engine==='postgresql'){
     const sqlFile=path.resolve(`.lab/database/row-delete-role-${user}.sql`);await writeFile(sqlFile,`CREATE ROLE ${user} LOGIN PASSWORD '${password}';`);
     if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-v','ON_ERROR_STOP=1','-f',sqlFile],{stdio:'pipe'});
     else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-v','ON_ERROR_STOP=1','-f',`/mnt/c/Users/sonx/Desktop/termterm/.lab/database/row-delete-role-${user}.sql`],{stdio:'pipe',windowsHide:true});
    }else if(engine==='mssql'){await q(`CREATE LOGIN ${user} WITH PASSWORD='${password}'`);await q(`CREATE USER ${user} FOR LOGIN ${user}`);}
    else {await q(`CREATE USER ${user} IDENTIFIED BY "${password}"`);await q(`GRANT CREATE SESSION TO ${user}`);}
    await q(`GRANT SELECT ON ${target} TO ${user}`);reader=(await rpc({op:'open',profile:{...profile,username:user,password}})).sessionId;
    await assert.rejects(()=>preview('9007199254740993',reader));assert.equal(await count(),'2');tests.push('select-only-account-delete-denied');
   }finally{if(reader)await rpc({op:'close',sessionId:reader}).catch(()=>{});if(engine==='postgresql'){
     const sqlFile=path.resolve(`.lab/database/row-delete-role-${user}.sql`);await writeFile(sqlFile,`DROP OWNED BY ${user};DROP ROLE ${user};`);
     if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',sqlFile],{stdio:'pipe'});
     else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',`/mnt/c/Users/sonx/Desktop/termterm/.lab/database/row-delete-role-${user}.sql`],{stdio:'pipe',windowsHide:true});
    }else {await q(`DROP USER ${user}`).catch(()=>{});if(engine==='mssql')await q(`DROP LOGIN ${user}`).catch(()=>{});}}
  }

  // Editor transaction on another table must not be committed by the independent delete connection.
  await q(`CREATE TABLE ${child}(id INT PRIMARY KEY)`); await rpc({ op: 'transaction', sessionId: owner, action: 'autocommit', enabled: false }); await q(`INSERT INTO ${child}(id) VALUES(1)`);
  p = await preview(); const result = await apply(p); assert.equal(result.state, 'succeeded', JSON.stringify(result)); assert.equal(result.affectedRows, 1);
  assert.equal((await rpc({ op: 'status', sessionId: owner })).transaction, true); await rpc({ op: 'transaction', sessionId: owner, action: 'rollback' }); await rpc({ op: 'transaction', sessionId: owner, action: 'autocommit', enabled: true });
  assert.equal((await q(`SELECT COUNT(*) FROM ${child}`)).results[0].rows[0][0], '0'); await q(`DROP TABLE ${child}`); assert.equal(await count(), '1'); assert.equal((await apply(p)).state, 'failed'); tests.push('one-row-delete-editor-transaction-isolated-replay-rejected');
  p = await preview('2'); await q(`DELETE FROM ${target} WHERE id=2`, second); assert.equal((await apply(p)).state, 'conflict'); tests.push('deleted-row-conflict');
  await rpc({ op: 'close', sessionId: second }); await assert.rejects(() => preview('2', second)); tests.push('closed-session-rejected');
  report.push({ engine, status: 'passed', tests });
 } catch (e) { report.push({ engine, status: 'failed', tests, error: e.message.replaceAll(profile.password, '[redacted]') }); }
 finally {
  for (const sessionId of sessions) { await rpc({ op: 'transaction', sessionId, action: 'rollback' }).catch(() => {}); await rpc({ op: 'transaction', sessionId, action: 'autocommit', enabled: true }).catch(() => {}); }
  if (owner) for (const sql of [`DROP TABLE ${child}`, `DROP TABLE ${target}`, ...(engine === 'postgresql' ? [`DROP FUNCTION ${quote(schema)}.${quote(table + '_tr')}()`] : [])]) await rpc({ op: 'query', sessionId: owner, sql }).catch(() => {});
  for (const sessionId of sessions) await rpc({ op: 'close', sessionId }).catch(() => {});
 }
} } finally { close(); }
await writeFile(`artifacts/database-workspace/${process.platform === 'linux' ? 'linux-' : ''}row-deletion-tests.json`, JSON.stringify({ date: new Date().toISOString(), report }, null, 2));
console.log(JSON.stringify(report, null, 2)); process.exitCode = report.some(r => r.status === 'failed') ? 1 : 0;
