const tail = (value, lines) => value.trimEnd().split('\n').slice(-lines).join('\n');

/**
 * Keep each process stream independently so a long stdout cannot evict the
 * panic or connection error Cargo wrote to stderr.
 */
export function outputDetails(result, lines = 40) {
  const stdout = tail(result.stdout ?? '', lines);
  const stderr = tail(result.stderr ?? '', lines);
  return [`--- stdout (last ${lines} lines) ---`, stdout, `--- stderr (last ${lines} lines) ---`, stderr]
    .join('\n')
    .trimEnd();
}

const CONNECTION_FAILURES = [
  ['I/O error', /\b(?:io|i\/o) error\b/i],
  ['connection closed', /\bconnection (?:was )?closed\b/i],
  ['connection reset', /\bconnection reset(?: by peer)?\b/i],
  ['connection terminated', /\bconnection (?:was )?terminated\b/i],
  ['pool timeout', /\b(?:pool timed out|pool timeout|timed out (?:while )?waiting for (?:an? )?(?:open )?connection)\b/i],
  ['terminating connection', /\bterminating connection\b/i],
  ['server closed connection', /\bserver closed the connection unexpectedly\b/i],
  ['unexpected EOF', /\bunexpected eof\b/i],
  ['broken pipe', /\bbroken pipe\b/i],
];

/**
 * Decide the test gate without weakening it: connection evidence changes only
 * the kind of red. A plain assertion remains a test failure, and a zero exit
 * carrying connection-loss evidence is refused rather than banked as a pass.
 */
export function testGateVerdict(result) {
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  for (const [marker, pattern] of CONNECTION_FAILURES) {
    const line = output.split('\n').find((candidate) => pattern.test(candidate));
    if (line !== undefined) {
      return {
        kind: 'infrastructure',
        evidence: {
          marker,
          line: line.trim().slice(0, 240),
          wholeSuiteFailed: /test result: FAILED\. 0 passed; [1-9]\d* failed/i.test(output),
        },
      };
    }
  }
  return { kind: result.ok ? 'pass' : 'test-failure' };
}
