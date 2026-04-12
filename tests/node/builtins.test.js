'use strict';

const { assert, isJsliteError, runtime, test } = require('./support/helpers.js');

test('run executes sync programs', async () => {
  const result = await runtime(`
    const values = [1, 2, 3];
    values[0] + values[2];
  `).run();

  assert.equal(result, 4);
});

test('run supports conservative array, string, object, and Math helper surface', async () => {
  const result = await runtime(`
    const values = [1, 2];
    values.push(3);
    const object = { zebra: 1, alpha: 2 };
    const arrayEntries = Object.entries(values);
    const assignedObjectTarget = { alpha: 1 };
    const assignedObject = Object.assign(
      assignedObjectTarget,
      { zebra: 2 },
      undefined,
      { beta: 3 },
    );
    const assignedArrayTarget = [4];
    assignedArrayTarget.label = "seed";
    const assignedArray = Object.assign(
      assignedArrayTarget,
      { 1: 5 },
      [6],
      null,
      { extra: 7 },
    );
    const csv = "alpha,beta,gamma";
    ({
      slice: values.slice(1).join('-'),
      includes: values.includes(2),
      indexOf: values.indexOf(3),
      trim: "  MiXeD Example  ".trim(),
      startsWith: "  MiXeD Example  ".startsWith("Mi", 2),
      substring: "  MiXeD Example  ".substring(8, 3),
      split: csv.split(",", 2),
      replace: "  MiXeD Example  ".replace("MiXeD", "Mixed"),
      replaceAll: "a-b-a".replaceAll("a", "z"),
      search: "  MiXeD Example  ".search("Example"),
      match: "  MiXeD Example  ".match("Example"),
      objectKeys: Object.keys(object),
      objectValues: Object.values(object),
      hasOwn: Object.hasOwn(object, "alpha"),
      assignedObjectIdentity: assignedObject === assignedObjectTarget,
      assignedObjectEntries: Object.entries(assignedObject),
      assignedArrayIdentity: assignedArray === assignedArrayTarget,
      assignedArrayEntries: Object.entries(assignedArray),
      arrayEntries: arrayEntries,
      pow: Math.pow(2, 5),
      sqrt: Math.sqrt(81),
      trunc: Math.trunc(-3.9),
      sign: Math.sign(-0),
    });
  `).run();

  assert.deepEqual(result, {
    slice: '2-3',
    includes: true,
    indexOf: 2,
    trim: 'MiXeD Example',
    startsWith: true,
    substring: 'iXeD ',
    split: ['alpha', 'beta'],
    replace: '  Mixed Example  ',
    replaceAll: 'z-b-z',
    search: 8,
    match: ['Example'],
    objectKeys: ['zebra', 'alpha'],
    objectValues: [1, 2],
    hasOwn: true,
    assignedObjectIdentity: true,
    assignedObjectEntries: [['alpha', 1], ['zebra', 2], ['beta', 3]],
    assignedArrayIdentity: true,
    assignedArrayEntries: [['0', 6], ['1', 5], ['label', 'seed'], ['extra', 7]],
    arrayEntries: [['0', 1], ['1', 2], ['2', 3]],
    pow: 32,
    sqrt: 9,
    trunc: -3,
    sign: -0,
  });
  assert.ok(Object.is(result.sign, -0));
});

test('JSON.stringify matches Node ordering and omission semantics for supported values', async () => {
  const result = await runtime(`
    const object = {};
    object.beta = 2;
    object[10] = 10;
    object.alpha = 1;
    object[2] = 3;
    object["01"] = 4;
    const values = [1, undefined, () => 3, (0 / 0), -0, (1 / 0)];
    ({
      objectKeys: Object.keys(object),
      arrayStringified: JSON.stringify(values),
      objectStringified: JSON.stringify(object),
      wrapperStringified: JSON.stringify({
        keep: 1,
        skipUndefined: undefined,
        skipFunction: () => 1,
        nested: object,
      }),
      topLevelUndefined: JSON.stringify(undefined),
    });
  `).run();

  assert.deepEqual(result, {
    objectKeys: ['2', '10', 'beta', 'alpha', '01'],
    arrayStringified: '[1,null,null,null,0,null]',
    objectStringified: '{"2":3,"10":10,"beta":2,"alpha":1,"01":4}',
    wrapperStringified: '{"keep":1,"nested":{"2":3,"10":10,"beta":2,"alpha":1,"01":4}}',
    topLevelUndefined: undefined,
  });
});

test('run supports callback-heavy array helpers and thisArg for supported callbacks', async () => {
  const result = await runtime(`
    const values = [1, 2, 3];
    let seen = 0;
    const mapped = values.map(function (value, index) {
      seen += this.step;
      return value + index + this.offset;
    }, { step: 10, offset: 4 });
    const filtered = values.filter((value) => value % 2 === 1);
    const found = values.find((value) => value > 2);
    const foundIndex = values.findIndex((value) => value > 2);
    const some = values.some((value) => value === 2);
    const every = values.every((value) => value > 0);
    const reduced = values.reduce((acc, value) => acc + value, 5);
    values.forEach((value) => {
      seen += value;
    });
    [mapped, filtered, found, foundIndex, some, every, reduced, seen];
  `).run();

  assert.deepEqual(result, [[5, 7, 9], [1, 3], 3, 2, true, true, 11, 36]);
});

test('run supports Array.of, Array.prototype.concat, Array.prototype.at, Math.log, and Math.random', async () => {
  const result = await runtime(`
    const single = Array.of(7);
    const merged = Array.of(1, 2, 3).concat([4, 5], 6);
    const random = Math.random();
    ({
      singleLength: single.length,
      singleValue: single[0],
      merged: merged,
      atFront: merged.at(0),
      atFromEnd: merged.at(-2),
      atMissing: merged.at(99),
      logOne: Math.log(1),
      logBase2: Math.round(Math.log(8) / Math.log(2)),
      randomIsNumber: typeof random === "number",
      randomInRange: random >= 0 && random < 1,
      randomIsFinite: random === random && random !== (1 / 0) && random !== (-1 / 0),
    });
  `).run();

  assert.deepEqual(result, {
    singleLength: 1,
    singleValue: 7,
    merged: [1, 2, 3, 4, 5, 6],
    atFront: 1,
    atFromEnd: 5,
    atMissing: undefined,
    logOne: 0,
    logBase2: 3,
    randomIsNumber: true,
    randomInRange: true,
    randomIsFinite: true,
  });
});

test('run supports Array.prototype.splice, Array.prototype.flat, and Array.prototype.flatMap', async () => {
  const result = await runtime(`
    const values = [1, 2, 3, 4];
    values.label = "seed";
    const removed = values.splice(-3, 2, 9, 10, 11);
    const untouched = [7, 8];
    const untouchedRemoved = untouched.splice();
    ({
      valuesEntries: Object.entries(values),
      removed,
      untouched,
      untouchedRemoved,
      shallow: [1, [2, [3]], 4].flat(undefined),
      deep: [1, [2, [3, [4]]], 5].flat(2),
      flatMapped: [1, 2, 3].flatMap(function (value, index) {
        return [value + this.offset, [index]];
      }, { offset: 4 }),
    });
  `).run();

  assert.deepEqual(result, {
    valuesEntries: [['0', 1], ['1', 9], ['2', 10], ['3', 11], ['4', 4], ['label', 'seed']],
    removed: [2, 3],
    untouched: [7, 8],
    untouchedRemoved: [],
    shallow: [1, 2, [3], 4],
    deep: [1, 2, 3, [4], 5],
    flatMapped: [5, [0], 6, [1], 7, [2]],
  });
});

test('run preserves sparse array holes across helpers, enumeration, and JSON', async () => {
  const result = await runtime(`
    const values = [1, , undefined, 4];
    const callbackIndexes = [];
    const findVisits = [];
    const findIndexVisits = [];
    values.forEach((value, index) => {
      callbackIndexes[callbackIndexes.length] = index;
    });
    const foundHole = values.find((value, index) => {
      findVisits[findVisits.length] = [index, value, index in values];
      return index === 1;
    });
    const foundHoleIndex = values.findIndex((value, index) => {
      findIndexVisits[findIndexVisits.length] = [index, value, index in values];
      return value === undefined && index === 1;
    });
    const sliced = values.slice(0, 4);
    const mapped = values.map((value, index) => value ?? (index + 10));
    const merged = values.concat([, 5]);
    ({
      length: values.length,
      holeIsUndefined: values[1] === undefined,
      hasHole: 1 in values,
      hasUndefined: 2 in values,
      keys: Object.keys(values),
      entries: Object.entries(values),
      iterated: Array.from(values.values()),
      includesUndefined: values.includes(undefined),
      indexOfUndefined: values.indexOf(undefined),
      joined: values.join("-"),
      json: JSON.stringify(values),
      callbackIndexes,
      foundHole,
      foundHoleIndex,
      findVisits,
      findIndexVisits,
      slicedKeys: Object.keys(sliced),
      mappedKeys: Object.keys(mapped),
      mappedHasHole: 1 in mapped,
      mergedKeys: Object.keys(merged),
      mergedTail: merged[5],
    });
  `).run();

  assert.deepEqual(result, {
    length: 4,
    holeIsUndefined: true,
    hasHole: false,
    hasUndefined: true,
    keys: ['0', '2', '3'],
    entries: [['0', 1], ['2', undefined], ['3', 4]],
    iterated: [1, undefined, undefined, 4],
    includesUndefined: true,
    indexOfUndefined: 2,
    joined: '1---4',
    json: '[1,null,null,4]',
    callbackIndexes: [0, 2, 3],
    foundHole: undefined,
    foundHoleIndex: 1,
    findVisits: [
      [0, 1, true],
      [1, undefined, false],
    ],
    findIndexVisits: [
      [0, 1, true],
      [1, undefined, false],
    ],
    slicedKeys: ['0', '2', '3'],
    mappedKeys: ['0', '2', '3'],
    mappedHasHole: false,
    mergedKeys: ['0', '2', '3', '5'],
    mergedTail: 5,
  });
});

test('run supports computed object literal keys, method shorthand, and object spread for plain objects and arrays', async () => {
  const result = await runtime(`
    const key = "value";
    const extra = [3];
    extra.label = "ok";
    const record = {
      alpha: 1,
      [key]: 2,
      total(step) {
        return this.alpha + this[key] + step;
      },
      ...null,
      ...undefined,
      ...extra,
      ...{ beta: 4 },
    };
    ({
      value: record.value,
      zero: record[0],
      label: record.label,
      total: record.total(5),
      methodType: typeof record.total,
      keys: Object.keys(record),
    });
  `).run();

  assert.deepEqual(result, {
    value: 2,
    zero: 3,
    label: 'ok',
    total: 8,
    methodType: 'function',
    keys: ['0', 'alpha', 'value', 'total', 'label', 'beta'],
  });
});

test('array callback helpers fail closed for invalid callbacks and synchronous host suspensions', async () => {
  await assert.rejects(
    () => runtime('([1]).map(1);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.map expects a callable callback',
    }),
  );

  await assert.rejects(
    () =>
      runtime('[1].map(fetch_data);').run({
        capabilities: {
          fetch_data(value) {
            return value + 1;
          },
        },
      }),
    isJsliteError({
      kind: 'Runtime',
      message: 'array callback helpers do not support synchronous host suspensions',
    }),
  );

  await assert.rejects(
    () => runtime('[].reduce((acc, value) => acc + value);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.reduce requires an initial value for empty arrays',
    }),
  );
});

test('Array.prototype.splice, Array.prototype.flat, and Array.prototype.flatMap fail closed for incompatible receivers and invalid callbacks', async () => {
  await assert.rejects(
    () => runtime('const splice = [1].splice; splice(0, 1);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.splice called on incompatible receiver',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('const flat = [1].flat; flat();').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.flat called on incompatible receiver',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('const flatMap = [1].flatMap; flatMap((value) => [value]);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.flatMap called on incompatible receiver',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('([1]).flatMap(1);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.flatMap expects a callable callback',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () =>
      runtime('[1].flatMap(fetch_data);').run({
        capabilities: {
          fetch_data(value) {
            return [value];
          },
        },
      }),
    isJsliteError({
      kind: 'Runtime',
      message: 'array callback helpers do not support synchronous host suspensions',
      guestSafe: true,
    }),
  );
});

test('Array.prototype.concat and Array.prototype.at fail closed for incompatible receivers', async () => {
  await assert.rejects(
    () => runtime('const concat = [1].concat; concat([2]);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.concat called on incompatible receiver',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('const at = [1].at; at(0);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.at called on incompatible receiver',
      guestSafe: true,
    }),
  );
});

test('Object.assign copies supported enumerable properties and unsupported object helpers fail closed', async () => {
  await assert.rejects(
    () => runtime('Object.assign(1, { alpha: 1 });').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Object helpers currently only support plain objects and arrays',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('Object.create(null);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Object.create is unsupported because prototype semantics are deferred',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('Object.freeze({ alpha: 1 });').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Object.freeze is unsupported because property descriptor semantics are deferred',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('Object.seal({ alpha: 1 });').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Object.seal is unsupported because property descriptor semantics are deferred',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('({ ...1 });').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'object spread currently only supports plain objects and arrays',
      guestSafe: true,
    }),
  );
});

test('run supports RegExp helpers, regex string patterns, and callback replacements', async () => {
  const result = await runtime(`
    const exec = /(?<letters>[a-z]+)(\\d+)/g;
    const first = exec.exec("ab12cd34");
    const firstLast = exec.lastIndex;
    const second = exec.exec("ab12cd34");
    const secondLast = exec.lastIndex;
    const third = exec.exec("ab12cd34");
    const thirdLast = exec.lastIndex;
    const sticky = /a/y;
    sticky.lastIndex = 1;
    const stickyFirst = sticky.exec("ba");
    const stickyFirstLast = sticky.lastIndex;
    const stickySecond = sticky.exec("ba");
    const stickySecondLast = sticky.lastIndex;
    const matched = "abc123".match(/(?<letters>[a-z]+)(\\d+)/);
    ({
      split: "a1b2".split(/(\\d)/),
      replaceLiteralCallback: "abc".replace("a", (match, offset, input) => \`\${match}:\${offset}:\${input}\`),
      replaceRegexTemplate: "abc123".replace(/(?<letters>[a-z]+)(\\d+)/, "$<letters>-$2"),
      replaceAllRegexCallback: "alpha-1 beta-2".replaceAll(
        /([a-z]+)-(\\d)/g,
        (match, word, digit, offset, input) => \`\${word.toUpperCase()}:\${digit}:\${offset}:\${input.length}\`,
      ),
      search: "abc123".search(/\\d+/),
      matchSingle: [matched[0], matched[1], matched[2], matched.index, matched.input, matched.groups.letters],
      matchGlobal: "ab12cd34".match(/\\d+/g),
      firstExec: [first[0], first[1], first[2], first.index, first.input, first.groups.letters, firstLast],
      secondExec: [second[0], second.index, secondLast],
      thirdExec: [third === null, thirdLast],
      testState: (() => {
        const regex = /a/g;
        return [regex.test("ba"), regex.lastIndex, regex.test("ba"), regex.lastIndex];
      })(),
      stickyState: [stickyFirst[0], stickyFirst.index, stickyFirstLast, stickySecond === null, stickySecondLast],
      ctor: [RegExp("a", "gi").flags, new RegExp(/b/g).source, new RegExp(/b/g).flags],
    });
  `).run();

  assert.deepEqual(result, {
    split: ['a', '1', 'b', '2', ''],
    replaceLiteralCallback: 'a:0:abcbc',
    replaceRegexTemplate: 'abc-123',
    replaceAllRegexCallback: 'ALPHA:1:0:14 BETA:2:8:14',
    search: 3,
    matchSingle: ['abc123', 'abc', '123', 0, 'abc123', 'abc'],
    matchGlobal: ['12', '34'],
    firstExec: ['ab12', 'ab', '12', 0, 'ab12cd34', 'ab', 4],
    secondExec: ['cd34', 4, 8],
    thirdExec: [true, 0],
    testState: [true, 2, false, 0],
    stickyState: ['a', 1, 2, true, 0],
    ctor: ['gi', 'b', 'g'],
  });
});

test('RegExp helpers fail closed for unsupported flags, non-global replaceAll, and sync host replacements', async () => {
  await assert.rejects(
    () => runtime('new RegExp("a", "dg");').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'unsupported regular expression flag `d`',
    }),
  );

  await assert.rejects(
    () => runtime('"abc".replaceAll(/a/, "z");').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'String.prototype.replaceAll requires a global RegExp',
    }),
  );

  await assert.rejects(
    () =>
      runtime('"abc".replace("a", fetch_data);').run({
        capabilities: {
          fetch_data(value) {
            return value;
          },
        },
      }),
    isJsliteError({
      kind: 'Runtime',
      message: 'String.prototype.replace callback replacements do not support host suspensions',
    }),
  );
});

test('run applies nullish assignment only to nullish identifiers and members', async () => {
  const result = await runtime(`
    let missing;
    missing ??= 7;
    const box = { present: 5, absent: undefined };
    box.present ??= 9;
    box.absent ??= 11;
    const key = 'dynamic';
    box[key] ??= 13;
    [missing, box.present, box.absent, box.dynamic];
  `).run();

  assert.deepEqual(result, [7, 5, 11, 13]);
});

test('run binds member-call receivers for guest functions', async () => {
  const result = await runtime(`
    const method = function (delta) {
      return this.base + delta;
    };
    const obj = { base: 40, method: method };
    obj.method(2);
  `).run();

  assert.equal(result, 42);
});

test('run aligns globalThis, top-level this, and arrow lexical this with the real global object', async () => {
  const result = await runtime(`
    globalThis.answer = 7;
    function declared() {
      return 'ok';
    }
    const box = {
      value: 3,
      method() {
        return (() => this.value)();
      },
    };
    ({
      globalSelf: globalThis.globalThis === globalThis,
      objectCtor: globalThis.Object === Object,
      hasObject: "Object" in globalThis,
      lookup: answer,
      declaredOnGlobal: globalThis.declared === declared,
      topLevelThis: this === globalThis,
      arrowThis: box.method(),
    });
  `).run();

  assert.deepEqual(result, {
    globalSelf: true,
    objectCtor: true,
    hasObject: true,
    lookup: 7,
    declaredOnGlobal: true,
    topLevelThis: true,
    arrowThis: 3,
  });
});

test('run installs root function declarations on the real global object even when globalThis is shadowed', async () => {
  const result = await runtime(`
    const globalThis = { note: 1 };
    function declared() {}
    ({
      declaredType: typeof declared,
      localNote: globalThis.note,
      globalBinding: this.declared === declared,
    });
  `).run();

  assert.deepEqual(result, {
    declaredType: 'function',
    localNote: 1,
    globalBinding: true,
  });
});

test('run rejects duplicate lexical bindings during validation', async () => {
  assert.throws(
    () => runtime(`
      let value = 1;
      let value = 2;
      value;
    `),
    isJsliteError({
      kind: 'Validation',
      message: 'already been declared',
    }),
  );
});

test('run exposes callable metadata and constructor links for supported callables', async () => {
  const result = await runtime(`
    function declared(alpha, beta) {}
    declared.extra = 5;
    const arrow = (left, right) => left + right;
    const method = [].map;
    ({
      declared: {
        name: declared.name,
        length: declared.length,
        prototypeType: typeof declared.prototype,
        extra: declared.extra,
        keys: Object.keys(declared),
        instanceofObject: declared instanceof Object,
      },
      arrow: {
        name: arrow.name,
        length: arrow.length,
        prototypeType: typeof arrow.prototype,
        instanceofObject: arrow instanceof Object,
      },
      builtins: {
        arrayName: Array.name,
        arrayLength: Array.length,
        arrayPrototypeType: typeof Array.prototype,
        arrayObject: Array instanceof Object,
        arrayOwnLength: Object.hasOwn(Array, "length"),
        methodName: method.name,
        methodLength: method.length,
        methodObject: method instanceof Object,
      },
      constructors: {
        array: [].constructor === Array,
        object: ({}).constructor === Object,
        promise: Promise.resolve(1).constructor === Promise,
        date: new Date(0).constructor === Date,
        regexp: /a/.constructor === RegExp,
      },
    });
  `).run();

  assert.deepEqual(result, {
    declared: {
      name: 'declared',
      length: 2,
      prototypeType: 'object',
      extra: 5,
      keys: ['extra'],
      instanceofObject: true,
    },
    arrow: {
      name: 'arrow',
      length: 2,
      prototypeType: 'undefined',
      instanceofObject: true,
    },
    builtins: {
      arrayName: 'Array',
      arrayLength: 1,
      arrayPrototypeType: 'object',
      arrayObject: true,
      arrayOwnLength: true,
      methodName: 'map',
      methodLength: 1,
      methodObject: true,
    },
    constructors: {
      array: true,
      object: true,
      promise: true,
      date: true,
      regexp: true,
    },
  });
});

test('run exposes callable helper methods and boxed string wrapper methods on the supported surface', async () => {
  const result = await runtime(`
    function sum(left, right) {
      return this.base + left + right;
    }
    const bound = sum.bind({ base: 10 }, 1);
    ({
      helperTypes: [typeof sum.call, typeof sum.apply, typeof sum.bind],
      viaCall: sum.call({ base: 4 }, 5, 6),
      viaApply: sum.apply({ base: 7 }, [8, 9]),
      boundType: typeof bound,
      boundResult: bound(2),
      boxedTrim: Object("  MiXeD  ").trim(),
      boxedTrimViaCall: "".trim.call(Object("  spaced  ")),
    });
  `).run();

  assert.deepEqual(result, {
    helperTypes: ['function', 'function', 'function'],
    viaCall: 15,
    viaApply: 24,
    boundType: 'function',
    boundResult: 13,
    boxedTrim: 'MiXeD',
    boxedTrimViaCall: 'spaced',
  });
});

test('run matches supported Array, Object, and primitive-wrapper constructor semantics', async () => {
  const result = await runtime(`
    const array = Array(3);
    const built = new Array(3);
    const existing = [1, 2];
    const map = new Map([[1, 2]]);
    const boxedString = Object("ab");
    const boxedNumber = new Number(1);
    const boxedText = new String("ab");
    const boxedBoolean = new Boolean(false);
    ({
      arrayLength: array.length,
      arrayKeys: Object.keys(array),
      builtLength: built.length,
      builtJson: JSON.stringify(built),
      invalidLength: (() => {
        try {
          new Array(2.5);
          return null;
        } catch (error) {
          return [error.name, error.message];
        }
      })(),
      sameArray: Object(existing) === existing,
      sameMap: Object(map) === map,
      boxedString: {
        object: boxedString instanceof Object,
        string: boxedString instanceof String,
        length: boxedString.length,
        first: boxedString[0],
        value: String(boxedString),
      },
      boxedNumber: {
        type: typeof boxedNumber,
        object: boxedNumber instanceof Object,
        number: boxedNumber instanceof Number,
        value: String(boxedNumber),
      },
      boxedText: {
        type: typeof boxedText,
        object: boxedText instanceof Object,
        string: boxedText instanceof String,
        value: String(boxedText),
      },
      boxedBoolean: {
        type: typeof boxedBoolean,
        object: boxedBoolean instanceof Object,
        boolean: boxedBoolean instanceof Boolean,
        truthy: !!boxedBoolean,
      },
    });
  `).run();

  assert.deepEqual(result, {
    arrayLength: 3,
    arrayKeys: [],
    builtLength: 3,
    builtJson: '[null,null,null]',
    invalidLength: ['RangeError', 'Invalid array length'],
    sameArray: true,
    sameMap: true,
    boxedString: {
      object: true,
      string: true,
      length: 2,
      first: 'a',
      value: 'ab',
    },
    boxedNumber: {
      type: 'object',
      object: true,
      number: true,
      value: '1',
    },
    boxedText: {
      type: 'object',
      object: true,
      string: true,
      value: 'ab',
    },
    boxedBoolean: {
      type: 'object',
      object: true,
      boolean: true,
      truthy: true,
    },
  });
});

test('run keeps Array.prototype.reduce callback this undefined when an initial accumulator is present', async () => {
  const result = await runtime(`
    const seed = { tag: "seed" };
    [1].reduce(function (acc, value) {
      return {
        same: this === acc,
        thisType: typeof this,
        thisTag: this && this.tag,
        accTag: acc.tag,
        value,
      };
    }, seed);
  `).run();

  assert.deepEqual(result, {
    same: false,
    thisType: 'undefined',
    thisTag: undefined,
    accTag: 'seed',
    value: 1,
  });
});

test('run updates array length writes and callback traversal semantics', async () => {
  const result = await runtime(`
    const someValues = [0, 1, 2];
    const someVisits = [];
    someValues.some((value, index, array) => {
      someVisits.push([index, value, index in array]);
      if (index === 0) {
        array.length = 1;
      }
      return false;
    });

    const reduced = [1, 2, 3];
    const reduceVisits = [];
    const reduceResult = reduced.reduce((acc, value, index, array) => {
      reduceVisits.push(index);
      if (index === 0) {
        array.length = 1;
      }
      return acc + value;
    }, 0);

    const reducedRight = [1, 2, 3];
    const reduceRightVisits = [];
    const reduceRightResult = reducedRight.reduceRight((acc, value, index, array) => {
      reduceRightVisits.push(index);
      if (index === 2) {
        array.length = 0;
      }
      return acc + value;
    }, 0);

    const sparse = [0, , 2];
    const findLastVisits = [];
    const findLastIndex = sparse.findLastIndex((value, index, array) => {
      findLastVisits.push([index, value, index in array]);
      return index === 1;
    });

    const invalidLength = (() => {
      try {
        const values = [1];
        values.length = 1.5;
        return 'unreachable';
      } catch (error) {
        return [error.name, error.message];
      }
    })();

    ({
      someVisits,
      someKeys: Object.keys(someValues),
      someLength: someValues.length,
      reduceResult,
      reduceVisits,
      reduceKeys: Object.keys(reduced),
      reduceLength: reduced.length,
      reduceRightResult,
      reduceRightVisits,
      reduceRightKeys: Object.keys(reducedRight),
      reduceRightLength: reducedRight.length,
      findLastIndex,
      findLastVisits,
      invalidLength,
    });
  `).run();

  assert.deepEqual(result, {
    someVisits: [[0, 0, true]],
    someKeys: ['0'],
    someLength: 1,
    reduceResult: 1,
    reduceVisits: [0],
    reduceKeys: ['0'],
    reduceLength: 1,
    reduceRightResult: 3,
    reduceRightVisits: [2],
    reduceRightKeys: [],
    reduceRightLength: 0,
    findLastIndex: 1,
    findLastVisits: [[2, 2, true], [1, undefined, false]],
    invalidLength: ['RangeError', 'Invalid array length'],
  });
});

test('run truncates Date timestamps to integer milliseconds', async () => {
  const expectedParsed = Date.parse('2026-04-10T14:00:00.123456789Z');
  const result = await runtime(`
    const now = Date.now();
    const current = new Date().getTime();
    const positive = new Date(1.9).getTime();
    const negative = new Date(-1.9).getTime();
    const dateOnly = new Date("1970-01-01").getTime();
    const parsed = new Date("2026-04-10T14:00:00.123456789Z").getTime();
    const clipped = new Date(8640000000000001).getTime();
    ({
      nowIsInteger: now === Math.trunc(now),
      currentIsInteger: current === Math.trunc(current),
      prototypeGetTime: typeof Date.prototype.getTime,
      prototypeValueOf: typeof Date.prototype.valueOf,
      positive,
      negative,
      dateOnly,
      parsed,
      clippedIsNaN: clipped !== clipped,
      valueOf: new Date(5).valueOf(),
    });
  `).run();

  assert.deepEqual(result, {
    nowIsInteger: true,
    currentIsInteger: true,
    prototypeGetTime: 'function',
    prototypeValueOf: 'function',
    positive: 1,
    negative: -1,
    dateOnly: 0,
    parsed: expectedParsed,
    clippedIsNaN: true,
    valueOf: 5,
  });
});

test('run matches Date invalid and extended-year semantics', async () => {
  const result = await runtime(`
    ({
      negativeZeroIsClipped: new Date(-0.1).getTime() === 0,
      negativeZeroReciprocalPositive: (1 / new Date(-0.1).getTime()) > 0,
      invalidYearIsNaN: Number.isNaN(new Date(0 / 0).getUTCFullYear()),
      invalidMonthIsNaN: Number.isNaN(new Date(0 / 0).getUTCMonth()),
      maxIso: new Date(8640000000000000).toISOString(),
      maxJson: new Date(8640000000000000).toJSON(),
      extendedYear: new Date("+010000-01-01T00:00:00.000Z").getTime(),
      negativeYearJson: new Date(-62198755200000).toJSON(),
    });
  `).run();

  assert.deepEqual(result, {
    negativeZeroIsClipped: true,
    negativeZeroReciprocalPositive: true,
    invalidYearIsNaN: true,
    invalidMonthIsNaN: true,
    maxIso: '+275760-09-13T00:00:00.000Z',
    maxJson: '+275760-09-13T00:00:00.000Z',
    extendedYear: 253402300800000,
    negativeYearJson: '-000001-01-01T00:00:00.000Z',
  });
});

test('run supports broader Date, Number, string-formatting, and reverse array helper surface', async () => {
  const result = await runtime(`
    const date = new Date("2026-04-10T14:05:06.789Z");
    ({
      iso: date.toISOString(),
      json: JSON.stringify({ date }),
      utc: [
        date.getUTCFullYear(),
        date.getUTCMonth(),
        date.getUTCDate(),
        date.getUTCHours(),
        date.getUTCMinutes(),
        date.getUTCSeconds(),
      ],
      parsedInt: Number.parseInt("  -0x10"),
      globalParsedInt: parseInt("08"),
      parsedFloat: Number.parseFloat("  -10.25ms"),
      isNaN: Number.isNaN(0 / 0),
      isNaNString: Number.isNaN("NaN"),
      globalIsNaN: isNaN(NaN),
      isFinite: Number.isFinite(12.5),
      isFiniteInfinite: Number.isFinite(1 / 0),
      globalIsFinite: isFinite(12.5),
      isInteger: Number.isInteger(12),
      isSafeInteger: Number.isSafeInteger(Number.MAX_SAFE_INTEGER),
      maxSafeInteger: Number.MAX_SAFE_INTEGER,
      minSafeInteger: Number.MIN_SAFE_INTEGER,
      epsilon: Number.EPSILON > 0 && Number.EPSILON < 1,
      numberNaN: Number.isNaN(Number.NaN),
      positiveInfinity: Number.POSITIVE_INFINITY,
      negativeInfinity: Number.NEGATIVE_INFINITY,
      globalInfinity: Infinity,
      trimStart: "  padded  ".trimStart(),
      trimEnd: "  padded  ".trimEnd(),
      padStart: "7".padStart(3, "0"),
      padEnd: "7".padEnd(3, "0"),
      reduceRight: [1, 2, 3].reduceRight((acc, value) => acc + ":" + value, "tail"),
      findLast: [1, 2, 3, 4].findLast((value) => value % 2 === 0),
      findLastIndex: [1, 2, 3, 4].findLastIndex((value) => value % 2 === 0),
      mathPiRounded: Math.round(Math.PI * 1000) / 1000,
      mathExpRounded: Math.round(Math.exp(1) * 1000) / 1000,
      mathLog2: Math.log2(8),
      mathLog10: Math.log10(1000),
      mathSinRounded: Math.round(Math.sin(Math.PI / 2) * 1000) / 1000,
      mathCosRounded: Math.round(Math.cos(Math.PI) * 1000) / 1000,
      mathAtan2Rounded: Math.round(Math.atan2(0, -1) * 1000) / 1000,
      mathHypot: Math.hypot(3, 4),
      mathCbrt: Math.cbrt(27),
      syntaxError: [
        new SyntaxError("bad").name,
        new SyntaxError("bad") instanceof SyntaxError,
        new SyntaxError("bad") instanceof Error,
      ],
    });
  `).run();

  assert.deepEqual(result, {
    iso: '2026-04-10T14:05:06.789Z',
    json: '{"date":"2026-04-10T14:05:06.789Z"}',
    utc: [2026, 3, 10, 14, 5, 6],
    parsedInt: -16,
    globalParsedInt: 8,
    parsedFloat: -10.25,
    isNaN: true,
    isNaNString: false,
    globalIsNaN: true,
    isFinite: true,
    isFiniteInfinite: false,
    globalIsFinite: true,
    isInteger: true,
    isSafeInteger: true,
    maxSafeInteger: 9007199254740991,
    minSafeInteger: -9007199254740991,
    epsilon: true,
    numberNaN: true,
    positiveInfinity: Infinity,
    negativeInfinity: -Infinity,
    globalInfinity: Infinity,
    trimStart: 'padded  ',
    trimEnd: '  padded',
    padStart: '007',
    padEnd: '700',
    reduceRight: 'tail:3:2:1',
    findLast: 4,
    findLastIndex: 3,
    mathPiRounded: 3.142,
    mathExpRounded: 2.718,
    mathLog2: 3,
    mathLog10: 3,
    mathSinRounded: 1,
    mathCosRounded: -1,
    mathAtan2Rounded: 3.142,
    mathHypot: 5,
    mathCbrt: 3,
    syntaxError: ['SyntaxError', true, true],
  });
});

test('run supports additional string and array helper surface', async () => {
  const result = await runtime(`
    const values = [1, , 3, 1];
    values.label = "seed";
    const reversed = values.reverse();
    const filled = Array(4);
    filled.fill("x", 1, 3);
    ({
      stringIndexOf: "banana".indexOf("na", 1),
      stringLastIndexOf: "banana".lastIndexOf("na"),
      charAt: "hello".charAt(1),
      at: "hello".at(-2),
      missingAt: "hello".at(9),
      repeat: "ha".repeat(3),
      concat: "alpha".concat("-", 2, true),
      reversedIdentity: reversed === values,
      reversedKeys: Object.keys(values),
      reversedValues: Array.from(values.values()),
      reversedLastIndexOf: values.lastIndexOf(1),
      filledKeys: Object.keys(filled),
      filledValues: Array.from(filled.values()),
    });
  `).run();

  assert.deepEqual(result, {
    stringIndexOf: 2,
    stringLastIndexOf: 4,
    charAt: 'e',
    at: 'l',
    missingAt: undefined,
    repeat: 'hahaha',
    concat: 'alpha-2true',
    reversedIdentity: true,
    reversedKeys: ['0', '1', '3', 'label'],
    reversedValues: [1, 3, undefined, 1],
    reversedLastIndexOf: 3,
    filledKeys: ['1', '2'],
    filledValues: [undefined, 'x', 'x', undefined],
  });
});

test('run supports a narrow Intl DateTimeFormat and NumberFormat subset', async () => {
  const result = await runtime(`
    const date = new Date("2026-04-10T14:05:06.789Z");
    const dateFormatter = new Intl.DateTimeFormat("en-US", {
      timeZone: "UTC",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    });
    const numberFormatter = Intl.NumberFormat("en-US", {
      style: "currency",
      currency: "USD",
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
    ({
      date: dateFormatter.format(date),
      dateOptions: dateFormatter.resolvedOptions(),
      currency: numberFormatter.format(1234.5),
      negativeCurrency: numberFormatter.format(-1.23),
      currencyOptions: numberFormatter.resolvedOptions(),
      hourMinute: Intl.DateTimeFormat("en-US", {
        timeZone: "UTC",
        hour: "numeric",
        minute: "2-digit",
      }).format(date),
      decimal: Intl.NumberFormat("en-US", {
        useGrouping: false,
        minimumFractionDigits: 1,
        maximumFractionDigits: 1,
      }).format(12),
    });
  `).run();

  assert.deepEqual(result, {
    date: '04/10/2026',
    dateOptions: {
      locale: 'en-US',
      timeZone: 'UTC',
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
    },
    currency: '$1,234.50',
    negativeCurrency: '-$1.23',
    currencyOptions: {
      locale: 'en-US',
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
      useGrouping: true,
    },
    hourMinute: '2:05 PM',
    decimal: '12.0',
  });
});

test('Intl and new helper additions fail closed for unsupported options and invalid callbacks', async () => {
  await assert.rejects(
    () => runtime('Intl.DateTimeFormat("fr-FR");').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Intl currently supports only the `en-US` locale',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('Intl.DateTimeFormat("en-US", { timeZone: "America/Los_Angeles" });').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Intl.DateTimeFormat currently supports only the `UTC` timeZone',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('Intl.NumberFormat("en-US", { style: "currency", currency: "EUR" });').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Intl.NumberFormat currency style currently supports only `USD`',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('Intl.DateTimeFormat("en-US", { weekday: "long" });').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Intl.DateTimeFormat does not support the `weekday` option',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('Intl.NumberFormat("en-US", { notation: "scientific" });').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Intl.NumberFormat does not support the `notation` option',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () =>
      runtime(
        'Intl.DateTimeFormat("en-US", { timeZone: "UTC", year: "numeric" }).format(new Date(0 / 0));',
      ).run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'RangeError: Invalid time value',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('([].reduceRight((acc, value) => acc + value));').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.reduceRight requires an initial value for empty arrays',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('([1]).findLast(1);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'Array.prototype.findLast expects a callable callback',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    () => runtime('"ha".repeat(-1);').run(),
    isJsliteError({
      kind: 'Runtime',
      message: 'RangeError: Invalid count value',
      guestSafe: true,
    }),
  );
});

test('run binds rest parameters for functions and arrows', async () => {
  const result = await runtime(`
    function collect(head, ...tail) {
      return [head, tail.length, tail[0], tail[1]];
    }
    const sumFirstTwo = (...[first, second]) => first + second;
    [collect(1, 2, 3), sumFirstTwo(4, 5, 6)];
  `).run();

  assert.deepEqual(result, [[1, 2, 2, 3], 9]);
});
