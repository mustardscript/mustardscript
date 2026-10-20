'use strict';

// Global-regexp exec loops over a multi-megabyte subject. Each `exec` call
// resolves `lastIndex` to a byte offset and hands the subject to the engine,
// so this lane tracks the per-call cost on large ASCII and non-ASCII inputs
// rather than regex matching itself. Run with `node benchmarks/regexp-large-subject.ts`.

const assert = require('node:assert/strict');

const { ExecutionContext, Mustard } = require('../index.ts');
const { measure } = require('./support.ts');

const HITS = 2000;
const MEASURE_OPTIONS = Object.freeze({ warmup: 1, iterations: 3, batch: 1 });

function subject(filler, hits) {
  const hit = ' src/components/Button.tsx ';
  const chunk = Math.floor(filler.length / hits);
  let text = '';
  for (let index = 0; index < hits; index += 1) {
    text += filler.slice(index * chunk, (index + 1) * chunk) + hit;
  }
  return text;
}

const SUBJECTS = {
  ascii_1mb: subject(' '.repeat(1024 * 1024), HITS),
  // Two-byte characters: every lastIndex resolution has to count chars.
  latin1_1mb: subject('é'.repeat(512 * 1024), HITS),
  // Three-byte replacement characters, as produced by lossily decoding a
  // binary file: the worst case for byte/char offset mapping.
  binary_like_1mb: subject('�'.repeat(350 * 1024), HITS),
};

const EXEC_LOOP = `
  const text = await read_file();
  const re = /[a-zA-Z0-9_\\-\\.\\/]{4,}\\.(sql|ts|tsx|js|json|md|txt|png|jpg|jpeg|css|html)/g;
  let count = 0;
  let m;
  while ((m = re.exec(text)) !== null) {
    count += 1;
  }
  count;
`;

async function benchmarkExecLoop(name, text) {
  const context = new ExecutionContext({
    capabilities: { read_file: async () => text },
    limits: { instructionBudget: Number.MAX_SAFE_INTEGER, heapLimitBytes: 128 * 1024 * 1024 },
  });
  const runtime = new Mustard(EXEC_LOOP);
  const [, summary] = await measure(name, async () => {
    const result = await runtime.run({ context });
    assert.equal(result, HITS);
  }, MEASURE_OPTIONS);
  return summary;
}

async function main() {
  const rows = [];
  for (const [name, text] of Object.entries(SUBJECTS)) {
    const summary = await benchmarkExecLoop(name, text);
    rows.push({
      subject: name,
      chars: text.length,
      bytes: Buffer.byteLength(text),
      'median ms': Number(summary.medianMs.toFixed(1)),
      'p95 ms': Number(summary.p95Ms.toFixed(1)),
      'ms per exec': Number((summary.medianMs / HITS).toFixed(3)),
    });
  }
  console.table(rows);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
