// Resolve dependencies from the original Mach-O files before relocating them.
// Reading LC_RPATH from copied files loses @loader_path's original meaning.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

if (process.platform !== 'darwin') throw Error('Run on the target Mac architecture');
const [clientArgument, runtimeArgument] = process.argv.slice(2);
const client = fs.realpathSync(clientArgument), runtime = path.resolve(runtimeArgument);
const run = (command, args) => execFileSync(command, args, { encoding: 'utf8' });
const executableDir = path.dirname(client);
const expand = (name, source) => name
  .replace(/^@loader_path(?=\/|$)/, path.dirname(source))
  .replace(/^@executable_path(?=\/|$)/, executableDir);
const systemLibrary = name => name.startsWith('/usr/lib/') || name.startsWith('/System/Library/');
const names = new Map();
const queue = [{ source: client, destination: path.join(runtime, 'bin/mosh-client'), inherited: [] }];
for (let index = 0; index < queue.length; index++) {
  const { source, destination, inherited } = queue[index];
  if (fs.existsSync(destination)) fs.chmodSync(destination, 0o755);
  fs.copyFileSync(source, destination);
  fs.chmodSync(destination, 0o755);
  const commands = run('otool', ['-l', source]);
  const rpaths = [...commands.matchAll(/cmd LC_RPATH\s+cmdsize \d+\s+path (.*?) \(offset \d+\)/g)]
    .map(match => expand(match[1], source));
  const search = [...rpaths, ...inherited];
  const id = run('otool', ['-D', source]).split('\n').slice(1).map(line => line.trim()).filter(Boolean)[0];
  const dependencies = run('otool', ['-L', source]).split('\n').slice(1)
    .map(line => line.trim().replace(/ \(compatibility version .*$/, '')).filter(Boolean);
  for (const dependency of dependencies) {
    if (dependency === id || systemLibrary(dependency)) continue;
    const candidates = dependency.startsWith('@rpath/')
      ? search.map(directory => path.join(directory, dependency.slice(7)))
      : [expand(dependency, source)];
    const found = candidates.find(candidate => !candidate.startsWith('@') && fs.existsSync(candidate));
    if (!found) throw Error(`Unresolved ${dependency} from ${source}; searched ${candidates.join(', ')}`);
    const resolved = fs.realpathSync(found);
    if (systemLibrary(resolved)) continue;
    const basename = path.basename(dependency), previous = names.get(basename);
    if (previous && previous !== resolved) throw Error(`Conflicting library name ${basename}`);
    if (!previous) {
      names.set(basename, resolved);
      queue.push({ source: resolved, destination: path.join(runtime, 'lib', basename), inherited: search });
    }
    run('install_name_tool', ['-change', dependency, `@executable_path/../lib/${basename}`, destination]);
  }
  if (id) run('install_name_tool', ['-id', `@executable_path/../lib/${path.basename(destination)}`, destination]);
}
for (const { destination } of queue) run('codesign', ['--force', '--sign', '-', destination]);
console.log(`Bundled and signed Mosh with ${names.size} recursive Mach-O dependencies`);
