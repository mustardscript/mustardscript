'use strict';

const { assert, runtime, test } = require('./support/helpers.js');

test('run supports default parameters and default destructuring in parameter scope', async () => {
  const result = await runtime(`
    function wrap(value = 1, next = value + 1, { label = "ok", total = next + 1 } = {}) {
      const shadow = 99;
      return [value, next, label, total, shadow];
    }
    wrap();
  `).run();

  assert.deepEqual(result, [1, 2, 'ok', 3, 99]);
});

test('run supports default destructuring in declarations and catch bindings', async () => {
  const result = await runtime(`
    const { value = 2, nested: { total = value + 1 } = {} } = {};
    let captured;
    try {
      throw {};
    } catch ({ message = "missing" }) {
      captured = message;
    }
    [value, total, captured];
  `).run();

  assert.deepEqual(result, [2, 3, 'missing']);
});

test('run supports destructuring assignment for identifiers, members, and loop assignment headers', async () => {
  const result = await runtime(`
    let left = 0;
    const boxRef = { current: 0, seen: [] };
    [left, boxRef.current] = [2, 3];
    ({ left = 5, current: boxRef.current = 7 } = { current: 9 });
    for ([left, boxRef.current] of [[10, 11], [12, 13]]) {
      boxRef.seen[boxRef.seen.length] = left + boxRef.current;
    }
    for ({ alpha: boxRef.current } in [{ alpha: "ignored" }]) {
      boxRef.current = 14;
    }
    [left, boxRef.current, boxRef.seen];
  `).run();

  assert.deepEqual(result, [12, 14, [21, 25]]);
});

test('run supports prefix and postfix update expressions for identifiers and members', async () => {
  const result = await runtime(`
    let value = 1;
    let big = 1n;
    const record = { count: 4 };
    const access = { object: 0, key: 0 };
    function object() {
      access.object += 1;
      return record;
    }
    function key() {
      access.key += 1;
      return "count";
    }
    ({
      postfixValue: value++,
      afterPostfixValue: value,
      prefixValue: ++value,
      postfixMember: object()[key()]--,
      afterPostfixMember: record.count,
      prefixMember: ++object()[key()],
      finalMember: record.count,
      bigintTrace: [String(big++), String(big), String(++big), String(big)],
      access,
    });
  `).run();

  assert.deepEqual(result, {
    postfixValue: 1,
    afterPostfixValue: 2,
    prefixValue: 3,
    postfixMember: 4,
    afterPostfixMember: 3,
    prefixMember: 4,
    finalMember: 4,
    bigintTrace: ['1', '2', '3', '3'],
    access: { object: 2, key: 2 },
  });
});

test('run supports remainder and exponent compound assignment operators', async () => {
  const result = await runtime(`
    let left = 10;
    let right = 2;
    const boxRef = { value: 9 };
    left %= 4;
    right **= 3;
    boxRef.value %= 4;
    ({
      left,
      right,
      box: boxRef.value,
    });
  `).run();

  assert.deepEqual(result, {
    left: 2,
    right: 8,
    box: 1,
  });
});

test('run supports conservative instanceof checks for supported constructors', async () => {
  const result = await runtime(`
    function Box() {}
    ({
      array: [] instanceof Array,
      arrayObject: [] instanceof Object,
      map: new Map() instanceof Map,
      mapObject: new Map() instanceof Object,
      set: new Set() instanceof Set,
      promise: Promise.resolve(1) instanceof Promise,
      date: new Date(0) instanceof Date,
      regexp: /a/ instanceof RegExp,
      typeError: new TypeError("boom") instanceof TypeError,
      error: new TypeError("boom") instanceof Error,
      guestFunction: ({}) instanceof Box,
    });
  `).run();

  assert.deepEqual(result, {
    array: true,
    arrayObject: true,
    map: true,
    mapObject: true,
    set: true,
    promise: true,
    date: true,
    regexp: true,
    typeError: true,
    error: true,
    guestFunction: false,
  });
});
