const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError, Progress } = require('../../index.js');

test('run supports array for...of with fresh iteration bindings', async () => {
  const runtime = new Jslite(`
    const fns = [];
    for (const [value] of [[1], [2]]) {
      fns[fns.length] = () => value;
    }
    [fns[0](), fns[1]()];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [1, 2]);
});

test('progress snapshots preserve active array iterators across resumes', () => {
  const runtime = new Jslite(`
    let total = 0;
    for (const value of [1, 2, 3]) {
      total += fetch_data(value);
    }
    total;
  `);

  const first = runtime.start({
    capabilities: {
      fetch_data(value) {
        return value * 10;
      },
    },
  });

  assert.ok(first instanceof Progress);
  assert.equal(first.capability, 'fetch_data');
  assert.deepEqual(first.args, [1]);

  const restored = Progress.load(first.dump());
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

test('run supports strings, keyed collections, and iterator helper objects', async () => {
  const runtime = new Jslite(`
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

test('run rejects unsupported for...of iterable inputs', async () => {
  const runtime = new Jslite(`
    for (const value of { alpha: 1 }) {
      value;
    }
  `);

  await assert.rejects(
    () => runtime.run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('value is not iterable in the supported surface'),
  );
});
