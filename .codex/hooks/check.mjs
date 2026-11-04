import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));

export function checks(node = process.execPath) {
  const whitespace = ['-c', 'core.whitespace=blank-at-eol,blank-at-eof,space-before-tab,cr-at-eol'];
  return [
    ['whitespace (unstaged)', 'git', [...whitespace, 'diff', '--check'], 10000],
    ['whitespace (staged)', 'git', [...whitespace, 'diff', '--cached', '--check'], 10000],
    ['hook tests', node, ['--test', '.codex/hooks/check.test.mjs'], 15000],
    ['typecheck', node, ['node_modules/typescript/bin/tsc', '--noEmit'], 60000],
    ['unit tests', node, ['node_modules/vitest/vitest.mjs', 'run', '--pool=threads', '--maxWorkers=2'], 180000],
  ];
}

export function gate(input, run) {
  if (!input || typeof input !== 'object' || Array.isArray(input)
      || (input.stop_hook_active !== undefined && typeof input.stop_hook_active !== 'boolean')) {
    throw new Error('Invalid Stop hook input');
  }
  for (const [name, command, args, timeout] of checks()) {
    const result = run(command, args, timeout);
    if (result.error || result.status !== 0) {
      const reason = `TermTerm check failed: ${name}. Fix the failure or explicitly report the blocker; do not claim checks passed.`;
      // A repeated Stop failure must not create an endless continuation loop.
      return input.stop_hook_active
        ? { systemMessage: reason }
        : { decision: 'block', reason };
    }
  }
  return { systemMessage: 'TermTerm baseline passed: whitespace, hook tests, TypeScript, unit tests. Native, lab, lint and review checks remain task-specific.' };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const raw = readFileSync(0, 'utf8');
    const input = raw.trim() ? JSON.parse(raw) : {};
    const result = gate(input, (command, args, timeout) => spawnSync(command, args, {
      cwd: root, timeout, windowsHide: true,
      // Stop accepts only JSON on stdout; test output belongs on stderr.
      stdio: ['ignore', 2, 2],
    }));
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } catch (error) {
    process.stderr.write(`TermTerm check could not run: ${error.message}. Fix the failure or explicitly report the blocker; do not claim checks passed.\n`);
    process.exitCode = 2;
  }
}
