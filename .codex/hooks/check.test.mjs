import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { checks, gate } from './check.mjs';

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
