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

test('JSON replacers, revivers, indentation, and live mutations match Node', async () => {
  const vm = require('node:vm');
  const cases = [
    'JSON.stringify({b: [1, {a: 2}], a: 3}, ["a", "b", "a"], 2);',
    'JSON.stringify({x: 1, y: {}}, null, "abcdefghijk");',
    'JSON.stringify({x: 1}, null, Infinity);',
    'JSON.stringify({x: 1}, null, new Number(2));',
    'JSON.stringify({x: 1, y: 2}, [new String("x"), true, null, "x"], new String("--"));',
    'JSON.stringify({x: new Number(2), y: new String("x"), z: new Boolean(false)});',
    'JSON.stringify({x: {toJSON(key) { return key; }}, y: new Date(0)}, (key, value) => value);',
    'JSON.stringify({x: 1}, function(key, value) { return key === "" ? [value] : value; }, 1);',
    'JSON.stringify({a: undefined, b: () => 1, c: []}, () => undefined, 2);',
    'JSON.stringify(JSON.parse("[1,2,3]", (key, value) => key === "1" ? undefined : value));',
    'JSON.stringify(JSON.parse("1", () => undefined));',
    `JSON.stringify(JSON.parse('{"__proto__":1}', (key, value) => value));`,
    `const o = {a: 1, b: 2, c: 3}; const seen = [];
     const text = JSON.stringify(o, function(k, v) {
       seen.push(k); if (k === 'a') { delete this.b; this.c = 4; }
       return typeof v === 'number' ? v * 2 : v;
     }); JSON.stringify([text, seen]);`,
    `const seen = []; const value = JSON.parse('{"a":[1,2],"b":3}', function(k,v) {
       seen.push(k); if (k === '0' || k === 'b') return undefined; return v;
     }); JSON.stringify([value, seen, Object.keys(value)]);`,
    `const a = [1,2,3]; JSON.stringify(a, function(k,v) { if (k === '0') { a[1] = 4; a.push(5); } return v; });`,
    'const x = "🙂a".repeat(200); JSON.parse(JSON.stringify(x));',
  ];
  for (const source of cases) {
    assert.equal(await runtime(source).run(), vm.runInNewContext(source), source);
  }
  assert.equal(await runtime('(() => { try { JSON.parse("{"); } catch (e) { return e instanceof SyntaxError; } })()').run(), true);
  await assert.rejects(runtime('JSON.stringify({x:1}, () => { throw new Error("callback failed"); });').run(), /callback failed/);
  await assert.rejects(runtime('JSON.parse("1", () => { throw new Error("reviver failed"); });').run(), /reviver failed/);
  await assert.rejects(runtime('const x = {}; x.self = x; JSON.stringify(x, null, 2);').run(), /circular structure/);
});

test('JSON callbacks preserve failures and cannot bypass runtime limits', async () => {
  await assert.rejects(runtime('JSON.stringify({x:1}, () => checkpoint());').run({
    capabilities: { checkpoint() { return 1; } },
  }), /JSON callbacks do not support synchronous host suspensions/);
  await assert.rejects(runtime('JSON.parse("1", () => checkpoint());').run({
    capabilities: { checkpoint() { return 1; } },
  }), /JSON callbacks do not support synchronous host suspensions/);
  await assert.rejects(runtime('JSON.stringify(values, (k, v) => v, 2);').run({
    inputs: { values: Array.from({length: 500}, (_, i) => i) },
    limits: { instructionBudget: 200 },
  }), /instruction budget exhausted/);
  const text = await runtime(`
    let total = 0;
    const object = {a: {x: 1}, b: {x: 2}};
    JSON.stringify(object, function(k,v) {
      for (let i=0; i<100; i++) { const garbage = {array:[i,i,i]}; total += garbage.array[0]; }
      return v;
    });
  `).run({ limits: { heapLimitBytes: 32768 } });
  assert.equal(text, '{"a":{"x":1},"b":{"x":2}}');
});

test('error constructors, AggregateError, and guest-only stacks are complete', async () => {
  const source = `
    const rows = [Error, TypeError, ReferenceError, RangeError, SyntaxError, EvalError, URIError].map(C => {
      const error = new C("message", {cause: 1});
      return [error instanceof C, error instanceof Error, error.constructor === C,
        error.name === C.name, error.cause, error.toString(), Object.keys(error), typeof error.stack, C.length];
    });
    const error = new AggregateError([1,2], "failed", {cause: 3});
    JSON.stringify([rows, [error instanceof AggregateError, error instanceof Error, error.errors,
      error.cause, error.toString(), Object.keys(error), AggregateError.length]]);
  `;
  assert.equal(await runtime(source).run(), require('node:vm').runInNewContext(source));
  const stack = await runtime('function guestFailure() { return new URIError("bad"); } guestFailure().stack;').run();
  assert.match(stack, /^URIError: bad\n/);
  assert.match(stack, /guestFailure \(guest:\d+\.\.\d+\)/);
  assert.ok(!stack.includes(process.cwd()));
  assert.ok(!stack.includes('.rs:'));
  assert.equal(await runtime(`
    (async () => { try { await Promise.any([Promise.reject(1)]); }
      catch(error) { return error instanceof AggregateError && error instanceof Error; } })();
  `).run(), true);
  await assert.rejects(runtime('new AggregateError({length:1});').run(), /not iterable/);
});

test('error stacks and nested causes survive snapshot restore', () => {
  const snapshotKey = Buffer.from('language-error-stack-test');
  const capabilities = { checkpoint() {} };
  const first = runtime(`
    function failure() { return new AggregateError([new URIError("inner")], "outer", {cause: "cause"}); }
    const error = failure(); const stack = error.stack;
    checkpoint();
    [error instanceof AggregateError, error.errors[0] instanceof URIError,
      error.cause, error.stack === stack, error.stack.includes("failure")];
  `).start({ snapshotKey, capabilities });
  const restored = Progress.load(first.dump(), { snapshotKey, capabilities, limits: {} });
  assert.deepEqual(restored.resume(undefined), [true, true, 'cause', true, true]);
});
