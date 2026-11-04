import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { checks, gate } from './check.mjs';

test('Git accepts CRLF while rejecting real whitespace defects in both indexes', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'termterm-whitespace-'));
  const run = args => spawnSync('git', args, { cwd: directory, encoding: 'utf8', windowsHide: true });
  const ok = result => { assert.ifError(result.error); assert.equal(result.status, 0, result.stderr + result.stdout); };
  try {
    ok(run(['init', '-q']));
    const file = path.join(directory, 'fixture.txt');
    for (const [content, valid] of [['changed\r\n', true], ['changed \r\n', false], ['changed\t\r\n', false], ['changed\r\n\r\n', false], [' \tindented\r\n', false]]) {
      fs.writeFileSync(file, 'original\n'); ok(run(['-c', 'core.autocrlf=false', 'add', 'fixture.txt']));
      fs.writeFileSync(file, content);
      const unstaged = run(checks()[0][2]); assert.ifError(unstaged.error);
      assert.equal(unstaged.status === 0, valid, unstaged.stdout + unstaged.stderr);
      ok(run(['-c', 'core.autocrlf=false', 'add', 'fixture.txt']));
      const staged = run(checks()[1][2]); assert.ifError(staged.error);
      assert.equal(staged.status === 0, valid, staged.stdout + staged.stderr);
    }
  } finally {
    const resolved = fs.realpathSync(directory);
    assert.equal(path.dirname(resolved), fs.realpathSync(os.tmpdir()));
    assert.ok(path.basename(resolved).startsWith('termterm-whitespace-'));
    fs.rmSync(resolved, { recursive: true, force: true });
  }
});

test('all baseline checks run and pass', () => {
  let count = 0;
  const result = gate({}, () => { count++; return { status: 0 }; });
  assert.equal(count, 5);
  assert.equal(result.decision, undefined);
  assert.match(result.systemMessage, /baseline passed/);
});

test('each failed check blocks completion without running later checks', () => {
  for (let fail = 0; fail < checks().length; fail++) {
    let count = 0;
    const result = gate({}, () => ({ status: count++ === fail ? 1 : 0 }));
    assert.equal(result.decision, 'block');
    assert.equal(count, fail + 1);
    assert.match(result.reason, /do not claim checks passed/);
  }
});

test('missing tool and timeout are failures, not passes', () => {
  for (const error of ['ENOENT', 'ETIMEDOUT']) {
    assert.equal(gate({}, () => ({ error: new Error(error), status: null })).decision, 'block');
  }
});

test('continued turns recheck but report repeated failure without looping', () => {
  let count = 0;
  const result = gate({ stop_hook_active: true }, () => { count++; return { status: 1 }; });
  assert.equal(count, 1);
  assert.equal(result.decision, undefined);
  assert.match(result.systemMessage, /failed/);
});

test('invalid input cannot silently bypass the gate', () => {
  for (const input of [null, [], 'text', { stop_hook_active: 'true' }]) {
    assert.throws(() => gate(input, () => { throw new Error('must not run'); }), /Invalid Stop/);
  }
});

test('check budget fits hook timeout and never installs or publishes', () => {
  assert.ok(checks().reduce((sum, check) => sum + check[3], 0) < 300000);
  assert.ok(checks().every(([, command, args]) => !/install|publish|release|push/.test([command, ...args].join(' '))));
});

test('CLI blocks malformed and invalid input with exit 2', () => {
  for (const input of ['{', 'null', '{"stop_hook_active":"true"}']) {
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('./check.mjs', import.meta.url))], {
      input, encoding: 'utf8', windowsHide: true,
    });
    assert.ifError(result.error);
    assert.equal(result.status, 2);
    assert.equal(result.stdout, '');
    assert.match(result.stderr, /TermTerm check could not run:/);
    assert.match(result.stderr, /do not claim checks passed/);
  }
});
