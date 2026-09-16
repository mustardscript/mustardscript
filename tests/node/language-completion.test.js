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

test('array-like construction and queue/copy helpers match Node', async () => {
  const cases = [
    'JSON.stringify(Array.from({length: 5}, (_, i) => i));',
    'JSON.stringify(Array.from({0: "a", 2: "c", length: 3.9}));',
    'JSON.stringify([Array.from(1), Array.from({}), Array.from({length: -1})]);',
    'JSON.stringify(Array.from(new String("ab")));',
    'JSON.stringify(Array.from({length: 3}, function(v,i) { return i + this.x; }.bind({x: 4})));',
    `const a={0:1,1:2,length:3}; JSON.stringify(Array.from(a, function(v,i) { if (i===0) { a[1]=4; a.length=1; } return v; }));`,
    'const a=[,2,3]; JSON.stringify([a.shift(),a.unshift(0,1),a,Object.keys(a)]);',
    'const a=[]; JSON.stringify([a.shift(), a.unshift(), a.unshift(1,2), a]);',
    'const a=[,undefined,3,1]; JSON.stringify([a.toSorted(),a,Object.keys(a.toSorted())]);',
    'const a=[,undefined,3,1]; JSON.stringify(a.toSorted((a,b) => a-b));',
    'JSON.stringify([,1,2].toReversed());',
    'JSON.stringify([,1,2].toSpliced(1));',
    'JSON.stringify([,1,2].toSpliced());',
    'JSON.stringify([,1,2].toSpliced(-2, 1, 4, 5));',
    'JSON.stringify([,1,2].toSpliced(0, Infinity));',
    'JSON.stringify([,1,2].with(-1,4));',
    'const a=[,undefined,3,1]; a.copyWithin(1,0,3); JSON.stringify([a,Object.keys(a)]);',
    'const a=[1,2,3,4]; a.copyWithin(0,1); JSON.stringify(a);',
    'const a=[1,2,3,4]; a.copyWithin(-2,0,2); JSON.stringify(a);',
    'JSON.stringify(Array.prototype.toSorted.call([3,1,2]));',
    'const a=[1,2,3]; JSON.stringify([a.splice(1),a]);',
    `const a=[3,1,2]; let once=false; const b=a.toSorted((x,y)=> { if (!once) { a[0]=9; once=true; } return x-y; }); JSON.stringify([a,b]);`,
    `const a=[3,1,2]; let once=false; a.sort((x,y)=> { if (!once) { a.length=0; once=true; } return x-y; }); JSON.stringify(a);`,
    `const a=[undefined,3,undefined,1]; let ok=true; a.sort((x,y)=> { if (x===undefined || y===undefined) ok=false; return x-y; }); JSON.stringify([a,ok]);`,
  ];
  for (const source of cases) assert.equal(await runtime(source).run(), require('node:vm').runInNewContext(source), source);
  for (const source of ['Array.from(null)', 'Array.from({}, 3)', 'Array.prototype.shift.call({})', '[].with(0)', '[1].with(Infinity)']) {
    await assert.rejects(runtime(source).run(), /TypeError|RangeError/);
  }
  for (const source of ['Array.from({length:1e9})', 'new Array(1e9)', 'const a=[]; a[1e9]=1', 'const a=[]; a.length=1e9']) {
    await assert.rejects(runtime(source).run(), /heap limit exceeded/);
  }
});

test('array helpers and their builtin identities survive snapshots', () => {
  const snapshotKey = Buffer.from('language-array-test');
  const capabilities = { checkpoint() {} };
  const first = runtime(`
    const source = [,3,1]; const copy = source.toSorted();
    const method = Array.prototype.toSpliced;
    checkpoint();
    [copy, Object.keys(copy), method.call(source,1,1,4), source.shift(), source.unshift(2), source];
  `).start({snapshotKey, capabilities});
  const restored = Progress.load(first.dump(), {snapshotKey, capabilities, limits: {}});
  assert.deepEqual(restored.resume(undefined), [[1,3,undefined], ['0','1','2'], [undefined,4,1], undefined, 3, [2,3,1]]);
});

test('groupBy handles ordered keys, object identity, and prototype-less results', async () => {
  const sources = [
    'JSON.stringify(Object.groupBy([1,2,3,4], (v,i)=>i%2));',
    'JSON.stringify(Object.groupBy("aba", value=>value));',
    'JSON.stringify(Object.groupBy(new Set([3,1,2]), value=>value%2));',
    'JSON.stringify(Object.groupBy(["__proto__","constructor","toString"], value=>value));',
    'const g=Object.groupBy([], value=>value); JSON.stringify([g.constructor===undefined, !("toString" in g), g instanceof Object]);',
    'const g=Object.groupBy([1,2], value=>value); const copy={...g}; delete g[1]; JSON.stringify([g,copy,Object.keys(g)]);',
    'JSON.stringify(Object.groupBy([1,2,3], function(value) { return value % this.n; }.bind({n:2})));',
    'const a={},b={}; const g=Map.groupBy([a,b,a], value=>value); const keys=[...g.keys()]; JSON.stringify([g.size,keys[0]===a,keys[1]===b,g.get(a).length]);',
    'const g=Map.groupBy([NaN,-0,NaN,0], value=>value); JSON.stringify([g.size,g.get(NaN).length,g.get(0).length,1/[...g.keys()][1]===Infinity]);',
    'const seen=[]; const g=Map.groupBy([1,2,3], (v,i)=>{seen.push(i); return v%2;}); JSON.stringify([[...g.entries()],seen]);',
  ];
  for (const source of sources) assert.equal(await runtime(source).run(), require('node:vm').runInNewContext(source), source);
  const result = await runtime('Object.groupBy(["__proto__","constructor"], v=>v);').run();
  assert.ok(Object.hasOwn(result, '__proto__'));
  assert.deepEqual(result.__proto__, ['__proto__']);
  assert.deepEqual(result.constructor, ['constructor']);
  for (const source of ['Object.groupBy({},v=>v)', 'Object.groupBy([],null)', 'Map.groupBy(null,v=>v)', 'String(Object.groupBy([],v=>v))']) {
    await assert.rejects(runtime(source).run(), /TypeError/);
  }
  await assert.rejects(runtime('Object.groupBy([1], ()=>{throw new Error("group callback");});').run(), /group callback/);
  await assert.rejects(runtime('Map.groupBy([1], ()=>checkpoint());').run({capabilities:{checkpoint(){return 1;}}}), /groupBy callbacks do not support synchronous host suspensions/);
});

test('grouping protects keys and values under GC pressure and snapshots', async () => {
  const result = await runtime(`
    const values=Array.from({length:100}, (_,i)=>({i}));
    const groups=Map.groupBy(values, value=> {
      for(let i=0;i<50;i++) { const garbage={text:"x".repeat(100)}; }
      return {key:value.i};
    });
    [...groups.entries()].every(entry=>entry[0].key===entry[1][0].i) && groups.size===100;
  `).run({limits:{heapLimitBytes:128*1024}});
  assert.equal(result, true);
  const snapshotKey=Buffer.from('group-by-snapshot-test');
  const capabilities={checkpoint(){}};
  const first=runtime(`
    const key={x:1}; const map=Map.groupBy([key,key], v=>v);
    const object=Object.groupBy([1,2], v=>v%2);
    checkpoint();
    [map.get(key).length, Object.keys(object), object.constructor===undefined, object instanceof Object];
  `).start({snapshotKey, capabilities});
  const restored=Progress.load(first.dump(), {snapshotKey, capabilities, limits:{}});
  assert.deepEqual(restored.resume(undefined), [2,['0','1'],true,false]);
});

test('Object compatibility helpers and explicit array stringification match Node', async () => {
  const vm = require('node:vm');
  const cases = [
    `const same = {}; [Object.is(NaN, NaN), Object.is(-0, 0), Object.is(-0, -0), Object.is(same, same), Object.is({}, {}), Object.is(1n, 1n), Object.is()];`,
    `const o = { x: undefined }; const own = Object.prototype.hasOwnProperty; [o.hasOwnProperty('x'), o.hasOwnProperty('toString'), Object.hasOwn('abc', '1'), own.call(1, 'x'), own.call(new Map(), 'size'), own.call(Object.prototype, 'hasOwnProperty'), own.call(Array.prototype, 'toString'), own.call(Array.prototype, 'hasOwnProperty')];`,
    `const tag = Object.prototype.toString; [undefined, null, 1, true, 1n, 'x', [], {}, new Map(), new Set(), new Date(0), /x/, new TypeError('x'), Math, JSON, Promise.resolve(1)].map(x => tag.call(x));`,
    `const a = [1, , undefined, null, [2,3]]; a.push(a); [a.toString(), a.join(undefined), String(a), Array.prototype.toString.call({join() { return this.x; }, x: 42}), Array.prototype.toString.call({join: 2})];`,
    `const o = {x: 1}; function read() { return o.hasOwnProperty('x'); } let n = 0; for (let i = 0; i < 80; i++) n += read(); o.hasOwnProperty = undefined; [n, o.hasOwnProperty, ({}).toString(), 'hasOwnProperty' in [], 'toString' in {}];`,
    `const grouped = Object.groupBy([1], () => 'x'); [grouped.hasOwnProperty, grouped.toString, Object.prototype.hasOwnProperty.call(grouped, 'x'), Object.prototype.toString.call(grouped)];`,
  ];
  for (const source of cases) {
    const expected = vm.runInNewContext(source);
    assert.deepEqual(JSON.parse(JSON.stringify(await runtime(source).run())), JSON.parse(JSON.stringify(expected)), source);
  }
  for (const source of ['Object.hasOwn(null, "x");', 'Object.prototype.hasOwnProperty.call(undefined, "x");', 'Array.prototype.toString.call(null);']) {
    await assert.rejects(runtime(source).run(), /TypeError/);
  }
  await assert.rejects(runtime('let a = []; for (let i = 0; i < 140; i++) a = [a]; a.toString();').run(), /nesting limit/);
});

test('URI codecs match Node and reject malformed percent/UTF-8 sequences', async () => {
  const samples = [undefined, null, true, -0, NaN, Infinity, -Infinity, 1e21, 1e-7,
    '', "AZaz09-_.!~*'();/?:@&=+$,# %", 'é🙂片\u0000\n', 'https://x.test/a b?q=é🙂&v=+%#片'];
  for (const name of ['encodeURI', 'encodeURIComponent']) {
    for (const value of samples) {
      const sourceValue = typeof value === 'number' ? String(value) : JSON.stringify(value) ?? 'undefined';
      assert.equal(await runtime(`${name}(${sourceValue});`).run(), globalThis[name](value));
    }
  }
  for (const name of ['decodeURI', 'decodeURIComponent']) {
    for (const value of ['a+b', '%2f%3F%23%2b%20%25', '%00%7F%C2%80%F4%8F%BF%BF', 'é%E7%89%87🙂']) {
      assert.equal(await runtime(`${name}(${JSON.stringify(value)});`).run(), globalThis[name](value));
    }
    for (const value of ['%', '%0', '%GG', '%FF', '%80', '%C0%AF', '%E0%80%AF', '%ED%A0%80', '%F4%90%80%80', '%F0%9F%99', '%C2x', '%E2%28%A1']) {
      assert.equal(await runtime(`(() => { try { ${name}(${JSON.stringify(value)}); return false; } catch (e) { return e instanceof URIError; } })();`).run(), true);
    }
  }
  const source = `const text = '🙂'.repeat(2000); encodeURIComponent(text);`;
  await assert.rejects(runtime(source).run({limits: {instructionBudget: 2000}}), /instruction budget/);
});

test('Unicode APIs match Node over well-formed strings and reject unsupported surrogates', async () => {
  const vm = require('node:vm');
  const cases = [
    `const s = 'A🙂é𝄞'; [-1,0,1,2,3,4,5,6,7,Infinity,NaN].map(i => [s.charCodeAt(i), s.codePointAt(i)]);`,
    `['NFC','NFD','NFKC','NFKD'].map(form => 'éﬃÅ각a\u0315\u0300'.normalize(form));`,
    `[String.fromCharCode('0x41',65537,-65535,Infinity,0xD83D,0xDE42), String.fromCodePoint(0,0x1F642,0x10FFFF), ''.isWellFormed(), '�'.isWellFormed(), 'x'.normalize(), String.prototype.normalize.call(123)];`,
    `['', '  ', '\uFEFF1\u00a0', '0xFF', '0b11', '0o77', '+0x1', 'inf', '1e2', '\u0085'].map(Number);`,
  ];
  for (const source of cases) {
    assert.deepEqual(JSON.parse(JSON.stringify(await runtime(source).run())), JSON.parse(JSON.stringify(vm.runInNewContext(source))), source);
  }
  for (const source of [String.raw`'\ud800';`, String.raw`'\udfff';`, String.raw`({'\ud800': 1});`, '`\\ud800`;']) {
    assert.throws(() => runtime(source), /lone surrogates/, source);
  }
  for (const source of ['String.fromCharCode(0xD800);', 'String.fromCodePoint(0xDFFF);', 'String.fromCodePoint(0x110000);', 'String.fromCodePoint(1.5);', 'String.fromCodePoint(NaN);', "'x'.normalize('bad');"]) {
    await assert.rejects(runtime(source).run(), /RangeError/);
  }
  await assert.rejects(runtime('text.normalize();').run({inputs: {text: 'a' + '\u0301'.repeat(10000)}, limits: {instructionBudget: 2000}}), /instruction budget/);
  await assert.rejects(runtime('text;').run({inputs: {text: '\ud800'}}), /lone surrogates/);
  await assert.rejects(runtime('text;').run({inputs: {text: {'\ud800': 1}}}), /lone surrogates/);
  assert.throws(() => runtime("'\ud800';"), /lone surrogates/);
  await assert.rejects(runtime('checkpoint();').run({capabilities: {checkpoint() { return '\ud800'; }}}), /lone surrogates/);
});

test('linear RegExp classes and d indices match Node over supported offsets', async () => {
  const vm = require('node:vm');
  const cases = [
    String.raw`[/\w/.test('é'), /\d/.test('١'), /\babc\b/.test('éabcé'), /[\b]/.test('\b'), /\s/.test('\uFEFF'), /\s/.test('\u0085'), /./.test('\r'), /./.test('\u2028')];`,
    String.raw`['', 'i', 'u', 'iu'].map(flags => ['s', 'k', '\\w', '[\\w]', '[\\W_]', '[^s\\W]', '[a-z]'].map(pattern => ['ſ', 'K', 'é', 's', 'S', 'K', '_', '1'].map(text => new RegExp(pattern, flags).test(text))));`,
    String.raw`const m = /(?<word>\w+)(-(?<digits>\d+))?/d.exec('éabc!'); [m.index, m.indices, m.indices.groups.word === m.indices[1], m.indices.groups.digits, m.groups.constructor, Object.hasOwn(m, 'groups'), Object.hasOwn(m.indices, 'groups')];`,
    String.raw`[... 'a12 b3'.matchAll(/(?<name>[a-z])(\d+)/dg)].map(m => [m.index, m.indices, m.groups.name, m.indices.groups.name.slice()]);`,
    String.raw`const r = /(?<x>a)/dg; r.lastIndex = 2; const rows = [...'a a'.matchAll(r)].map(m => [m.index, m.indices[0]]); [rows, r.lastIndex];`,
    String.raw`const r = new RegExp('x', 'ygdi'); const empty = /(?:)/dg; empty.exec('x'); const beyond = /(?:)/dg; beyond.lastIndex = 2; [r.flags, r.hasIndices, empty.lastIndex, beyond.exec('x'), beyond.lastIndex];`,
  ];
  for (const source of cases) {
    assert.deepEqual(JSON.parse(JSON.stringify(await runtime(source).run())), JSON.parse(JSON.stringify(vm.runInNewContext(source))), source);
  }
  assert.deepEqual(await runtime(String.raw`const m = /(?<x>é)/dg.exec('🙂é'); [m.index, m.indices[0], m.indices.groups.x];`).run(), [1, [1,2], [1,2]]);
  await assert.rejects(runtime(String.raw`/\b\w+\b/iu.test('ſ');`).run(), /iu word boundaries/);
  await assert.rejects(runtime(String.raw`new RegExp('(?i)a');`).run(), /inline flags/);
  await assert.rejects(runtime(String.raw`new RegExp('a', 'dd');`).run(), /duplicate regular expression flag/);
});

test('RegExp indices retain named-group pair identity across snapshots and GC', () => {
  const capabilities = {checkpoint() {}};
  const snapshotKey = Buffer.from('language-completion-test-key');
  const first = runtime(String.raw`
    const regex = /(?<x>a)(b)?/dg;
    const match = regex.exec('za');
    checkpoint();
    for (let i = 0; i < 800; i++) { const garbage = {x: [i, i]}; }
    [match.indices.groups.x === match.indices[1], match.indices[0], match.indices[2],
      match.groups.constructor, regex.hasIndices, regex.lastIndex, regex.exec('za')];
  `).start({capabilities, snapshotKey, limits: {heapLimitBytes: 128 * 1024}});
  const restored = Progress.load(first.dump(), {capabilities, snapshotKey, limits: {}});
  assert.deepEqual(restored.resume(undefined), [true, [1,2], undefined, undefined, true, 2, null]);
});


test('native receivers and intermediate regexp results survive GC-triggering work', async () => {
  assert.equal(await runtime(String.raw`
    let count = 0;
    for (let i = 0; i < 350; i++) {
      const garbage = {data: [i, i, i, i]};
      const m = new RegExp('(?<x>a)(b)?', 'd').exec('a');
      if (m.indices.groups.x === m.indices[1] && m[1] === 'a') count++;
    }
    count;
  `).run({limits: {heapLimitBytes: 128 * 1024}}), 350);
  assert.deepEqual(await runtime(String.raw`
    for (let i = 0; i < 200; i++) { const garbage = [i, {x:i}]; }
    [...'ab '.repeat(30).matchAll(/(?<x>a)(b)/dg)].map(m => [m[1], m.indices[0][0], m.indices.groups.x === m.indices[1]]);
  `).run({limits: {heapLimitBytes: 128 * 1024}}), Array.from({length:30}, (_, i) => ['a', i * 3, true]));
  assert.equal(await runtime('function f() { return this === undefined; } f.call();').run(), true);
});

test('Number bitwise operators and compound assignments match Node coercion and ordering', async () => {
  const values = [undefined, null, true, false, NaN, Infinity, -Infinity, -0, 0, 1, -1,
    31, 32, 33, -33, 9.9, -9.9, 2 ** 31, 2 ** 32 - 1, 2 ** 32 + 1,
    -(2 ** 32) - 1, 1e30, -1e30, 1e-20, '', '  ', '0xff', '0b11', '-9.5', 'oops'];
  const source = `values.flatMap(a => values.map(b => [a & b, a | b, a ^ b, a << b, a >> b, a >>> b]));`;
  assert.deepEqual(await runtime(source).run({inputs: {values}}), values.flatMap(a => values.map(b => [a & b, a | b, a ^ b, a << b, a >> b, a >>> b])));
  assert.deepEqual(await runtime('values.map(x => ~x);').run({inputs: {values}}), values.map(x => ~x));
  const ordering = `
    const object = {x: 4294967295}; let trace = '';
    function base() { trace += 'B'; return object; }
    function key() { trace += 'K'; return 'x'; }
    function rhs() { trace += 'R'; object.x = 0; return 1; }
    const result = base()[key()] >>>= rhs();
    let x = 6; x &= 3; x |= 8; x ^= 1; x <<= 32; x >>= 1; x >>>= 0;
    [result, object.x, trace, x];
  `;
  assert.deepEqual(await runtime(ordering).run(), [2147483647, 2147483647, 'BKR', 5]);
  for (const source of ['~1n;', '1n & 1n;', '1 | 1n;', '1n << 2;', 'let x = 1n; x >>>= 1;']) {
    await assert.rejects(runtime(source).run(), /TypeError/);
  }
});

test('bitwise bytecode round-trips through authenticated snapshots', () => {
  const capabilities = {checkpoint() {}};
  const snapshotKey = Buffer.from('language-completion-test-key');
  const first = runtime('let x = -1; checkpoint(); x >>>= 1; [x, x << 1, ~x, x & 255];').start({capabilities, snapshotKey});
  const restored = Progress.load(first.dump(), {capabilities, snapshotKey, limits: {}});
  assert.deepEqual(restored.resume(undefined), [2147483647, -2, -2147483648, 255]);
});

test('BigInt conversion and explicit radix formatting match Node for supported inputs', async () => {
  const vm = require('node:vm');
  const cases = [
    `['', '  ', '0xFF', '0o77', '0b11', '+0012', '-0012', '\uFEFF42\u00A0', '9'.repeat(100)].map(v => BigInt(v).toString());`,
    `[0, -0, true, false, 1e20, 1e100, -1e100, new Number(2), new String('16')].map(v => BigInt(v).toString());`,
    `const n = BigInt('18446744073709551616'); [n === 18446744073709551616n, n.toString(2), n.toString(16), n.toString(36), (-10n).toString(2), n.valueOf() === n];`,
    `[BigInt(0).toString(), typeof BigInt, BigInt.name, BigInt.length, Object.hasOwn(BigInt.prototype, 'toString'), Object.prototype.toString.call(BigInt.prototype), (1n instanceof BigInt)];`,
  ];
  for (const source of cases) {
    assert.deepEqual(JSON.parse(JSON.stringify(await runtime(source).run())), JSON.parse(JSON.stringify(vm.runInNewContext(source))), source);
  }
  for (const [source, name] of [
    ['BigInt(1.5);', 'RangeError'], ['BigInt(Infinity);', 'RangeError'], ['BigInt(NaN);', 'RangeError'],
    ["BigInt('1.0');", 'SyntaxError'], ["BigInt('-0xff');", 'SyntaxError'], ["BigInt('0x');", 'SyntaxError'], ["BigInt('1e3');", 'SyntaxError'],
    ['BigInt.prototype.toString();', 'TypeError'], ['BigInt.prototype.valueOf();', 'TypeError'], ['BigInt();', 'TypeError'], ['BigInt(null);', 'TypeError'], ['new BigInt(1);', 'TypeError'],
    ['BigInt.prototype.toString.call(1);', 'TypeError'], ['(1n).toString(1);', 'RangeError'],
  ]) {
    await assert.rejects(runtime(source).run(), new RegExp(name));
  }
  await assert.rejects(runtime('BigInt(text);').run({inputs: {text: '9'.repeat(10000)}, limits: {instructionBudget: 20000}}), /instruction budget/);
  await assert.rejects(runtime('BigInt(1);').run(), /BigInt.*boundary|BigInt.*cross/i);
  await assert.rejects(runtime('text;').run({inputs: {text: 1n}}), /Unsupported host value/);
});

test('converted BigInts retain exact value across snapshots', () => {
  const capabilities = {checkpoint() {}};
  const snapshotKey = Buffer.from('language-completion-test-key');
  const first = runtime(`let n = BigInt('9999999999999999999999999999999999999999'); checkpoint(); n += 2n; n.toString();`).start({capabilities, snapshotKey});
  const restored = Progress.load(first.dump(), {capabilities, snapshotKey, limits: {}});
  assert.equal(restored.resume(undefined), '10000000000000000000000000000000000000001');
});

test('remaining Math helpers preserve special values and numerical accuracy', async () => {
  const names = ['tan', 'asin', 'acos', 'atan', 'sinh', 'cosh', 'tanh', 'asinh', 'acosh', 'atanh', 'fround', 'clz32', 'log1p', 'expm1'];
  const values = [undefined, null, false, true, 0, -0, 1, -1, 0.5, -0.5, 1 + Number.EPSILON,
    1e-20, -1e-20, 1e-100, -1e-100, 1e200, -1e200, 1000, -1000, Number.MAX_VALUE,
    Number.MIN_VALUE, NaN, Infinity, -Infinity, '0x10', '  ', 'bad', 16777217, 4294967295];
  const actual = await runtime('names.map(name => values.map(value => Math[name](value)));').run({inputs: {names, values}});
  for (let row = 0; row < names.length; row++) {
    for (let col = 0; col < values.length; col++) {
      const expected = Math[names[row]](values[col]);
      const value = actual[row][col];
      const label = `${names[row]}(${String(values[col])})`;
      if (Number.isNaN(expected)) assert.ok(Number.isNaN(value), label);
      else if (!Number.isFinite(expected) || expected === 0 || ['fround', 'clz32'].includes(names[row])) assert.ok(Object.is(value, expected), label);
      else assert.ok(Math.abs(value - expected) <= Math.max(4 * Number.MIN_VALUE, Math.abs(expected) * 32 * Number.EPSILON), `${label}: ${value} vs ${expected}`);
    }
  }
  const pairs = [0, -1, 1, 0xffffffff, 0x80000000, 2 ** 32 + 1, Infinity, 1.9, '0xff'];
  assert.deepEqual(await runtime('values.flatMap(a => values.map(b => Math.imul(a,b)));').run({inputs: {values:pairs}}), pairs.flatMap(a => pairs.map(b => Math.imul(a,b))));
  assert.equal(await runtime(`let hash = 0x811c9dc5; const text = 'hello'; for (let i = 0; i < text.length; i++) hash = Math.imul(hash ^ text.charCodeAt(i), 16777619); hash >>> 0;`).run(), 1335831723);
  for (const source of ['Math.imul(1n,1);', 'Math.clz32(0n);', 'Math.fround(1n);', 'Math.acos(0n);']) await assert.rejects(runtime(source).run(), /TypeError/);
  await assert.rejects(runtime('Math.fround(text);').run({inputs: {text: '0'.repeat(10000)}, limits: {instructionBudget: 200}}), /instruction budget/);
});

test('Set algebra matches Node for order, identity, live mutation and set-like records', async () => {
  const vm = require('node:vm');
  const methods = ['union', 'intersection', 'difference', 'symmetricDifference', 'isSubsetOf', 'isSupersetOf', 'isDisjointFrom'];
  const cases = [
    `const a = new Set([3, 1, 2]); const b = new Set([2, 3, 4]); methods.map(m => { const r = a[m](b); return typeof r === 'boolean' ? r : [...r]; });`,
    `const a = new Set([3, 1, 2]); const b = new Map([[2, 9], [1, 8]]); methods.map(m => { const r = a[m](b); return typeof r === 'boolean' ? r : [...r]; });`,
    `const x = {}; const a = new Set([x, NaN, -0]); const b = new Set([0, x, NaN]); methods.map(m => { const r = a[m](b); return typeof r === 'boolean' ? r : [r.size, r.has(x), r.has(NaN), r.has(0), [...r].some(v => Object.is(v, -0))]; });`,
    `methods.map(m => { const a = new Set([1, 2, 3]); const seen = []; const b = {size: 20, has(v) { seen.push(v); if (v === 1) { a.delete(2); a.delete(3); a.add(4); } return true; }, keys() { return [2, 5, 5].values(); }}; const r = a[m](b); return [typeof r === 'boolean' ? r : [...r], seen, [...a]]; });`,
    `methods.map(m => { const a = new Set([1, 2]); const b = {size: 1, has(v) { return v === 2; }, keys() { a.add(7); a.delete(1); return [2, 3, 3].values(); }}; const r = a[m](b); return typeof r === 'boolean' ? r : [...r]; });`,
    `const a = new Set([1, 2]); const b = {size: Infinity, has: async v => false, keys() { return [].values(); }}; [...a.intersection(b)];`,
    `const a = new Set([1, 2]); const iterator = a.values(); iterator.next(); iterator.next(); a.delete(2); a.add(3); [iterator.next(), [...a]];`,
    `methods.map(m => [m in new Set(), Object.hasOwn(Set.prototype, m), Object.hasOwn(new Set(), m), Set.prototype[m].name, Set.prototype[m].length]);`,
  ];
  for (const source of cases) {
    const actual = await runtime(source).run({inputs: {methods}});
    const expected = vm.runInNewContext(source, {methods});
    assert.deepEqual(JSON.parse(JSON.stringify(actual)), JSON.parse(JSON.stringify(expected)), source);
  }
  for (const size of [-1, -Infinity, NaN, undefined]) {
    const name = size < 0 ? 'RangeError' : 'TypeError';
    await assert.rejects(runtime('new Set().union({size, has() {}, keys() { return [].values(); }});').run({inputs:{size}}), new RegExp(name));
  }
  await assert.rejects(runtime('new Set().union({size: 0, has() {}, keys() { return {next() {return {done: true};}}; }});').run(), /supported native iterator/);
  await assert.rejects(runtime('new Set([1]).intersection({size: 2, has() { throw new URIError("boom"); }, keys() {return [].values();}});').run(), /URIError: boom/);
  await assert.rejects(runtime('new Set([1]).intersection({size: 2, has: checkpoint, keys() {return [].values();}});').run({capabilities: {checkpoint() { throw new Error('must not run'); }}}), /synchronous host suspensions/);
  await assert.rejects(runtime('const a = new Set([0]); a.intersection({size: Infinity, has(v) { a.add(v+1); return true; }, keys() {return [].values();}});').run({limits: {instructionBudget: 10000}}), /instruction budget/);
});

test('Set algebra roots callback values under GC and survives snapshots', async () => {
  const source = `const a = new Set(Array.from({length: 40}, (_, i) => ({i}))); const b = {size: Infinity, has(v) { for(let i=0;i<60;i++) { const garbage = {text: 'x'.repeat(200)}; } return v.i % 2 === 0; }, keys() { return [].values(); }}; [...a.intersection(b)].map(v => v.i);`;
  assert.deepEqual(await runtime(source).run({limits: {heapLimitBytes: 128 * 1024}}), Array.from({length: 20}, (_, i) => i*2));
  const capabilities = {checkpoint() {}};
  const snapshotKey = Buffer.from('language-completion-test-key');
  const first = runtime('const a = new Set([1, 2]); const operation = a.union; checkpoint(); [...operation.call(a, new Set([2, 3]))];').start({capabilities, snapshotKey});
  const restored = Progress.load(first.dump(), {capabilities, snapshotKey, limits: {}});
  assert.deepEqual(restored.resume(undefined), [1, 2, 3]);
});

test('labeled exits and nested finally completions match Node', async () => {
  const vm = require('node:vm');
  const cases = [
    `const log=[]; outer: alias: for(let i=0;i<3;i++){ try { for(let j=0;j<3;j++){ if(j===1) continue alias; log.push(i*10+j); } } finally { log.push('f'+i); } } log;`,
    `const log=[]; block: { try { break block; } finally { log.push('f'); } log.push('bad'); } log;`,
    `const log=[]; outer: for(const x of [1,2,3]) { for(const y in {a:1,b:2}) { log.push(x+y); continue outer; } } log;`,
    `const log=[]; let i=0; outer: do { i++; inner: while(true) { if(i<3) continue outer; break inner; } log.push(i); } while(i<3); log;`,
    `const log=[]; for(let i=0;i<3;i++){ switch(i){case 0: continue; case 1: log.push(i); break; default: log.push(i); } log.push('tail'); } log;`,
    `const log=[]; function f(){ try{return 1;}finally{ local: { log.push('inside'); break local; } log.push('after'); } } [f(),log];`,
    `const log=[]; function f(){ try{return 1;}finally{ try {} finally { log.push('nested'); } log.push('after'); } } [f(),log];`,
    `const log=[]; function f(){ try{return 1;}finally{ try {return 2;} finally {log.push('nested');} } } [f(),log];`,
    `const log=[]; function f(){ try { try{return 1;}finally{try {throw 2;} catch(e){log.push(e);} log.push('after');} } finally {log.push('outer');} } [f(),log];`,
    `const log=[]; outer: { try {} finally { try {break outer;} finally {log.push('inner');} log.push('bad'); } } log.push('done'); log;`,
    `const log=[]; outer: { try {try {break outer;} finally { try {} finally {log.push('inner');} log.push('middle'); }} finally {log.push('outer');}} log;`,
    `const log=[]; try { outer: { try { break outer; } finally {throw new Error('override');} } } catch(e) { log.push(e.message); } log;`,
    `const log=[]; try { try { throw 1; } finally { try { throw 2; } finally { log.push('inner'); } } } catch(e) { log.push(e); } log;`,
    `const log=[]; function f(){ try{return 1;}finally{ for(let i=0;i<3;i++){ if(i===1) continue; log.push(i); if(i===2) break; } log.push('after'); } } [f(),log];`,
    `let sum=0; loop: for(let i=0;i<3;i++){ try{continue loop;} finally{ sum+=i; if(i===1) break loop; } } sum;`,
  ];
  for (const source of cases) {
    const expected = vm.runInNewContext(source);
    const actual = await runtime(source).run();
    assert.deepEqual(JSON.parse(JSON.stringify(actual)), JSON.parse(JSON.stringify(expected)), source);
  }
  for (const source of ['x: { continue x; }', 'x: x: while(false) {}', 'x: while(false) { (()=>{break x;})(); }', 'x: function f() {}', 'break absent;']) {
    assert.throws(() => runtime(source), /label|Jump|jump|Break/i, source);
  }
  assert.equal(await runtime('label: { await Promise.resolve(1); break label; } 42;').run(), 42);
});

test('labeled pending exits survive snapshots taken inside nested cleanup', () => {
  const capabilities = {checkpoint() {}};
  const snapshotKey = Buffer.from('language-completion-test-key');
  const source = `const log=[]; outer: for(let i=0;i<3;i++){ try { if(i===1) break outer; continue outer; } finally { try { log.push(i); } finally { checkpoint(i); } log.push('tail'); } } log;`;
  let progress = runtime(source).start({capabilities, snapshotKey});
  for (let i=0;i<2;i++) {
    assert.ok(progress instanceof Progress);
    assert.deepEqual(progress.args, [i]);
    const restored = Progress.load(progress.dump(), {capabilities, snapshotKey, limits: {}});
    progress = restored.resume(undefined);
  }
  assert.deepEqual(progress, [0, 'tail', 1, 'tail']);
});

test('Promise.withResolvers and Array.fromAsync match Node values and sequencing', async () => {
  const vm = require('node:vm');
  const bodies = [
    `const gate=Promise.withResolvers(); const r=Promise.withResolvers(); r.resolve({then(resolve,reject){resolve(gate.promise);reject(2);throw 3;}}); gate.resolve(7); return await r.promise;`,
    `const gate=Promise.withResolvers(); const p=new Promise((resolve,reject)=>{resolve(gate.promise);reject(2);throw 3;}); gate.resolve(7); return await p;`,
    `const r=Promise.withResolvers(); const gate=Promise.withResolvers(); r.resolve(gate.promise); r.reject('ignored'); r.resolve(99); gate.resolve(7); return [await r.promise, Object.keys(r), r.resolve.name, r.resolve.length, Promise.withResolvers.length];`,
    `const r=Promise.withResolvers(); r.reject(new URIError('first')); r.resolve(2); try { await r.promise; } catch(e) { return [e.name,e.message]; }`,
    `const r=Promise.withResolvers(); r.resolve(r.promise); try {await r.promise;} catch(e) {return e.name;}`,
    `return await Array.fromAsync([Promise.resolve(2),3,4], async function(v,i){return this.offset+v+i;},{offset:10});`,
    `return await Array.fromAsync({0: Promise.resolve('a'),2:'c',length:3}, (v,i)=>[v,i]);`,
    `const values=[1,2]; return await Array.fromAsync(values, function(v,i){if(i===0)values.push(3);return v*2;});`,
    `const values={0:1,1:2,length:2}; return await Array.fromAsync(values, function(v,i){if(i===0){values.length=0;delete values[1];}return v;});`,
    `return [await Array.fromAsync('a🙂'),await Array.fromAsync(new Set([3,1])), await Array.fromAsync(new Map([[1,'x']]))];`,
    `return await Array.fromAsync([1,2], function(prefix,v,i){return this.offset+prefix+v+i;}.bind({offset:10},100), {offset:0});`,
    `const log=[]; const p=Array.fromAsync([1], v=>{log.push('map');return v;}).then(()=>log.push('done')); Promise.resolve().then(()=>log.push('one')).then(()=>log.push('two')).then(()=>log.push('three')); await p;return log;`,
    `const log=[]; const p=Array.fromAsync([]).then(()=>log.push('done')); Promise.resolve().then(()=>log.push('one')).then(()=>log.push('two')); await p;return log;`,
    `const log=[]; try {await Array.fromAsync([1,Promise.reject(new TypeError('bad')),3], v=>{log.push(v);return v;});}catch(e){log.push(e.name,e.message);}return log;`,
    `return await Array.fromAsync([1,2], async v=>({then(resolve){resolve(v+1);}}));`,
  ];
  for (const body of bodies) {
    const source = `(async function(){${body}})();`;
    const expected = await vm.runInNewContext(source);
    const actual = await runtime(source).run();
    assert.deepEqual(JSON.parse(JSON.stringify(actual)), JSON.parse(JSON.stringify(expected)), source);
  }
  assert.equal(await runtime('let sync=false; let p; try {p=Array.fromAsync(null);} catch(e) {sync=true;} [sync,p instanceof Promise];').run().then(JSON.stringify), '[false,true]');
  for (const source of ['Array.fromAsync(null);','Array.fromAsync(undefined);','Array.fromAsync([],1);','Promise.withResolvers.call({});']) await assert.rejects(runtime(source).run(), /TypeError/);
  await assert.rejects(runtime('Array.fromAsync({length:1e9});').run(), /heap limit/);
});

test('async array construction is sequential across authenticated host snapshots', () => {
  const capabilities = {checkpoint() {}};
  const snapshotKey = Buffer.from('language-completion-test-key');
  const source = `async function main(){ return await Array.fromAsync([1,2,3], async (v,i)=>{const next=await checkpoint(v,i);return next+1;});} main();`;
  let progress = runtime(source).start({capabilities, snapshotKey});
  for (let i=0;i<3;i++) {
    assert.ok(progress instanceof Progress);
    assert.deepEqual(progress.args,[i+1,i]);
    const restored = Progress.load(progress.dump(), {capabilities, snapshotKey, limits: {}});
    progress=restored.resume((i+1)*10);
  }
  assert.deepEqual(progress,[11,21,31]);
  const direct = runtime('Array.fromAsync([4,5], checkpoint);').start({capabilities, snapshotKey});
  assert.deepEqual(direct.args, [4,0]);
  const second = Progress.load(direct.dump(),{capabilities,snapshotKey,limits:{}}).resume(40);
  assert.deepEqual(second.args,[5,1]);
  assert.deepEqual(Progress.load(second.dump(),{capabilities,snapshotKey,limits:{}}).resume(50),[40,50]);
});

test('async drivers and extracted resolvers retain GC roots', async () => {
  const source = `async function main(){ let gate=Promise.withResolvers(); const resolve=gate.resolve; const reject=gate.reject; const input={0:gate.promise,length:1}; const p=Array.fromAsync(input, v=>({value:v})); delete input[0]; gate=null; for(let i=0;i<1000;i++){const garbage={text:'x'.repeat(200)};} resolve(42); reject(0); return await p; } main();`;
  assert.deepEqual(await runtime(source).run({limits:{heapLimitBytes:128*1024}}),[{value:42}]);
  const capabilities={checkpoint(){}}; const snapshotKey=Buffer.from('language-completion-test-key');
  const first=runtime('const r=Promise.withResolvers(); const resolve=r.resolve; checkpoint(); resolve(7); resolve(8); r.promise;').start({capabilities,snapshotKey});
  assert.equal(Progress.load(first.dump(),{capabilities,snapshotKey,limits:{}}).resume(undefined),7);
});

test('UTC Date construction, getters, setters and rendering match Node in UTC', async () => {
  const vm = require('node:vm');
  const priorTZ = process.env.TZ;
  process.env.TZ = 'UTC';
  try {
    const dates=[0,-1,951782400123,1583020799999,-62167219200000,-62198755200000,253402300800000,8640000000000000,-8640000000000000,NaN];
    const getters=['getTime','getFullYear','getMonth','getDate','getDay','getHours','getMinutes','getSeconds','getMilliseconds','getTimezoneOffset','getUTCFullYear','getUTCMonth','getUTCDate','getUTCDay','getUTCHours','getUTCMinutes','getUTCSeconds','getUTCMilliseconds','getYear'];
    const read='dates.map(t=>{const d=new Date(t);return getters.map(m=>d[m]());});';
    assert.deepEqual(await runtime(read).run({inputs:{dates,getters}}), Array.from(vm.runInNewContext(read,{dates,getters}), row=>Array.from(row)));
    const components=[[],[0],[null],[true],[false],[undefined],[2020,1,29],[2020,1,30],[2020,-1,0,25,61,61,1001],[99,11,31],[0.9,0],[-1,0,1],[275760,8,13],[-271821,3,20],[1e20,0],[2020,0,1,Infinity],[2020,0,1,0,0,0,-1]];
    const construct='components.map(parts=>[new Date(...parts).getTime(),Date.UTC(...parts)]);';
    // Zero-argument new Date uses a live clock; compare all deterministic cases.
    const noClock=components.slice(1);
    assert.deepEqual(await runtime(construct).run({inputs:{components:noClock}}), Array.from(vm.runInNewContext(construct,{components:noClock}), row=>Array.from(row)));
    const setters=['setFullYear','setMonth','setDate','setHours','setMinutes','setSeconds','setMilliseconds','setUTCFullYear','setUTCMonth','setUTCDate','setUTCHours','setUTCMinutes','setUTCSeconds','setUTCMilliseconds','setTime','setYear'];
    const args=[[],[0],[undefined],[NaN],[Infinity],[2021],[2021,13,32,25],[-1,-1,-1,-1],[1.9,null,false,'2']];
    const change='dates.flatMap(t=>setters.flatMap(name=>args.map(a=>{const d=new Date(t);const result=d[name](...a);return [result,d.getTime()];})));';
    assert.deepEqual(await runtime(change).run({inputs:{dates,setters,args}}), Array.from(vm.runInNewContext(change,{dates,setters,args}), row=>Array.from(row)));
    const strings='dates.map(t=>{const d=new Date(t);return [d.toString(),d.toDateString(),d.toTimeString(),d.toUTCString(),d.toLocaleDateString(),d.toLocaleTimeString(),d.toLocaleString(),String(d)];});';
    assert.deepEqual(await runtime(strings).run({inputs:{dates}}), Array.from(vm.runInNewContext(strings,{dates}), row=>Array.from(row)));
    const options=[{}, {year:'numeric'}, {year:'2-digit',month:'2-digit',day:'2-digit'}, {hour:'numeric',minute:'numeric',second:'numeric'}, {minute:'2-digit'}, {second:'numeric'}];
    const locales='options.map(o=>{const d=new Date(1583020799999);return [d.toLocaleDateString("en-US",o),d.toLocaleTimeString("en-US",o),d.toLocaleString("en-US",o)];});';
    assert.deepEqual(await runtime(locales).run({inputs:{options}}), Array.from(vm.runInNewContext(locales,{options}), row=>Array.from(row)));
    assert.deepEqual(await runtime('[Date.UTC(), Date.UTC(2000), Date.prototype.toGMTString === Date.prototype.toUTCString, Object.hasOwn(Date.prototype,"setFullYear"), "getDay" in new Date(0)];').run(), [NaN,946684800000,true,true,true]);
  } finally {
    if(priorTZ===undefined) delete process.env.TZ; else process.env.TZ=priorTZ;
  }
});

test('UTC Date parsing, errors, budgeting and snapshots preserve the declared profile', async () => {
  const texts=['1970','1970-01','1970-01-01','2020-02-29T12:34','2020-02-29T12:34:56','2020-02-29T12:34:56.123456','2020-02-29T24:00:00.000Z','2000-01-01T00:00:00+05:30','+010000-01-01T00:00Z','-000001-01-01T00:00:00Z','-000000-01-01T00:00Z','2020-13-01','2020-01-01T24:00:00.1Z','bad'];
  const priorTZ=process.env.TZ; process.env.TZ='UTC';
  try { assert.deepEqual(await runtime('texts.map(text=>Date.parse(text));').run({inputs:{texts}}),texts.map(Date.parse)); }
  finally { if(priorTZ===undefined)delete process.env.TZ;else process.env.TZ=priorTZ; }
  assert.deepEqual(await runtime('[-8640000000000000,-62198755200000,0,8640000000000000].map(t=>{const d=new Date(t);return [Date.parse(d.toString())===t,Date.parse(d.toUTCString())===t];});').run(),[[true,true],[true,true],[true,true],[true,true]]);
  assert.equal(await runtime('typeof Date(1,2,3);').run(),'string');
  for(const source of ['Date.prototype.getDay();','Date.prototype.setTime.call({},1);','new Date(0).setMonth(1n);','Date.UTC(1n);','new Date(0).toLocaleString("fr-FR");','new Date(0).toLocaleString("en-US",{timeZone:"America/Los_Angeles"});','new Date(0).toLocaleString("en-US",null);']) await assert.rejects(runtime(source).run(),/TypeError/);
  assert.equal(await runtime('new Date(NaN).toLocaleDateString("unsupported");').run(),'Invalid Date');
  await assert.rejects(runtime('Date.parse(text);').run({inputs:{text:'0'.repeat(10000)},limits:{instructionBudget:100}}),/instruction budget/);
  const capabilities={checkpoint(){}};const snapshotKey=Buffer.from('language-completion-test-key');
  const first=runtime('const d=new Date(2000,1,29); const setter=d.setFullYear; checkpoint(); setter.call(d,2001); [d.toISOString(),d.getTimezoneOffset()];').start({capabilities,snapshotKey});
  assert.deepEqual(Progress.load(first.dump(),{capabilities,snapshotKey,limits:{}}).resume(undefined),['2001-03-01T00:00:00.000Z',0]);
});

test('localeCompare matches en-US collation across accents, case, digits and scripts', async () => {
  const words = ['', 'a', 'A', 'á', 'ä', 'a\u0301', 'b', 'Z', '2', '10', '002', 'file9', 'file10',
    'ab', 'a-b', 'a b', 'a_b', '$', '€', 'ß', 'ss', 'œ', 'oe', '中', '😀', '\u0345\u0301'];
  const profiles = [{}, {numeric:true}, ...['base','accent','case','variant'].map(sensitivity=>({sensitivity})),
    {caseFirst:'upper'}, {caseFirst:'lower'}, {ignorePunctuation:true},
    {numeric:true, sensitivity:'base', ignorePunctuation:true, caseFirst:'upper'}];
  for (const options of profiles) {
    const expected = words.map(a => words.map(b => Math.sign(a.localeCompare(b, 'en-US', options))));
    assert.deepEqual(await runtime(`words.map(a => words.map(b => a.localeCompare(b, 'en-US', options)));`).run({inputs:{words, options}}), expected, JSON.stringify(options));
  }
  const source = `JSON.stringify([
    ['z', 'ä', 'B', 'a'].toSorted((a,b) => a.localeCompare(b)),
    String.prototype.localeCompare.call(42, '9'), new String('é').localeCompare('e', 'EN-us', {sensitivity:'base'}),
    ''.localeCompare(), 'x'.localeCompare('y', []), 'x'.localeCompare('y', [,'en-US']),
    ''.localeCompare.name, ''.localeCompare.length, String.prototype.hasOwnProperty('localeCompare')
  ]);`;
  assert.equal(await runtime(source).run(), require('node:vm').runInNewContext(source));
});

test('collation rejects unsupported profiles, bounds work/space and survives snapshots', async () => {
  for (const source of [
    `'a'.localeCompare('b', ['en-US','fr-FR']);`,
    `const x = []; x.push(x); 'a'.localeCompare('b', x);`,
    `'a'.localeCompare('b', undefined, null);`,
    `'a'.localeCompare('b', undefined, {usage:'search'});`,
    `'a'.localeCompare('b', undefined, {numeric:1});`,
    `'a'.localeCompare('b', undefined, {caseFirst:'bad'});`,
    `'a'.localeCompare('b', undefined, {sensitivity:'bad'});`,
    `'a'.localeCompare('b', undefined, {collation:'emoji'});`,
    `'a'.localeCompare('b', undefined, {unknown:true});`,
    `String.prototype.localeCompare.call(undefined, 'x');`,
  ]) await assert.rejects(runtime(source).run(), /TypeError|RangeError/, source);
  await assert.rejects(runtime(`text.localeCompare('a');`).run({inputs:{text:'a'.repeat(10000)},limits:{instructionBudget:1000}}), /instruction budget/);
  await assert.rejects(runtime(`text.localeCompare(text);`).run({inputs:{text:'a'+'\u0301'.repeat(1000)},limits:{instructionBudget:100000,heapLimitBytes:1024*1024}}), /instruction budget/);
  await assert.rejects(runtime(`text.localeCompare('a');`).run({inputs:{text:'a'.repeat(10000)},limits:{heapLimitBytes:128*1024}}), /heap limit/);
  const capabilities = {checkpoint() {}};
  const snapshotKey = Buffer.from('language-completion-collation');
  const first = runtime(`const compare = String.prototype.localeCompare; checkpoint(); compare.call('2','10','en-US',{numeric:true});`).start({capabilities,snapshotKey});
  assert.equal(Progress.load(first.dump(), {capabilities,snapshotKey,limits:{}}).resume(undefined), -1);
});

test('number locale formatting matches decimal half-expand rounding and currency defaults', async () => {
  const numbers = [NaN, Infinity, -Infinity, 0, -0, 1, -1, 1.005, -1.005, 2.675, 9.995, 99.95,
    .00005, -.00005, 0.1 + 0.2, 1e-7, 5e-324, 1e20, 1e21, 1000000000000000128,
    Number.MAX_VALUE, -Number.MAX_VALUE, 1234.56789, 0.9999999999999999];
  const profiles = [{}, {useGrouping:false}, {minimumFractionDigits:5}, {maximumFractionDigits:0},
    {maximumFractionDigits:2}, {minimumFractionDigits:20}, {maximumFractionDigits:100},
    {minimumFractionDigits:100}, {style:'percent'}, {style:'percent',maximumFractionDigits:2},
    {style:'percent',minimumFractionDigits:5}, {style:'currency',currency:'usd'},
    {style:'currency',currency:'EUR',minimumFractionDigits:4},
    {style:'currency',currency:'BHD',maximumFractionDigits:1},
    {style:'currency',currency:'JPY',minimumFractionDigits:2}, {currency:'EUR'}];
  for (const options of profiles) {
    assert.deepEqual(await runtime(`numbers.map(n=>n.toLocaleString('en-US', options));`).run({inputs:{numbers,options}}), numbers.map(n=>n.toLocaleString('en-US',options)), JSON.stringify(options));
  }
  // All non-default CLDR minor units and all non-code en-US symbols, plus fallback.
  const currencies = ['USD','EUR','GBP','CAD','AUD','CNY','BRL','HKD','ILS','INR','MXN','NZD','PHP','TWD',
    'XCD','XCG','XXX','XYZ','ADP','AFN','ALL','BHD','BIF','BYR','CLF','CLP','DJF','ESP','GNF','IQD','IRR',
    'ISK','ITL','JOD','JPY','KMF','KPW','KRW','KWD','LAK','LBP','LUF','LYD','MGA','MGF','MMK','MRO','OMR',
    'PYG','RSD','RWF','SLL','SOS','STD','SYP','TMM','TND','TRL','UGX','UYI','UYW','VND','VUV','XAF','XOF',
    'XPF','YER','ZMK','ZWD','COP','HUF','IDR','PKR'];
  for (const currency of currencies) {
    const formatter = new Intl.NumberFormat('en-US',{style:'currency',currency});
    const result = await runtime(`const f = new Intl.NumberFormat('en-US',{style:'currency',currency});
      [f.resolvedOptions().minimumFractionDigits, f.resolvedOptions().maximumFractionDigits, numbers.map(n=>f.format(n))];`).run({inputs:{currency,numbers}});
    assert.deepEqual(result, [formatter.resolvedOptions().minimumFractionDigits, formatter.resolvedOptions().maximumFractionDigits, numbers.map(n=>formatter.format(n))],currency);
  }
  const source = `JSON.stringify([new Number(42).toLocaleString(), Number.prototype.toLocaleString(),
    Number.prototype.toLocaleString.call(-0), (1).toLocaleString.name, (1).toLocaleString.length,
    Number.prototype.hasOwnProperty('toLocaleString'), 'toLocaleString' in new Number(0),
    (1).toLocaleString('en-US', {minimumFractionDigits:2.9}),
    (1).toLocaleString('en-US', {maximumFractionDigits:'4'})]);`;
  assert.equal(await runtime(source).run(),require('node:vm').runInNewContext(source));
});

test('number formatting rejects unsupported profiles, budgets coercion and restores snapshots', async () => {
  for (const source of [
    `Number.prototype.toLocaleString.call('42');`, `Number.prototype.toLocaleString.call(42n);`,
    `(1).toLocaleString('fr-FR');`, `(1).toLocaleString('en-US', null);`,
    `(1).toLocaleString('en-US', {style:'currency'});`, `(1).toLocaleString('en-US', {currency:'€UR'});`,
    `(1).toLocaleString('en-US', {maximumFractionDigits:NaN});`,
    `(1).toLocaleString('en-US', {maximumFractionDigits:101});`,
    `(1).toLocaleString('en-US', {minimumFractionDigits:-1});`,
    `(1).toLocaleString('en-US', {minimumFractionDigits:3,maximumFractionDigits:2});`,
    `(1).toLocaleString('en-US', {currencyDisplay:'name'});`,
    `Intl.NumberFormat().format(1n);`,
  ]) await assert.rejects(runtime(source).run(),/TypeError|RangeError/,source);
  await assert.rejects(runtime(`Intl.NumberFormat().format(text);`).run({inputs:{text:'0'.repeat(10000)},limits:{instructionBudget:1000}}), /instruction budget/);
  const capabilities = {checkpoint() {}};
  const snapshotKey = Buffer.from('language-completion-number-format');
  const first = runtime(`const f=Intl.NumberFormat('en-US',{style:'currency',currency:'CLF'});
    const locale=Number.prototype.toLocaleString; checkpoint();
    [f.format(1.23456), locale.call(-1.005,'en-US',{style:'currency',currency:'EUR'})];`).start({capabilities,snapshotKey});
  assert.deepEqual(Progress.load(first.dump(),{capabilities,snapshotKey,limits:{}}).resume(undefined), ['CLF 1.2346','-€1.01']);
});
