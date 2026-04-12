const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const { Jslite } = require('../../index.js');
const { COVERAGE, FEATURE_CONTRACT, OUTCOME } = require('./conformance-contract.js');
const { assertDifferential } = require('./runtime-oracle.js');

const LANGUAGE_CONTRACT = fs.readFileSync(path.join(__dirname, '../../docs/LANGUAGE.md'), 'utf8');

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

test('built-in error constructors round-trip visible fields', async () => {
  const runtime = new Jslite(`
    const range = new RangeError('too far');
    const type = new TypeError('wrong kind');
    [
      range.name,
      range.message,
      type.name,
      type.message,
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
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
