'use strict';

const { assert, Progress, runtime, test } = require('./support/helpers.js');

test('typeof unresolvable identifiers preserves TDZ, property errors, and ambient restrictions', async () => {
  assert.equal(await runtime('typeof missingName;').run(), 'undefined');
  assert.equal(await runtime('const x = 1; (() => typeof x)();').run(), 'number');
  assert.equal(await runtime('globalThis.extra = "x"; typeof extra;').run(), 'string');
  await assert.rejects(runtime('{ typeof value; let value = 1; }').run(), /before initialization/);
  await assert.rejects(runtime('typeof missingName.value;').run(), /not defined/);
  assert.throws(() => runtime('typeof process;'), /forbidden ambient global/);
});

test('delete preserves absence, sparse arrays, property order, and cached reads', async () => {
  const source = `
    const object = { a: 1, b: 2, c: 3 };
    function read() { return object.b; }
    read(); read();
    const removed = delete object.b;
    const missing = read() === undefined && !Object.hasOwn(object, "b") && !("b" in object);
    object.b = 4;
    const array = [1, 2, 3];
    delete array[1];
    let calls = 0;
    const skipped = delete null?.[calls++];
    [removed, missing, read(), Object.keys(object), array.length, 1 in array,
      Object.keys(array), JSON.stringify(array), [...array], skipped, calls];
  `;
  assert.deepEqual(await runtime(source).run(), [true, true, 4, ['a', 'c', 'b'], 3, false,
    ['0', '2'], '[1,null,3]', [1, undefined, 3], true, 0]);
  for (const source of ['delete [1].length', 'delete "x"[0]', 'delete null.x', 'delete globalThis.Math']) {
    await assert.rejects(runtime(source).run(), /TypeError/);
  }
  assert.throws(() => runtime('delete value;'), /delete of an identifier/);
});

test('deletion and subsequent property reads survive snapshot round-trips', () => {
  const snapshotKey = Buffer.from('language-completion-test-key');
  const capabilities = { checkpoint() {} };
  const first = runtime(`
    const object = { a: 1, b: 2 };
    const array = [1, 2, 3];
    delete object.a; delete array[1];
    checkpoint();
    const absent = !("a" in object) && !(1 in array);
    object.a = 4; delete array[0];
    [absent, Object.keys(object), array.length, Object.keys(array), JSON.stringify(array)];
  `).start({ capabilities, snapshotKey });
  const restored = Progress.load(first.dump(), { capabilities, snapshotKey, limits: {} });
  assert.deepEqual(restored.resume(undefined), [true, ['b', 'a'], 3, ['2'], '[null,null,3]']);
});
