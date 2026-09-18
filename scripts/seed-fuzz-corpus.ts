'use strict';

const fs = require('node:fs');
const path = require('node:path');

const { Mustard, Progress } = require('../index.ts');

const repoRoot = path.join(__dirname, '..');
const corpusRoot = path.join(repoRoot, 'fuzz', 'corpus');
const snapshotKey = Buffer.from('fuzz-corpus-snapshot-key');
const SIDECAR_PROTOCOL_VERSION = 2;

const SUPPORTED_SOURCE_SEEDS = Object.freeze([
  { name: 'global-regexp-cursors.js', source: `const text='a '.repeat(1500);
    [text.replace(/(a)/g,'b').length, Array.from(text.matchAll(/(a)/dg)).length];` },
  { name: 'queue-shift.js', source: `const q=Array.from({length:1500},(_,i)=>i);
    while(q.length) q.shift(); q.length;` },
  { name: 'error-property-attributes.js', source: `const e=new Error('hidden');
    e.name='Own';delete e.message;e.message='visible';[Object.keys(e),JSON.stringify(e)];` },
  {
    name: 'reentrant-json-reviver.js',
    source: `const text='['.repeat(120)+'0'+']'.repeat(120);
      function visit(k,v) { if(v===0) JSON.parse(text,visit); return v; }
      JSON.parse(text,visit);`,
  },
  {
    name: 'reentrant-json-replacer.js',
    source: `const root=JSON.parse('['.repeat(120)+'0'+']'.repeat(120));
      function visit(k,v) { if(v===0) JSON.stringify(root,visit); return v; }
      JSON.stringify(root,visit);`,
  },
  {
    name: 'reentrant-json-tojson.js',
    source: `let root={toJSON(){return JSON.stringify(root);}};
      for(let i=0;i<120;i++) root=[root]; JSON.stringify(root);`,
  },
  { name: 'native-callback-recursion.js', source: 'function f(){return [1].map(f);} f();' },
  { name: 'set-compaction.js', source: `const s=new Set([0]); const it=s.values(); it.next();
    for(let i=1;i<100;i++){s.add(i);s.delete(i);} s.add(100); it.next();` },
  { name: 'sparse-array-branch.js', source: '1 ? {x:[1,,3]} : 0;' },
  {
    name: 'abstract-equality.js',
    source: `
      const value={valueOf(){return this;},toString(){return '7';}};
      [undefined==null, '1'==1, [value]==7, 9007199254740993n!=9007199254740992];
    `,
  },
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
  {name:'deep-arrow-chain.js',source:'a=>'.repeat(3000)+'0;'},
  {name:'deep-else-chain.js',source:'if(1);else '.repeat(3000)+';'},
  {name:'deep-label-chain.js',source:Array.from({length:3000},(_,i)=>`label${i}:`).join('')+';'},
  {
    name: 'unsupported-class.js',
    source: 'class Example {}',
  },
  {
    name: 'unsupported-delete.js',
    source: 'delete value;',
  },
  {
    name: 'unsupported-dynamic-import.js',
    source: 'import("pkg");',
  },
]);

const SUSPENSION_SOURCE_SEEDS = Object.freeze([
  {
    name: 'equality-coercion.snapshot',
    source: '({valueOf: fetch_data}) == 7;',
  },
  {
    name: 'equality-array-coercion.snapshot',
    source: "[[{toString: fetch_data}]] == '7';",
  },
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
  const regexpRegression = fs.readFileSync(path.join(repoRoot, 'crates/mustard/tests/fixtures/regexp-preflight-reentry.bin'));
  for (const target of ['parser', 'bytecode_execution']) {
    writeSeed(target, 'regexp-preflight-reentry.bin', regexpRegression);
  }
  for (const seed of [...SUPPORTED_SOURCE_SEEDS, ...UNSUPPORTED_SOURCE_SEEDS]) {
    writeSeed('parser', seed.name, `${seed.source.trim()}\n`);
    writeSeed('ir_lowering', seed.name, `${seed.source.trim()}\n`);
  }
}

function writeProgramSeeds() {
  writeSeed('bytecode_execution', 'oversized-lexical-depth.bin',
    fs.readFileSync(path.join(repoRoot, 'crates/mustard/tests/fixtures/oversized-lexical-depth.bin')));
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
  for (const seed of UNSUPPORTED_SOURCE_SEEDS.filter(seed => seed.name.startsWith('deep-'))) {
    writeSeed('sidecar_protocol', seed.name + '.jsonl', JSON.stringify({
      protocol_version: SIDECAR_PROTOCOL_VERSION, method:'compile', id:1, source:seed.source,
    }) + '\n');
  }
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
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'compile',
      id: 1,
      source: 'const value = fetch_data(5); value + 1;',
    })}\n`,
  );
  writeSeed(
    'sidecar_protocol',
    'start-request.jsonl',
    `${JSON.stringify({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
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
      protocol_version: SIDECAR_PROTOCOL_VERSION,
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
      protocol_version: SIDECAR_PROTOCOL_VERSION,
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
  const deepRequest = fs.readFileSync(path.join(repoRoot, 'crates/mustard-sidecar/tests/fixtures/deep-source-request.json'), 'utf8');
  writeSeed('sidecar_protocol', 'deep-source-request.jsonl', deepRequest);
  writeSeed('parser', 'deep-source.js', JSON.parse(deepRequest).source);
  writeSeed('parser', 'malformed-template.js', fs.readFileSync(path.join(repoRoot, 'crates/mustard/tests/fixtures/malformed-template.js')));
}

main();
