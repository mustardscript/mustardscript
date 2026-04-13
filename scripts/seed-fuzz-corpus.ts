'use strict';

const fs = require('node:fs');
const path = require('node:path');

const { Mustard, Progress } = require('../index.ts');

const repoRoot = path.join(__dirname, '..');
const corpusRoot = path.join(repoRoot, 'fuzz', 'corpus');
const snapshotKey = Buffer.from('fuzz-corpus-snapshot-key');

const SUPPORTED_SOURCE_SEEDS = Object.freeze([
  {
    name: 'basic-arithmetic.js',
    source: 'const value = 1; value + 2;',
  },
  {
    name: 'collections-and-json.js',
    source: `
      const map = new Map([[1, 'one'], [2, 'two']]);
      JSON.stringify({ first: map.get(1), size: map.size });
    `,
  },
  {
    name: 'async-promises.js',
    source: `
      async function main() {
        return await Promise.all([
          Promise.resolve(1),
          Promise.resolve(2).then((value) => value + 3),
        ]);
      }
      main();
    `,
  },
]);

const UNSUPPORTED_SOURCE_SEEDS = Object.freeze([
  {
    name: 'unsupported-class.js',
    source: 'class Example {}',
  },
  {
    name: 'unsupported-delete.js',
    source: 'delete value.prop;',
  },
  {
    name: 'unsupported-dynamic-import.js',
    source: 'import("pkg");',
  },
]);

const SUSPENSION_SOURCE_SEEDS = Object.freeze([
  {
    name: 'sync-capability.snapshot',
    source: 'const value = fetch_data(4); value + 2;',
  },
  {
    name: 'queued-async-capabilities.snapshot',
    source: `
      async function main() {
        const first = fetch_data(1);
        const second = fetch_data(2);
        return [await first, await second];
      }
      main();
    `,
  },
  {
    name: 'loop-capability.snapshot',
    source: `
      let total = 0;
      for (const value of [1, 2, 3]) {
        total += fetch_data(value);
      }
      total;
    `,
  },
]);

function ensureDir(dirPath) {
  fs.mkdirSync(dirPath, { recursive: true });
}

function writeSeed(target, name, contents) {
  const targetDir = path.join(corpusRoot, target);
  ensureDir(targetDir);
  const destination = path.join(targetDir, name);
  const nextValue = Buffer.isBuffer(contents) ? contents : Buffer.from(String(contents), 'utf8');
  if (fs.existsSync(destination)) {
    const current = fs.readFileSync(destination);
    if (current.equals(nextValue)) {
      return;
    }
  }
  fs.writeFileSync(destination, nextValue);
}

function suspendedSnapshotBytes(source) {
  const runtime = new Mustard(source);
  const step = runtime.start({
    snapshotKey,
    capabilities: {
      fetch_data() {},
    },
    limits: {},
  });
  if (!(step instanceof Progress)) {
    throw new Error(`expected suspended progress for snapshot corpus seed: ${source.trim()}`);
  }
  return step.dump().snapshot;
}

function writeSourceSeeds() {
  for (const seed of [...SUPPORTED_SOURCE_SEEDS, ...UNSUPPORTED_SOURCE_SEEDS]) {
    writeSeed('parser', seed.name, `${seed.source.trim()}\n`);
    writeSeed('ir_lowering', seed.name, `${seed.source.trim()}\n`);
  }
}

function writeProgramSeeds() {
  for (const seed of SUPPORTED_SOURCE_SEEDS) {
    const program = new Mustard(seed.source).dump();
    writeSeed('bytecode_validation', seed.name.replace(/\.js$/, '.bin'), program);
    writeSeed('bytecode_execution', seed.name.replace(/\.js$/, '.bin'), program);
  }
}

function writeSnapshotSeeds() {
  for (const seed of SUSPENSION_SOURCE_SEEDS) {
    writeSeed('snapshot_load', seed.name, suspendedSnapshotBytes(seed.source));
  }
}

function writeSidecarProtocolSeeds() {
  const compiled = new Mustard('const value = fetch_data(5); value + 1;');
  const programBase64 = compiled.dump().toString('base64');

  const suspended = compiled.start({
    snapshotKey,
    capabilities: {
      fetch_data() {},
    },
    limits: {},
  });
  if (!(suspended instanceof Progress)) {
    throw new Error('expected suspended progress while building sidecar protocol corpus');
  }
  const snapshotBase64 = suspended.dump().snapshot.toString('base64');

  writeSeed(
    'sidecar_protocol',
    'compile-request.jsonl',
    `${JSON.stringify({
      method: 'compile',
      id: 1,
      source: 'const value = fetch_data(5); value + 1;',
    })}\n`,
  );
  writeSeed(
    'sidecar_protocol',
    'start-request.jsonl',
    `${JSON.stringify({
      method: 'start',
      id: 2,
      program_base64: programBase64,
      options: {
        inputs: {},
        capabilities: ['fetch_data'],
      },
    })}\n`,
  );
  writeSeed(
    'sidecar_protocol',
    'resume-request.jsonl',
    `${JSON.stringify({
      method: 'resume',
      id: 3,
      snapshot_base64: snapshotBase64,
      policy: {
        capabilities: ['fetch_data'],
        limits: {},
      },
      payload: {
        type: 'value',
        value: {
          Number: {
            Finite: 5,
          },
        },
      },
    })}\n`,
  );
  writeSeed(
    'sidecar_protocol',
    'hostile-invalid-base64.jsonl',
    `${JSON.stringify({
      method: 'start',
      id: 4,
      program_base64: '%%%%',
      options: {
        inputs: {},
        capabilities: [],
      },
    })}\n`,
  );
}

function main() {
  ensureDir(corpusRoot);
  writeSourceSeeds();
  writeProgramSeeds();
  writeSnapshotSeeds();
  writeSidecarProtocolSeeds();
}

main();
