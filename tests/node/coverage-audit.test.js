const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const { Jslite } = require('../../index.js');
const { COVERAGE, FEATURE_CONTRACT, OUTCOME } = require('./conformance-contract.js');
const { assertDifferential } = require('./runtime-oracle.js');

function readRepo(relativePath) {
  return fs.readFileSync(path.join(__dirname, '../../', relativePath), 'utf8');
}

function assertFileContains(relativePath, pattern) {
  const body = readRepo(relativePath);
  if (pattern instanceof RegExp) {
    assert.match(body, pattern, `${relativePath} is missing ${pattern}`);
    return;
  }
  assert.ok(body.includes(pattern), `${relativePath} is missing ${pattern}`);
}

function extractBulletSection(document, heading) {
  const match = new RegExp(`${heading}\\n\\n((?:- .*\\n)+)`).exec(document);
  assert.ok(match, `missing section ${heading}`);
  return match[1]
    .trim()
    .split('\n')
    .map((line) => line.replace(/^- /, '').trim());
}

function extractSidecarMethods(document) {
  return Array.from(document.matchAll(/^\d+\. `([^`]+)`$/gm), (match) => match[1]);
}

const README = readRepo('README.md');
const HOST_API = readRepo('docs/HOST_API.md');
const LANGUAGE_CONTRACT = readRepo('docs/LANGUAGE.md');
const SERIALIZATION_DOC = readRepo('docs/SERIALIZATION.md');
const SIDECAR_PROTOCOL = readRepo('docs/SIDECAR_PROTOCOL.md');

const MANUAL_CONFORMANCE_BUCKETS = Object.freeze({
  'language.array-holes': {
    file: 'tests/node/builtins.test.js',
    pattern: 'run preserves sparse array holes across helpers, enumeration, and JSON',
  },
  'language.array-spread': {
    file: 'tests/node/differential.test.js',
    pattern: 'array spread and spread arguments over supported iterables',
  },
  'language.object-literal-computed-keys': {
    file: 'tests/node/builtins.test.js',
    pattern: 'run supports computed object literal keys, method shorthand, and object spread for plain objects and arrays',
  },
  'language.object-literal-methods': {
    file: 'tests/node/builtins.test.js',
    pattern: 'run supports computed object literal keys, method shorthand, and object spread for plain objects and arrays',
  },
  'language.object-literal-spread': {
    file: 'tests/node/builtins.test.js',
    pattern: 'run supports computed object literal keys, method shorthand, and object spread for plain objects and arrays',
  },
  'language.spread-arguments': {
    file: 'tests/node/differential.test.js',
    pattern: 'array spread and spread arguments over supported iterables',
  },
  'language.sequence-expressions': {
    file: 'tests/node/differential.test.js',
    pattern: 'sequence expressions and exponentiation',
  },
  'language.for-loops': {
    file: 'tests/node/differential.test.js',
    pattern: 'branching, loops, and switch',
  },
  'language.for-await-of': {
    file: 'tests/node/iteration.test.js',
    pattern: 'run supports for await...of over the documented iterable surface',
  },
  'validation.logical-assignment-or': {
    file: 'tests/node/differential.test.js',
    pattern: 'logical assignment short-circuits and evaluates member targets once',
  },
  'validation.logical-assignment-and': {
    file: 'tests/node/differential.test.js',
    pattern: 'logical assignment short-circuits and evaluates member targets once',
  },
  'runtime.object-create': {
    file: 'tests/node/builtins.test.js',
    pattern: 'Object.create is unsupported because prototype semantics are deferred',
  },
  'runtime.object-freeze': {
    file: 'tests/node/builtins.test.js',
    pattern: 'Object.freeze is unsupported because property descriptor semantics are deferred',
  },
  'runtime.object-seal': {
    file: 'tests/node/builtins.test.js',
    pattern: 'Object.seal is unsupported because property descriptor semantics are deferred',
  },
  'observable.sorted-object-enumeration': {
    file: 'tests/node/coverage-audit.test.js',
    pattern: 'JSON.stringify matches Node for property order, number rendering, and omission rules',
  },
  'observable.sorted-json-stringify': {
    file: 'tests/node/coverage-audit.test.js',
    pattern: 'JSON.stringify matches Node for property order, number rendering, and omission rules',
  },
});

const DOCUMENTED_BUILTIN_COVERAGE = Object.freeze({
  '`globalThis`': [
    {
      file: 'tests/node/coverage-audit.test.js',
      pattern: 'globalThis remains a stable guest-visible object',
    },
  ],
  '`Object`': [
    {
      file: 'tests/node/builtins.test.js',
      pattern: 'run supports conservative array, string, object, and Math helper surface',
    },
    {
      file: 'tests/node/builtins.test.js',
      pattern: 'Object.assign copies supported enumerable properties and unsupported object helpers fail closed',
    },
  ],
  '`Array`': [
    {
      file: 'tests/node/builtins.test.js',
      pattern: 'run preserves sparse array holes across helpers, enumeration, and JSON',
    },
    {
      file: 'tests/node/differential.test.js',
      pattern: 'array spread and spread arguments over supported iterables',
    },
  ],
  '`Map`': [
    {
      file: 'tests/node/keyed-collections.test.js',
      pattern: 'run supports Map mutation, lookup, and SameValueZero semantics',
    },
    {
      file: 'tests/node/keyed-collections.test.js',
      pattern: 'Map and Set values cannot cross the structured host boundary',
    },
  ],
  '`Set`': [
    {
      file: 'tests/node/keyed-collections.test.js',
      pattern: 'run supports Set mutation, membership, and clear semantics',
    },
    {
      file: 'tests/node/keyed-collections.test.js',
      pattern: 'Map and Set values cannot cross the structured host boundary',
    },
  ],
  '`Promise`': [
    {
      file: 'tests/node/async-runtime.test.js',
      pattern: 'run supports Promise instance methods and combinators for the documented surface',
    },
    {
      file: 'tests/node/async-schedule.test.js',
      pattern: 'Promise.all',
    },
  ],
  '`RegExp`': [
    {
      file: 'tests/node/builtins.test.js',
      pattern: 'run supports RegExp helpers, regex string patterns, and callback replacements',
    },
    {
      file: 'tests/node/builtins.test.js',
      pattern: 'RegExp helpers fail closed for unsupported flags, non-global replaceAll, and sync host replacements',
    },
  ],
  '`String`': [
    {
      file: 'tests/node/builtins.test.js',
      pattern: 'run supports conservative array, string, object, and Math helper surface',
    },
  ],
  '`Error`': [
    {
      file: 'tests/node/coverage-audit.test.js',
      pattern: 'primitive and error constructors expose the documented built-in surface',
    },
  ],
  '`TypeError`': [
    {
      file: 'tests/node/coverage-audit.test.js',
      pattern: 'primitive and error constructors expose the documented built-in surface',
    },
  ],
  '`ReferenceError`': [
    {
      file: 'tests/node/coverage-audit.test.js',
      pattern: 'primitive and error constructors expose the documented built-in surface',
    },
  ],
  '`RangeError`': [
    {
      file: 'tests/node/coverage-audit.test.js',
      pattern: 'primitive and error constructors expose the documented built-in surface',
    },
  ],
  '`Number`': [
    {
      file: 'tests/node/coverage-audit.test.js',
      pattern: 'primitive and error constructors expose the documented built-in surface',
    },
  ],
  '`Boolean`': [
    {
      file: 'tests/node/coverage-audit.test.js',
      pattern: 'primitive and error constructors expose the documented built-in surface',
    },
  ],
  '`Math`': [
    {
      file: 'tests/node/builtins.test.js',
      pattern: 'run supports conservative array, string, object, and Math helper surface',
    },
  ],
  '`JSON`': [
    {
      file: 'tests/node/builtins.test.js',
      pattern: 'JSON.stringify matches Node ordering and omission semantics for supported values',
    },
  ],
  'A placeholder `console` global object': [
    {
      file: 'tests/node/host-boundary.test.js',
      pattern: 'run routes deterministic console callbacks and ignores host return values',
    },
    {
      file: 'tests/node/host-boundary.test.js',
      pattern: 'console methods fail guest-safely when callbacks are not registered',
    },
  ],
});

const PUBLIC_API_MISUSE_COVERAGE = Object.freeze({
  'run()': [
    {
      file: 'tests/node/property-boundary.test.js',
      pattern: "new Jslite('value;').run({ inputs: { value } })",
    },
  ],
  'start()': [
    {
      file: 'tests/node/property-boundary.test.js',
      pattern: "new Jslite('value;').start({ inputs: { value } })",
    },
  ],
  'resume()': [
    {
      file: 'tests/node/property-boundary.test.js',
      pattern: 'resumed.resume(value)',
    },
    {
      file: 'tests/node/progress.test.js',
      pattern: 'progress objects are single-use',
    },
  ],
  'resumeError()': [
    {
      file: 'tests/node/property-boundary.test.js',
      pattern: 'resumedError.resumeError(hostError)',
    },
  ],
  'Progress.cancel()': [
    {
      file: 'tests/node/cancellation.test.js',
      pattern: 'progress.cancel aborts suspended execution without guest catch interception',
    },
  ],
  'Progress.load(...)': [
    {
      file: 'tests/node/security-progress-load.test.js',
      pattern: 'progress load requires explicit policy and snapshotKey outside the current process',
    },
    {
      file: 'tests/node/serialization.test.js',
      pattern: 'progress load surfaces snapshot failures as typed errors',
    },
  ],
  'Jslite.load(...)': [
    {
      file: 'tests/node/serialization.test.js',
      pattern: 'Jslite.load surfaces invalid compiled-program blobs as typed errors',
    },
  ],
});

test('JSON.stringify matches Node for property order, number rendering, and omission rules', async () => {
  await assertDifferential(`
    const record = {};
    record.beta = 2;
    record[10] = 10;
    record.alpha = 1;
    record[2] = 3;
    record["01"] = 4;
    const values = [1, undefined, () => 3, (0 / 0), -0, (1 / 0)];
    ({
      objectKeys: Object.keys(record),
      objectValues: Object.values(record),
      objectEntries: Object.entries(record),
      stringifiedRecord: JSON.stringify(record),
      stringifiedValues: JSON.stringify(values),
      stringifiedWrapper: JSON.stringify({
        keep: 1,
        skipUndefined: undefined,
        skipFunction: () => 1,
        nested: record,
      }),
    });
  `);
});

test('primitive and error constructors expose the documented built-in surface', async () => {
  const runtime = new Jslite(`
    const base = new Error('boom');
    const reference = new ReferenceError('missing');
    const range = new RangeError('too far');
    const type = new TypeError('wrong kind');
    [
      Number('12'),
      Number('nope'),
      Boolean(0),
      Boolean('ok'),
      base.name,
      base.message,
      reference.name,
      reference.message,
      range.name,
      range.message,
      type.name,
      type.message,
    ];
  `);

  const result = await runtime.run();
  assert.equal(result[0], 12);
  assert.ok(Number.isNaN(result[1]));
  assert.equal(result[2], false);
  assert.equal(result[3], true);
  assert.deepEqual(result.slice(4), [
    'Error',
    'boom',
    'ReferenceError',
    'missing',
    'RangeError',
    'too far',
    'TypeError',
    'wrong kind',
  ]);
});

test('globalThis remains a stable guest-visible object', async () => {
  const runtime = new Jslite(`
    globalThis.answer = 3;
    [
      typeof globalThis,
      globalThis.answer,
      globalThis === globalThis,
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, ['object', 3, true]);
});

test('in operator follows the conservative supported property surface and rejects primitives', async () => {
  const runtime = new Jslite(`
    const object = { alpha: undefined };
    const array = [4];
    array.extra = 5;
    const map = new Map();
    const set = new Set();
    const promise = Promise.resolve(1);
    const regex = /a/g;
    const date = new Date(5);
    [
      "alpha" in object,
      "missing" in object,
      0 in array,
      1 in array,
      "length" in array,
      "push" in array,
      "extra" in array,
      "log" in Math,
      "parse" in JSON,
      "then" in promise,
      "exec" in regex,
      "getTime" in date,
      "size" in map,
      "add" in set,
      "from" in Array,
      "assign" in Object,
      "now" in Date,
      "resolve" in Promise,
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    true,
    false,
    true,
    false,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
  ]);

  await assert.rejects(
    () => new Jslite(`"length" in "abc";`).run(),
    (error) =>
      error &&
      error.kind === 'Runtime' &&
      error.message.includes("right-hand side of 'in' must be an object in the supported surface"),
  );
});

test('deferring await does not inject a guest-visible cancellation signal', async () => {
  const runtime = new Jslite(`
    const value = fetch_data(2);
    value + 1;
  `);

  let calls = 0;
  let completed = 0;
  const pending = runtime.run({
    capabilities: {
      async fetch_data(value) {
        calls += 1;
        await new Promise((resolve) => setTimeout(resolve, 50));
        completed += 1;
        return value;
      },
    },
  });

  await new Promise((resolve) => setTimeout(resolve, 100));
  assert.equal(calls, 1);
  assert.equal(completed, 1);
  assert.equal(await pending, 3);
});

test('documented unsupported classes map to conformance reject entries with phase and category', () => {
  const contractById = new Map(FEATURE_CONTRACT.map((entry) => [entry.id, entry]));
  const requiredMappings = [
    {
      snippet: '- `import`, `export`, and dynamic `import()`',
      ids: ['validation.module-syntax', 'validation.dynamic-import'],
    },
    {
      snippet:
        '- free references to `process`, `module`, `exports`, `global`, `require`,\n  `setTimeout`, `setInterval`, `queueMicrotask`, and `fetch`',
      ids: ['validation.ambient-globals'],
    },
    {
      snippet: '- `var`, `using`, and `await using`',
      ids: ['validation.var', 'validation.using-declarations'],
    },
    {
      snippet: '- symbols',
      ids: ['runtime.symbol'],
    },
    {
      snippet: '- typed arrays',
      ids: ['runtime.typed-arrays'],
    },
    {
      snippet: '- `Intl`',
      ids: ['runtime.intl'],
    },
    {
      snippet: '- `Proxy`',
      ids: ['runtime.proxy'],
    },
    {
      snippet: '- accessors',
      ids: ['validation.object-accessors'],
    },
    {
      snippet: '- property descriptor semantics',
      ids: ['runtime.object-freeze', 'runtime.object-seal'],
    },
    {
      snippet: '- full prototype semantics',
      ids: ['runtime.object-create'],
    },
  ];

  for (const mapping of requiredMappings) {
    assert.ok(LANGUAGE_CONTRACT.includes(mapping.snippet), `missing docs snippet: ${mapping.snippet}`);
    for (const id of mapping.ids) {
      const entry = contractById.get(id);
      assert.ok(entry, `missing conformance contract entry for ${id}`);
      assert.notEqual(entry.outcome, OUTCOME.NODE_PARITY);
      assert.match(entry.phase, /\S/);
      assert.match(entry.category, /\S/);
      assert.ok(
        entry.coverage.some((coverage) =>
          [
            COVERAGE.EXISTING,
            COVERAGE.PROPERTY_NEGATIVE,
            COVERAGE.TEST262_UNSUPPORTED,
          ].includes(coverage),
        ),
        `missing executable coverage mapping for ${id}`,
      );
    }
  }
});

test('manual conformance buckets stay tied to explicit executable anchors', () => {
  const manualEntries = FEATURE_CONTRACT.filter((entry) =>
    entry.coverage.some((coverage) => [COVERAGE.EXISTING, COVERAGE.AUDIT].includes(coverage)),
  );
  const mappedIds = new Set(Object.keys(MANUAL_CONFORMANCE_BUCKETS));
  const contractIds = new Set(manualEntries.map((entry) => entry.id));

  assert.deepEqual(
    [...contractIds].filter((id) => !mappedIds.has(id)).sort(),
    [],
    'missing manual conformance audit mapping',
  );
  assert.deepEqual(
    [...mappedIds].filter((id) => !contractIds.has(id)).sort(),
    [],
    'stale manual conformance audit mapping',
  );

  for (const [id, anchor] of Object.entries(MANUAL_CONFORMANCE_BUCKETS)) {
    assert.ok(FEATURE_CONTRACT.find((entry) => entry.id === id), `missing contract entry for ${id}`);
    assertFileContains(anchor.file, anchor.pattern);
  }
});

test('documented built-ins keep explicit coverage anchors', () => {
  const builtins = extractBulletSection(README, 'Currently implemented built-ins:');

  assert.deepEqual(
    builtins.filter((builtin) => DOCUMENTED_BUILTIN_COVERAGE[builtin] === undefined).sort(),
    [],
    'missing built-in audit mapping',
  );
  assert.deepEqual(
    Object.keys(DOCUMENTED_BUILTIN_COVERAGE)
      .filter((builtin) => !builtins.includes(builtin))
      .sort(),
    [],
    'stale built-in audit mapping',
  );

  for (const builtin of builtins) {
    for (const anchor of DOCUMENTED_BUILTIN_COVERAGE[builtin]) {
      assertFileContains(anchor.file, anchor.pattern);
    }
  }
});

test('documented public runtime methods keep misuse-path coverage anchors', () => {
  const documentedMethods = [
    'run()',
    'start()',
    'resume()',
    'resumeError()',
    'Progress.cancel()',
    'Progress.load(...)',
    'Jslite.load(...)',
  ];

  for (const method of documentedMethods) {
    assert.ok(
      README.includes(`\`${method}\``) ||
        HOST_API.includes(`\`${method}\``) ||
        SERIALIZATION_DOC.includes(`\`${method}\``),
      `missing method in docs: ${method}`,
    );
    assert.ok(PUBLIC_API_MISUSE_COVERAGE[method], `missing misuse-path mapping for ${method}`);
    for (const anchor of PUBLIC_API_MISUSE_COVERAGE[method]) {
      assertFileContains(anchor.file, anchor.pattern);
    }
  }
});

test('documented sidecar methods keep both valid-flow and hostile-input coverage', () => {
  const validFiles = [
    'crates/jslite-sidecar/tests/protocol.rs',
    'crates/jslite-sidecar/tests/protocol_state.rs',
  ].map(readRepo);
  const hostile = readRepo('crates/jslite-sidecar/tests/hostile_protocol.rs');

  for (const method of extractSidecarMethods(SIDECAR_PROTOCOL)) {
    assert.ok(
      validFiles.some((body) => body.includes(`"method": "${method}"`)),
      `missing valid-flow coverage for sidecar method ${method}`,
    );
    assert.ok(
      hostile.includes(`"method": "${method}"`),
      `missing hostile-input coverage for sidecar method ${method}`,
    );
  }
});
