'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError, Progress } = require('../../index.js');

test('run supports Map mutation, lookup, and SameValueZero semantics', async () => {
  const runtime = new Jslite(`
    const shared = {};
    const nan = Number('nope');
    const map = new Map();
    map.set('alpha', 1);
    map.set(nan, 'nan');
    map.set(-0, 'zero');
    map.set(shared, 7);
    map.set('alpha', 2);
    [
      map.size,
      map.get('alpha'),
      map.has('alpha'),
      map.get(nan),
      map.has(0),
      map.get(0),
      map.get(-0),
      map.get(shared),
      map.delete('missing'),
      map.delete(nan),
      map.has(nan),
      map.size,
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [4, 2, true, 'nan', true, 'zero', 'zero', 7, false, true, false, 3]);
});

test('run supports Set mutation, membership, and clear semantics', async () => {
  const runtime = new Jslite(`
    const shared = {};
    const nan = Number('nope');
    const set = new Set();
    set.add('alpha');
    set.add(nan);
    set.add(-0);
    set.add(shared);
    set.add(nan);
    set.add(0);
    const before = [
      set.size,
      set.has(nan),
      set.has(0),
      set.has(-0),
      set.has(shared),
    ];
    const removed = [
      set.delete('missing'),
      set.delete(nan),
      set.has(nan),
      set.size,
    ];
    set.clear();
    [before, removed, set.size, set.has(shared)];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    [4, true, true, true, true],
    [false, true, false, 3],
    0,
    false,
  ]);
});

test('collection methods reject incompatible receivers', async () => {
  const runtime = new Jslite(`
    const map = new Map();
    const set = new Set();
    const mapGet = map.get;
    const setAdd = set.add;
    [
      (() => {
        try {
          mapGet('alpha');
          return 'unreachable';
        } catch (error) {
          return [error.name, error.message];
        }
      })(),
      (() => {
        try {
          setAdd(1);
          return 'unreachable';
        } catch (error) {
          return [error.name, error.message];
        }
      })(),
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    ['TypeError', 'Map.prototype.get called on incompatible receiver'],
    ['TypeError', 'Set.prototype.add called on incompatible receiver'],
  ]);
});

test('progress snapshots preserve live keyed collections and cycles across resumes', () => {
  const runtime = new Jslite(`
    const key = { label: 'shared' };
    const map = new Map();
    const set = new Set();
    map.set('count', 1);
    map.set(key, set);
    set.add(key);
    set.add(map);
    const value = fetch_data(41);
    map.set('count', value);
    ({
      count: map.get('count'),
      hasKey: map.has(key),
      setHasMap: set.has(map),
      setSize: set.size,
      mapSize: map.size,
    });
  `);

  const first = runtime.start({
    capabilities: {
      fetch_data(value) {
        return value;
      },
    },
  });

  assert.ok(first instanceof Progress);
  assert.equal(first.capability, 'fetch_data');
  assert.deepEqual(first.args, [41]);

  const restored = Progress.load(first.dump());
  const result = restored.resume(41);
  assert.deepEqual(result, {
    count: 41,
    hasKey: true,
    setHasMap: true,
    setSize: 2,
    mapSize: 2,
  });
});

test('unsupported collection iteration surfaces fail closed', async () => {
  await assert.rejects(
    () => new Jslite("new Map([['alpha', 1]]);").run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('Map constructor iterable inputs are not supported'),
  );

  await assert.rejects(
    () => new Jslite('new Map().entries();').run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('Map iterator-producing APIs are not supported'),
  );

  await assert.rejects(
    () => new Jslite('new Set([1, 2]);').run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('Set constructor iterable inputs are not supported'),
  );

  await assert.rejects(
    () => new Jslite('new Set().values();').run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('Set iterator-producing APIs are not supported'),
  );
});

test('guest keyed collections cannot cross the structured host boundary', async () => {
  await assert.rejects(
    () =>
      new Jslite(`
        const map = new Map();
        map.set('alpha', 1);
        map;
      `).run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('Map and Set values cannot cross the structured host boundary'),
  );

  await assert.rejects(
    () =>
      new Jslite(`
        const set = new Set();
        set.add(1);
        sink(set);
      `).run({
        capabilities: {
          sink() {
            return undefined;
          },
        },
      }),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('Map and Set values cannot cross the structured host boundary'),
  );
});

test('host Map and Set inputs are rejected before they cross the wrapper boundary', async () => {
  const runtime = new Jslite('value;');

  await assert.rejects(
    () => runtime.run({ inputs: { value: new Map([['alpha', 1]]) } }),
    (error) =>
      error instanceof TypeError &&
      error.message.includes('only plain objects and arrays can cross the host boundary'),
  );

  await assert.rejects(
    () => runtime.run({ inputs: { value: new Set([1, 2]) } }),
    (error) =>
      error instanceof TypeError &&
      error.message.includes('only plain objects and arrays can cross the host boundary'),
  );
});
