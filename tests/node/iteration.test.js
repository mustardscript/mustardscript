const test = require('node:test');
const assert = require('node:assert/strict');

const { Mustard, MustardError, Progress } = require('../../index.ts');

const SNAPSHOT_KEY = Buffer.from('iteration-test-snapshot-key');
const PROGRESS_LOAD_OPTIONS = Object.freeze({
  snapshotKey: SNAPSHOT_KEY,
  capabilities: {
    fetch_data() {},
  },
  limits: {},
});

test('run supports array for...of with fresh iteration bindings', async () => {
  const runtime = new Mustard(`
    const fns = [];
    for (const [value] of [[1], [2]]) {
      fns[fns.length] = () => value;
    }
    [fns[0](), fns[1]()];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [1, 2]);
});

test('run supports for...of assignment-target headers', async () => {
  const runtime = new Mustard(`
    let value = 0;
    const fns = [];
    for (value of [1, 2]) {
      fns[fns.length] = () => value;
    }
    const boxes = [{ current: 0 }, { current: 0 }];
    let index = 0;
    for (boxes[index].current of [3, 4]) {
      index += 1;
    }
    [fns[0](), fns[1](), value, boxes[0].current, boxes[1].current, index];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [2, 2, 2, 3, 4, 2]);
});

test('run supports for...in over plain objects and arrays', async () => {
  const runtime = new Mustard(`
    const object = { beta: 2, alpha: 1 };
    const array = [10, 20];
    array.extra = 30;
    const objectKeys = [];
    for (const key in object) {
      objectKeys[objectKeys.length] = key;
    }
    const arrayKeys = [];
    for (const key in array) {
      arrayKeys[arrayKeys.length] = key;
    }
    [objectKeys, arrayKeys];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [['beta', 'alpha'], ['0', '1', 'extra']]);
});

test('run supports for...in assignment-target headers', async () => {
  const runtime = new Mustard(`
    const record = { current: '' };
    for (record.current in { beta: 2, alpha: 1 }) {
    }
    record.current;
  `);

  const result = await runtime.run();
  assert.equal(result, 'alpha');
});

test('progress snapshots preserve active array iterators across resumes', () => {
  const runtime = new Mustard(`
    let total = 0;
    for (const value of [1, 2, 3]) {
      total += fetch_data(value);
    }
    total;
  `);

  const first = runtime.start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data(value) {
        return value * 10;
      },
    },
  });

  assert.ok(first instanceof Progress);
  assert.equal(first.capability, 'fetch_data');
  assert.deepEqual(first.args, [1]);

  const restored = Progress.load(first.dump(), PROGRESS_LOAD_OPTIONS);
  assert.ok(restored instanceof Progress);

  const second = restored.resume(10);
  assert.ok(second instanceof Progress);
  assert.equal(second.capability, 'fetch_data');
  assert.deepEqual(second.args, [2]);

  const third = second.resume(20);
  assert.ok(third instanceof Progress);
  assert.equal(third.capability, 'fetch_data');
  assert.deepEqual(third.args, [3]);

  const result = third.resume(30);
  assert.equal(result, 60);
});

test('run supports for await...of over the documented iterable surface', async () => {
  const runtime = new Mustard(`
    async function run() {
      const values = [Promise.resolve(1), 2, Promise.resolve(3)];
      const seen = [];
      let total = 0;
      for await (const value of values.values()) {
        seen[seen.length] = value;
        total += value;
      }
      const state = { current: 0 };
      for await (state.current of new Set([Promise.resolve(4), 5]).values()) {
        total += state.current;
      }
      return [seen, total, state.current];
    }
    run();
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [[1, 2, 3], 15, 5]);
});

test('progress snapshots preserve for await...of assignment-target headers across resumes', () => {
  const runtime = new Mustard(`
    async function load(value) {
      return await fetch_data(value);
    }
    async function run() {
      const state = { current: 0, total: 0 };
      for await (state.current of [load(1), load(2), load(3)]) {
        state.total += state.current;
      }
      return [state.current, state.total];
    }
    run();
  `);

  const first = runtime.start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {
        throw new Error('start should suspend before invoking JS handlers');
      },
    },
  });

  assert.ok(first instanceof Progress);
  assert.equal(first.capability, 'fetch_data');
  assert.deepEqual(first.args, [1]);

  const restored = Progress.load(first.dump(), PROGRESS_LOAD_OPTIONS);
  assert.ok(restored instanceof Progress);

  const second = restored.resume(1);
  assert.ok(second instanceof Progress);
  assert.equal(second.capability, 'fetch_data');
  assert.deepEqual(second.args, [2]);

  const third = second.resume(2);
  assert.ok(third instanceof Progress);
  assert.equal(third.capability, 'fetch_data');
  assert.deepEqual(third.args, [3]);

  const result = third.resume(3);
  assert.deepEqual(result, [3, 6]);
});

test('progress snapshots preserve assignment-target for...of headers across resumes', () => {
  const runtime = new Mustard(`
    const state = { current: 0, total: 0 };
    for (state.current of [1, 2, 3]) {
      state.total += fetch_data(state.current);
    }
    [state.current, state.total];
  `);

  const first = runtime.start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data(value) {
        return value * 10;
      },
    },
  });

  assert.ok(first instanceof Progress);
  assert.equal(first.capability, 'fetch_data');
  assert.deepEqual(first.args, [1]);

  const restored = Progress.load(first.dump(), PROGRESS_LOAD_OPTIONS);
  assert.ok(restored instanceof Progress);

  const second = restored.resume(10);
  assert.ok(second instanceof Progress);
  assert.equal(second.capability, 'fetch_data');
  assert.deepEqual(second.args, [2]);

  const third = second.resume(20);
  assert.ok(third instanceof Progress);
  assert.equal(third.capability, 'fetch_data');
  assert.deepEqual(third.args, [3]);

  const result = third.resume(30);
  assert.deepEqual(result, [3, 60]);
});

test('progress snapshots preserve active for...in iterators across resumes', () => {
  const runtime = new Mustard(`
    let total = 0;
    for (const key in { beta: 2, alpha: 1 }) {
      total += fetch_data(key);
    }
    total;
  `);

  const first = runtime.start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data(value) {
        return value.length;
      },
    },
  });

  assert.ok(first instanceof Progress);
  assert.equal(first.capability, 'fetch_data');
  assert.deepEqual(first.args, ['beta']);

  const restored = Progress.load(first.dump(), PROGRESS_LOAD_OPTIONS);
  assert.ok(restored instanceof Progress);

  const second = restored.resume(10);
  assert.ok(second instanceof Progress);
  assert.equal(second.capability, 'fetch_data');
  assert.deepEqual(second.args, ['alpha']);

  const result = second.resume(20);
  assert.equal(result, 30);
});

test('run supports strings, keyed collections, and iterator helper objects', async () => {
  const runtime = new Mustard(`
    const map = new Map([['alpha', 1], ['beta', 2]]);
    const set = new Set('aba');
    const seen = [];
    for (const [key, value] of map) {
      seen[seen.length] = key + ':' + value;
    }
    let chars = '';
    for (const value of 'hi') {
      chars += value;
    }
    let setChars = '';
    for (const value of set.keys()) {
      setChars += value;
    }
    const pair = [10, 20].entries().next();
    [seen, chars, setChars, pair.value[0], pair.value[1], pair.done];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [['alpha:1', 'beta:2'], 'hi', 'ab', 0, 10, false]);
});

test('run rejects unsupported for...in iterable inputs', async () => {
  const runtime = new Mustard(`
    for (const key in new Map()) {
      key;
    }
  `);

  await assert.rejects(
    () => runtime.run(),
    (error) =>
      error instanceof MustardError &&
      error.kind === 'Runtime' &&
      error.message.includes('Object helpers currently only support plain objects and arrays'),
  );
});

test('run rejects unsupported for...of iterable inputs', async () => {
  const runtime = new Mustard(`
    for (const value of { alpha: 1 }) {
      value;
    }
  `);

  await assert.rejects(
    () => runtime.run(),
    (error) =>
      error instanceof MustardError &&
      error.kind === 'Runtime' &&
      error.message.includes('value is not iterable in the supported surface'),
  );
});

test('run supports conservative for...in over plain objects and arrays', async () => {
  const runtime = new Mustard(`
    const object = { zebra: 1, alpha: 2 };
    const array = [10, 20];
    array.label = "seed";
    const objectKeys = [];
    const arrayKeys = [];
    let lastKey = '';
    const state = { current: '' };
    for (const key in object) {
      objectKeys[objectKeys.length] = key;
    }
    for (lastKey in array) {
      arrayKeys[arrayKeys.length] = lastKey;
    }
    for (state.current in { beta: 1 }) {
      lastKey = state.current;
    }
    [objectKeys, arrayKeys, lastKey, state.current];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [['zebra', 'alpha'], ['0', '1', 'label'], 'beta', 'beta']);
});

test('progress snapshots preserve for...in assignment-target headers across resumes', () => {
  const runtime = new Mustard(`
    const state = { current: "", seen: [] };
    for (state.current in { beta: 1, alpha: 2 }) {
      state.seen[state.seen.length] = fetch_data(state.current);
    }
    [state.current, state.seen];
  `);

  const first = runtime.start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data(value) {
        return 'seen:' + value;
      },
    },
  });

  assert.ok(first instanceof Progress);
  assert.equal(first.capability, 'fetch_data');
  assert.deepEqual(first.args, ['beta']);

  const restored = Progress.load(first.dump(), PROGRESS_LOAD_OPTIONS);
  assert.ok(restored instanceof Progress);

  const second = restored.resume('seen:beta');
  assert.ok(second instanceof Progress);
  assert.equal(second.capability, 'fetch_data');
  assert.deepEqual(second.args, ['alpha']);

  const result = second.resume('seen:alpha');
  assert.deepEqual(result, ['alpha', ['seen:beta', 'seen:alpha']]);
});

test('run rejects unsupported for...in right-hand sides', async () => {
  const runtime = new Mustard(`
    for (const key in "hi") {
      key;
    }
  `);

  await assert.rejects(
    () => runtime.run(),
    (error) =>
      error instanceof MustardError &&
      error.kind === 'Runtime' &&
      error.message.includes('Object helpers currently only support plain objects and arrays'),
  );
});
