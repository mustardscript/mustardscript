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

test('collection constructors and iteration helpers support the documented iterable surface', async () => {
  const runtime = new Jslite(`
    const map = new Map([['alpha', 1], ['beta', 2], ['alpha', 3]]);
    const set = new Set('abba');
    const entry = map.entries().next();
    const key = map.keys().next();
    const value = map.values().next();
    const setEntry = set.entries().next();
    const seen = [];
    for (const [itemKey, itemValue] of map) {
      seen[seen.length] = itemKey + ':' + itemValue;
    }
    let setSeen = '';
    for (const item of set) {
      setSeen += item;
    }
    [
      map.size,
      map.get('alpha'),
      set.size,
      entry.value[0],
      entry.value[1],
      entry.done,
      key.value,
      value.value,
      setEntry.value[0],
      setEntry.value[1],
      setSeen,
      seen,
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [2, 3, 2, 'alpha', 3, false, 'alpha', 3, 'a', 'a', 'ab', ['alpha:3', 'beta:2']]);
});

test('Map and Set iterators visit entries appended during active iteration', async () => {
  const runtime = new Jslite(`
    const map = new Map([
      ['alpha', 1],
      ['omega', 2],
    ]);
    const seen = [];
    for (const [key, value] of map) {
      seen[seen.length] = [key, value];
      if (key === 'alpha') {
        map.set('tail', 3);
      }
      if (key === 'omega') {
        map.delete('alpha');
      }
    }

    const set = new Set(['alpha', 'omega']);
    const setSeen = [];
    for (const value of set) {
      setSeen[setSeen.length] = value;
      if (value === 'alpha') {
        set.add('tail');
      }
      if (value === 'omega') {
        set.delete('alpha');
      }
    }

    ({ seen, finalMap: Array.from(map.entries()), setSeen, finalSet: Array.from(set.values()) });
  `);

  const result = await runtime.run();
  assert.deepEqual(result, {
    seen: [
      ['alpha', 1],
      ['omega', 2],
      ['tail', 3],
    ],
    finalMap: [
      ['omega', 2],
      ['tail', 3],
    ],
    setSeen: ['alpha', 'omega', 'tail'],
    finalSet: ['omega', 'tail'],
  });
});

test('Map and Set iterators can continue after clear followed by new entries', async () => {
  const runtime = new Jslite(`
    const map = new Map([
      ['alpha', 1],
      ['omega', 2],
    ]);
    const seen = [];
    for (const [key, value] of map) {
      seen[seen.length] = [key, value];
      if (key === 'alpha') {
        map.clear();
        map.set('tail', 3);
      }
    }

    const set = new Set(['alpha', 'omega']);
    const setSeen = [];
    for (const value of set) {
      setSeen[setSeen.length] = value;
      if (value === 'alpha') {
        set.clear();
        set.add('tail');
      }
    }

    ({ seen, finalMap: Array.from(map.entries()), setSeen, finalSet: Array.from(set.values()) });
  `);

  const result = await runtime.run();
  assert.deepEqual(result, {
    seen: [
      ['alpha', 1],
      ['tail', 3],
    ],
    finalMap: [['tail', 3]],
    setSeen: ['alpha', 'tail'],
    finalSet: ['tail'],
  });
});

test('Map.prototype.forEach and Set.prototype.forEach support callback iteration', async () => {
  const runtime = new Jslite(`
    const map = new Map([
      ['alpha', 1],
      ['beta', 2],
    ]);
    const set = new Set(['alpha', 'beta']);
    const mapSeen = [];
    const setSeen = [];
    map.forEach(function (value, key, source) {
      mapSeen[mapSeen.length] = [key, value, source === map, this.tag];
      if (key === 'alpha') {
        map.set('tail', 3);
      }
    }, { tag: 'map' });
    set.forEach(function (value, key, source) {
      setSeen[setSeen.length] = [value, key, source === set, this.tag];
      if (value === 'alpha') {
        set.add('tail');
      }
    }, { tag: 'set' });
    ({ mapSeen, setSeen });
  `);

  const result = await runtime.run();
  assert.deepEqual(result, {
    mapSeen: [
      ['alpha', 1, true, 'map'],
      ['beta', 2, true, 'map'],
      ['tail', 3, true, 'map'],
    ],
    setSeen: [
      ['alpha', 'alpha', true, 'set'],
      ['beta', 'beta', true, 'set'],
      ['tail', 'tail', true, 'set'],
    ],
  });
});

test('collection forEach helpers fail closed for invalid callbacks and host suspensions', async () => {
  await assert.rejects(
    () => new Jslite('new Map().forEach(1);').run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('Map.prototype.forEach expects a callable callback'),
  );

  await assert.rejects(
    () =>
      new Jslite('new Set([1]).forEach(fetch_data);').run({
        capabilities: {
          fetch_data(value) {
            return value;
          },
        },
      }),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes(
        'Set.prototype.forEach does not support synchronous host suspensions',
      ),
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
