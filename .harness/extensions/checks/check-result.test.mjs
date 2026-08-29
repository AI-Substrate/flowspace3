import assert from 'node:assert/strict';
import test from 'node:test';

import { outputDetails, testGateVerdict } from './check-result.mjs';

test('failure details retain stderr independently of long stdout', () => {
  const stdout = Array.from({ length: 100 }, (_, index) => `stdout ${index}`).join('\n');
  const details = outputDetails({ stdout, stderr: 'panic: forced-stderr-sentinel' }, 5);

  assert.match(details, /--- stdout \(last 5 lines\) ---\nstdout 95/);
  assert.match(details, /--- stderr \(last 5 lines\) ---\npanic: forced-stderr-sentinel/);
});

test('connection-shaped mid-suite failure is infrastructure and cannot pass', () => {
  const result = {
    ok: false,
    stdout: 'test result: FAILED. 0 passed; 12 failed; 0 ignored',
    stderr: 'error: terminating connection due to administrator command',
  };

  assert.deepEqual(testGateVerdict(result), {
    kind: 'infrastructure',
    evidence: {
      marker: 'terminating connection',
      line: 'error: terminating connection due to administrator command',
      wholeSuiteFailed: true,
    },
  });
  assert.equal(
    testGateVerdict({ ...result, ok: true }).kind,
    'infrastructure',
    'connection loss must never be banked as a pass even if the child exits zero',
  );
});

test('plain assertion failure remains a test failure', () => {
  assert.deepEqual(
    testGateVerdict({ ok: false, stdout: 'assertion `left == right` failed', stderr: '' }),
    { kind: 'test-failure' },
  );
});

test('clean zero exit passes', () => {
  assert.deepEqual(testGateVerdict({ ok: true, stdout: 'test result: ok', stderr: '' }), {
    kind: 'pass',
  });
});
