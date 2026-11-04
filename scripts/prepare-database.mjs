import { mkdir, writeFile, readFile, readdir, rename } from 'node:fs/promises';
import { existsSync, createWriteStream } from 'node:fs';
import { pipeline } from 'node:stream/promises';
import { Readable } from 'node:stream';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
const root = path.resolve(process.env.TERMTERM_DB_HELPER || 'database-helper');
await mkdir(path.join(root,'lib'),{recursive:true});
async function download(url,file) {
  const response=await fetch(url); if(!response.ok) throw new Error(`${response.status}: ${url}`);
  await pipeline(Readable.fromWeb(response.body),createWriteStream(file));
}
const deps=[['org/postgresql','postgresql','42.7.8'],['com/microsoft/sqlserver','mssql-jdbc','13.2.1.jre11'],['com/oracle/database/jdbc','ojdbc11','23.9.0.25.07'],['com/google/code/gson','gson','2.13.2']];
const hashes=[];
for(const [group,name,version] of deps){
  const file=`${name}-${version}.jar`,url=`https://repo.maven.apache.org/maven2/${group}/${name}/${version}/${file}`,dest=path.join(root,'lib',file);
  if(!existsSync(dest)) await download(url,dest);
  const data=await readFile(dest); const expected=await (await fetch(url+'.sha1')).text();
  if(createHash('sha1').update(data).digest('hex')!==expected.trim().split(/\s/)[0])throw new Error('Dependency checksum mismatch: '+file);
  hashes.push({file,url,sha256:createHash('sha256').update(data).digest('hex')});
}
const os=process.platform==='win32'?'windows':process.platform==='darwin'?'mac':'linux';
const arch=process.arch==='arm64'?'aarch64':'x64';
const runtime=path.join(root,'runtime');
if(!existsSync(path.join(runtime,'bin',os==='windows'?'java.exe':'java'))){
  const assets=await(await fetch(`https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=${arch}&image_type=jdk&os=${os}&vendor=eclipse`)).json();
  const pkg=assets[0].binary.package; const archive=path.join(root,os==='windows'?'runtime.zip':'runtime.tar.gz');
  if(!existsSync(archive))await download(pkg.link,archive);
  if(createHash('sha256').update(await readFile(archive)).digest('hex')!==pkg.checksum)throw new Error('JDK checksum mismatch');
  const unpack=path.join(root,'runtime-unpack');await mkdir(unpack,{recursive:true});
  const result=spawnSync('tar',['xf',archive,'-C',unpack],{stdio:'inherit'});
  if(result.status!==0)throw new Error('JDK unpack failed');
  const dir=(await readdir(unpack))[0]; await rename(path.join(unpack,dir,os==='mac'?'Contents/Home':''),runtime);
}
await writeFile(path.join(root,'lib','checksums.json'),JSON.stringify(hashes,null,2));
await mkdir(path.join(root,'out'),{recursive:true});
const sourceRoot=path.resolve('database-helper/src');
const sources=(await readdir(sourceRoot)).filter(f=>f.endsWith('.java')).map(f=>path.join(sourceRoot,f));
const result=spawnSync(path.join(runtime,'bin',os==='windows'?'javac.exe':'javac'),['-encoding','UTF-8','-cp',path.join(root,'lib','*'),'-d',path.join(root,'out'),...sources],{stdio:'inherit'});
if(result.status!==0)process.exit(result.status??1);
console.log('Database helper ready (Java 21, PostgreSQL / SQL Server / Oracle).');
