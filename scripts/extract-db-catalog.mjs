import {readFile,writeFile,mkdir} from 'node:fs/promises';
const source=await readFile('CODEX_DB_WORKSPACE_SQL_KATALOGU.md','utf8');
const blocks=[...source.matchAll(/^### ((?:PG|MS|OR)-(?:PROV|MON|OPS|CLOUD)-\d+) — (.+)\r?\n([\s\S]*?)(?=^### |^## |^# |$(?![\s\S]))/gm)];
const actions=blocks.map(([,id,title,body])=>{
 const metadata={};const yaml=body.match(/```yaml\s+([\s\S]*?)```/)?.[1]??'';
 for(const line of yaml.split(/\r?\n/)){const m=line.match(/^(\w+):\s*(.*)$/);if(m)metadata[m[1]]=m[2];}
 const field=name=>body.match(new RegExp(`\\*\\*${name}:\\*\\*\\s*([^\\r\\n]+)`))?.[1]??'';
 return {id,title,...metadata,catalogVersion:1,sql:[...body.matchAll(/```sql\s+([\s\S]*?)```/g)].map(m=>m[1].trim()),target:field('Hedef'),permissions:field('Yetki'),inputs:field('Girdi'),explanation:field('Yorum / kullanım'),preconditions:field('Ön koşul / güvenlik'),sources:field('Kaynak'),status:'unverified'};
});
if(actions.length!==216||new Set(actions.map(a=>a.id)).size!==216)throw new Error(`Expected 216 unique actions, got ${actions.length}`);
const overrides=JSON.parse(await readFile('src/database/catalogOverrides.json','utf8'));
for(const a of actions)if(overrides[a.id]){a.sql=overrides[a.id];a.catalogVersion=2;}
await mkdir('src/database',{recursive:true});await writeFile('src/database/catalog.json',JSON.stringify(actions,null,2)+'\n');
console.log(JSON.stringify({actions:actions.length,engines:Object.fromEntries(['postgresql','mssql','oracle'].map(e=>[e,actions.filter(a=>a.engine===e).length]))}));
