import {mkdir,cp,readdir,writeFile,readFile} from 'node:fs/promises';
import {existsSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import path from 'node:path';
if(process.platform!=='win32')throw Error('This script prepares the Windows development package');
const output=path.resolve('artifacts/database-workspace/TermTerm-0.5.0-dev.12-windows'),helper=path.join(output,'database-helper');
await mkdir(helper,{recursive:true});
await cp('src-tauri/target/debug/termterm.exe',path.join(output,'TermTerm-Database-Dev.exe'));
for(const dir of ['out','lib'])await cp('database-helper/'+dir,path.join(helper,dir),{recursive:true,filter:src=>!/(?:SqlScriptTest|RowEditorTest|ChangePlansTest|ProvisioningTest).*\.class$/.test(src)});
if(!existsSync(path.join(helper,'runtime'))){
 const jlink=spawnSync(path.resolve('database-helper/runtime/bin/jlink.exe'),['--add-modules','java.base,java.sql,java.sql.rowset,java.naming,java.management,java.security.jgss,java.net.http,jdk.crypto.ec,jdk.charsets,jdk.unsupported','--strip-debug','--no-man-pages','--no-header-files','--compress=zip-6','--output',path.join(helper,'runtime')],{stdio:'inherit',windowsHide:true});
 if(jlink.status!==0)throw Error('Runtime creation failed');
}
if(!existsSync('src-tauri/resources/mosh/bin/mosh-client.exe'))throw Error('Mosh runtime missing; run node scripts/fetch-mosh.mjs before packaging');
await cp('src-tauri/resources/mosh',path.join(output,'mosh'),{recursive:true});
await cp('docs/THIRD_PARTY.md',path.join(output,'THIRD_PARTY.md'));
await cp('docs/licenses',path.join(output,'licenses'),{recursive:true});
await writeFile(path.join(output,'BASLAT.txt'),'TermTerm Database 0.5.0-dev.12\r\nTermTerm-Database-Dev.exe dosyasını çalıştırın. Ayrı geliştirme uygulamasıdır; kararlı güncelleme kaynağı kapalıdır.\r\nJava/JDBC ve Mosh çalışma dosyaları pakete dahildir. Windows WebView2 gerekir.\r\nKendi kişisel test kasanızı oluşturun. Özel laboratuvar parolaları ve kasalar pakete konulmaz.\r\nBu paket tüm 216 yönetim işleminin tamamlandığı anlamına gelmez; DATABASE_WORKSPACE_STATUS.md dosyasını okuyun.\r\n');
for(const name of ['DATABASE_WORKSPACE_STATUS.md','DATABASE_CATALOG_VALIDATION.md','DATABASE_TEST_RESULTS.md','DATABASE_PROVISIONING.md','DATABASE_TEAM.md','PROJECT_STATUS_2026-10-05.md'])if(existsSync('docs/'+name))await cp('docs/'+name,path.join(output,name));
await cp('migrations/team',path.join(output,'migrations/team'),{recursive:true});
const manifest=[];
async function inventory(dir){for(const entry of await readdir(dir,{withFileTypes:true})){const file=path.join(dir,entry.name);if(entry.isDirectory())await inventory(file);else if(entry.name!=='manifest.json'){const data=await readFile(file);manifest.push({path:path.relative(output,file).replaceAll('\\','/'),bytes:data.length,sha256:createHash('sha256').update(data).digest('hex')});}}}
await inventory(output);await writeFile(path.join(output,'manifest.json'),JSON.stringify(manifest,null,2));
console.log(JSON.stringify({directory:output,files:manifest.length,bytes:manifest.reduce((n,f)=>n+f.bytes,0)}));
