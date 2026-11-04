import { readFile, writeFile } from 'node:fs/promises';
import { randomBytes } from 'node:crypto';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { helper } from './database-rpc.mjs';
const { rpc, close } = helper(), report = [];
try { for (const original of JSON.parse(await readFile('.lab/database/profiles.json', 'utf8'))) {
 const profile = { ...original }; if (process.platform === 'linux' && profile.caPath) profile.caPath = path.resolve('.lab/ca.crt');
 const engine = profile.engine, schema = engine === 'postgresql' ? 'public' : engine === 'mssql' ? 'dbo' : 'SYSTEM', table = 'tt_insert_' + randomBytes(4).toString('hex');
 const quote = n => engine === 'mssql' ? '[' + n + ']' : '"' + n + '"', target = quote(schema) + '.' + quote(table), name = n => engine === 'oracle' ? n.toUpperCase() : n, tests = [], sessions = [];
 const open = async () => { const id = (await rpc({ op: 'open', profile })).sessionId; sessions.push(id); return id; }; let owner;
 try {
  owner = await open(); const other = await open(), q = (sql, sessionId = owner) => rpc({ op: 'query', sessionId, sql });
  const identity = engine === 'mssql' ? 'BIGINT IDENTITY(1,1)' : engine === 'oracle' ? 'NUMBER GENERATED ALWAYS AS IDENTITY' : 'BIGINT GENERATED ALWAYS AS IDENTITY';
  await q(`CREATE TABLE ${target}(id ${identity} PRIMARY KEY,label ${engine === 'oracle' ? 'NVARCHAR2' : engine === 'mssql' ? 'NVARCHAR' : 'VARCHAR'}(100) DEFAULT 'default value' NOT NULL,amount DECIMAL(28,4) CHECK(amount>=0), note ${engine === 'oracle' ? 'VARCHAR2' : 'VARCHAR'}(100))`);
  const meta = await rpc({ op: 'rowInsertMetadata', sessionId: owner, schema, table }); assert.equal(meta.columns.find(c => c.name === name('id')).generated, true); tests.push('identity-and-default-metadata');
  const preview = (values = {}, sessionId = owner) => rpc({ op: 'rowInsertPreview', sessionId, schema, table, values: Object.fromEntries(Object.entries(values).map(([k, v]) => [name(k), v])) });
  const apply = (p, sessionId = owner) => rpc({ op: 'rowInsertApply', sessionId, planId: p.planId, hash: p.hash });
  const count = async () => (await q(`SELECT COUNT(*) FROM ${target}`)).results[0].rows[0][0];
  let p = await preview({}); assert.equal(await count(), '0'); let r = await apply(p); assert.equal(r.state, 'succeeded', JSON.stringify(r)); assert.ok(r.keys[name('id')]); assert.equal(r.row[name('label')], 'default value'); assert.equal(r.row[name('amount')], null); assert.equal((await apply(p)).state, 'failed'); tests.push('all-defaults-generated-key-verified-single-use');
  const label = "İstanbul'; DROP TABLE fake; --"; p = await preview({ label, amount: '9007199254740993.1234', note: null }); assert.ok(!p.sql.includes(label)); assert.equal(await count(), '1');
  r = await apply(p); assert.equal(r.state, 'succeeded', JSON.stringify(r)); assert.equal(r.row[name('label')], label); assert.equal(r.row[name('amount')], '9007199254740993.1234'); tests.push('unicode-bound-injection-literal-decimal-null');
  await assert.rejects(() => preview({ id: '77' })); await assert.rejects(() => preview({ amount: 42 })); await assert.rejects(() => preview({ amount: '0.00001' })); await assert.rejects(() => preview({ label: null })); tests.push('identity-override-number-rounding-and-nonnull-rejected');
  p = await preview({ amount: '-1' }); r = await apply(p); assert.equal(r.state, 'failed'); assert.equal(r.rolledBack, true); assert.equal(await count(), '2'); tests.push('constraint-error-rollback');
  p = await preview({ note: 'discard' }); await rpc({ op: 'rowInsertDiscard', sessionId: owner, planId: p.planId }); assert.equal((await apply(p)).state, 'failed'); tests.push('discard-invalidates');
  p = await preview({}); assert.equal((await apply(p, other)).state, 'failed'); assert.equal((await apply({ ...p, hash: 'bad' })).state, 'failed'); await assert.rejects(() => rpc({ op: 'rowInsertApply', sessionId: owner, planId: p.planId, hash: p.hash, sql: 'INSERT' })); tests.push('session-hash-renderer-sql-rejected');
  await q(`ALTER TABLE ${target} ADD extra_value INT`); assert.equal((await apply(p)).state, 'conflict'); tests.push('ddl-change-conflict');
  const hold = quote(schema) + '.' + quote(table + '_hold'); await q(`CREATE TABLE ${hold}(id INT PRIMARY KEY)`);
  try { await rpc({ op: 'transaction', sessionId: owner, action: 'autocommit', enabled: false }); await q(`INSERT INTO ${hold}(id) VALUES(1)`); p = await preview({ note: 'independent' }); assert.equal((await apply(p)).state, 'succeeded'); assert.equal((await rpc({ op: 'status', sessionId: owner })).transaction, true); await rpc({ op: 'transaction', sessionId: owner, action: 'rollback' }); await rpc({ op: 'transaction', sessionId: owner, action: 'autocommit', enabled: true }); assert.equal((await q(`SELECT COUNT(*) FROM ${hold}`)).results[0].rows[0][0], '0'); } finally { await q(`DROP TABLE ${hold}`); } tests.push('editor-transaction-isolation');
  p = await preview({}); await rpc({ op: 'transaction', sessionId: other, action: 'autocommit', enabled: false }); await q(`UPDATE ${target} SET note='locked'`, other); const start = Date.now(); r = await apply(p); assert.equal(r.state, 'failed'); assert.equal(r.rolledBack, true); assert.ok(Date.now() - start < 20000); await rpc({ op: 'transaction', sessionId: other, action: 'rollback' }); await rpc({ op: 'transaction', sessionId: other, action: 'autocommit', enabled: true }); tests.push('bounded-lock-wait');
  const manual = quote(schema) + '.' + quote(table + '_manual'); await q(`CREATE TABLE ${manual}(id DECIMAL(20,0) PRIMARY KEY, label ${engine === 'oracle' ? 'VARCHAR2' : 'VARCHAR'}(100) NOT NULL)`);
  try { await assert.rejects(() => rpc({ op: 'rowInsertPreview', sessionId: owner, schema, table: table + '_manual', values: {} })); const v = { [name('id')]: '9007199254740993', [name('label')]: 'manual' }; p = await rpc({ op: 'rowInsertPreview', sessionId: owner, schema, table: table + '_manual', values: v }); r = await apply(p); assert.equal(r.state, 'succeeded', JSON.stringify(r)); assert.equal(r.keys[name('id')], '9007199254740993'); p = await rpc({ op: 'rowInsertPreview', sessionId: owner, schema, table: table + '_manual', values: v }); assert.equal((await apply(p)).state, 'failed'); } finally { await q(`DROP TABLE ${manual}`); } tests.push('manual-large-pk-required-fields-duplicate-rollback');
  const trigger = quote(schema) + '.' + quote(table + '_tr');
  if (engine === 'postgresql') { await q(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'`); await q(`CREATE TRIGGER tt_insert_guard BEFORE INSERT ON ${target} FOR EACH ROW EXECUTE FUNCTION ${trigger}()`); }
  else if (engine === 'mssql') await q(`CREATE TRIGGER ${trigger} ON ${target} AFTER INSERT AS SELECT 1`);
  else await q(`CREATE TRIGGER ${trigger} BEFORE INSERT ON ${target} BEGIN NULL; END;`);
  await assert.rejects(() => preview(), /Trigger/);
  if (engine === 'postgresql') { await q(`DROP TRIGGER tt_insert_guard ON ${target}`); await q(`DROP FUNCTION ${trigger}()`); } else await q(`DROP TRIGGER ${trigger}`); tests.push('trigger-rejected');
  {
   const user='tt_insert_reader_'+randomBytes(5).toString('hex'),password='Tt9!'+randomBytes(20).toString('hex');let reader;
   try{
    if(engine==='postgresql'){
     const sqlFile=path.resolve(`.lab/database/row-insert-role-${user}.sql`);await writeFile(sqlFile,`CREATE ROLE ${user} LOGIN PASSWORD '${password}';`);
     if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-v','ON_ERROR_STOP=1','-f',sqlFile],{stdio:'pipe'});
     else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-v','ON_ERROR_STOP=1','-f',`/mnt/c/Users/sonx/Desktop/termterm/.lab/database/row-insert-role-${user}.sql`],{stdio:'pipe',windowsHide:true});
    }else if(engine==='mssql'){await q(`CREATE LOGIN ${user} WITH PASSWORD='${password}'`);await q(`CREATE USER ${user} FOR LOGIN ${user}`);}
    else {await q(`CREATE USER ${user} IDENTIFIED BY "${password}"`);await q(`GRANT CREATE SESSION TO ${user}`);}
    await q(`GRANT SELECT ON ${target} TO ${user}`);reader=(await rpc({op:'open',profile:{...profile,username:user,password}})).sessionId;
    await assert.rejects(()=>preview({},reader));assert.equal(await count(),'3');tests.push('select-only-account-insert-denied');
   }finally{if(reader)await rpc({op:'close',sessionId:reader}).catch(()=>{});if(engine==='postgresql'){
     const sqlFile=path.resolve(`.lab/database/row-insert-role-${user}.sql`);await writeFile(sqlFile,`DROP OWNED BY ${user};DROP ROLE ${user};`);
     if(process.platform==='linux')execFileSync('runuser',['-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',sqlFile],{stdio:'pipe'});
     else execFileSync('wsl',['-d','Ubuntu-24.04','-u','root','--','runuser','-u','postgres','--','psql','-p','55432','-d','termterm_sql_lab','-v','ON_ERROR_STOP=1','-f',`/mnt/c/Users/sonx/Desktop/termterm/.lab/database/row-insert-role-${user}.sql`],{stdio:'pipe',windowsHide:true});
    }else {await q(`DROP USER ${user}`).catch(()=>{});if(engine==='mssql')await q(`DROP LOGIN ${user}`).catch(()=>{});}}
  }

  await rpc({ op: 'close', sessionId: other }); await assert.rejects(() => preview({}, other)); tests.push('closed-session-rejection');
  report.push({ engine, status: 'passed', tests });
 } catch (e) { report.push({ engine, status: 'failed', tests, error: e.message.replaceAll(profile.password, '[redacted]') }); }
 finally { for (const sessionId of sessions) { await rpc({ op: 'transaction', sessionId, action: 'rollback' }).catch(() => {}); await rpc({ op: 'transaction', sessionId, action: 'autocommit', enabled: true }).catch(() => {}); } if (owner) await rpc({ op: 'query', sessionId: owner, sql: `DROP TABLE ${target}` }).catch(() => {}); if(owner && engine==='postgresql') await rpc({op:'query',sessionId:owner,sql:`DROP FUNCTION IF EXISTS ${quote(schema)+'.'+quote(table+'_tr')}()`}).catch(()=>{}); for (const sessionId of sessions) await rpc({ op: 'close', sessionId }).catch(() => {}); }
} } finally { close(); }
await writeFile(`artifacts/database-workspace/${process.platform === 'linux' ? 'linux-' : ''}row-insertion-tests.json`, JSON.stringify({ date: new Date().toISOString(), report }, null, 2)); console.log(JSON.stringify(report, null, 2)); process.exitCode = report.some(r => r.status !== 'passed') ? 1 : 0;
