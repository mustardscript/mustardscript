const test = require('node:test');

const { assertDifferential } = require('./runtime-oracle.js');

const DIFFERENTIAL_CASES = [
  {
    name: 'arithmetic and locals',
    source: `
      const a = 4;
      const b = 3;
      a * b + 2;
    `,
  },
  {
    name: 'closures and calls',
    source: `
      function outer() {
        let x = 2;
        function inner(y) {
          return x + y;
        }
        return inner(5);
      }
      outer();
    `,
  },
  {
    name: 'global object and lexical this semantics',
    source: `
      globalThis.answer = 7;
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
        topLevelThis: this === globalThis,
        arrowThis: box.method(),
      });
    `,
  },
  {
    name: 'branching, loops, and switch',
    source: `
      let total = 0;
      for (let i = 0; i < 4; i += 1) {
        if (i === 1) {
          continue;
        }
        total += i;
      }
      switch (total) {
        case 5:
          total += 4;
          break;
        default:
          total = 0;
      }
      total;
    `,
  },
  {
    name: 'simple array and object destructuring',
    source: `
      const pair = [4, 3];
      let [left, right] = pair;
      let { x, y } = { x: left, y: right };
      left + right + x + y;
    `,
  },
  {
    name: 'defaults destructuring assignment and update expressions',
    source: `
      function wrap(value = 1, next = value + 1, { label = "ok" } = {}) {
        return [value, next, label];
      }
      let left = 0;
      const record = { count: 4 };
      [left, record.count] = [2, 3];
      ({ left = 5, count: record.count = 7 } = { count: 9 });
      ({
        defaults: wrap(),
        postfix: left++,
        afterPostfix: left,
        prefix: ++record.count,
        final: [left, record.count],
      });
    `,
  },
  {
    name: 'optional chaining and nullish coalescing',
    source: `
      const present = { nested: { value: 3 } };
      const missing = null;
      [
        present?.nested?.value ?? 0,
        missing?.nested?.value ?? 7,
        ({ maybe: undefined }).maybe ?? 9,
      ];
    `,
  },
  {
    name: 'nullish assignment keeps existing values and fills missing ones',
    source: `
      let left;
      left ??= 4;
      const record = { present: 5, missing: undefined };
      record.present ??= 8;
      record.missing ??= 9;
      [left, record.present, record.missing];
    `,
  },
  {
    name: 'logical assignment short-circuits and evaluates member targets once',
    source: `
      let orAssign = 0;
      let orKeep = 7;
      let andAssign = 3;
      let andKeep = 0;
      const counts = { object: 0, key: 0, rhs: 0 };
      const record = { slot: 0, gate: 2 };
      function object() {
        counts.object += 1;
        return record;
      }
      function key() {
        counts.key += 1;
        return "slot";
      }
      function rhs(value) {
        counts.rhs += 1;
        return value;
      }
      ({
        orAssign: (orAssign ||= 4),
        orKeep: (orKeep ||= 9),
        andAssign: (andAssign &&= 5),
        andKeep: (andKeep &&= 9),
        computedOrAssign: (object()[key()] ||= rhs(6)),
        computedOrKeep: (object()[key()] ||= rhs(7)),
        staticAndAssign: (object().gate &&= rhs(8)),
        staticAndKeep: ((record.gate = 0), object().gate &&= rhs(9)),
        final: { orAssign, orKeep, andAssign, andKeep, slot: record.slot, gate: record.gate },
        counts,
      });
    `,
  },
  {
    name: 'array helpers',
    source: `
      const values = [1, 2];
      values.push(3, 4);
      [
        values.pop(),
        values.slice(1, 3),
        values.join('-'),
        values.includes(2),
        values.indexOf(3),
      ];
    `,
  },
  {
    name: 'array callback helpers',
    source: `
      const values = [1, 2, 3];
      let seen = 0;
      const mapped = values.map(function (value, index) {
        seen += this.step;
        return value + index + this.offset;
      }, { step: 10, offset: 4 });
      values.forEach((value) => {
        seen += value;
      });
      ({
        mapped,
        filtered: values.filter((value) => value % 2 === 1),
        found: values.find((value) => value > 2),
        foundIndex: values.findIndex((value) => value > 2),
        some: values.some((value) => value === 2),
        every: values.every((value) => value > 0),
        reduced: values.reduce((acc, value) => acc + value, 5),
        seen,
      });
    `,
  },
  {
    name: 'additional Array and Math helpers',
    source: `
      const merged = Array.of(1, 2, 3).concat([4, 5], 6);
      const random = Math.random();
      ({
        merged,
        atFront: merged.at(0),
        atFromEnd: merged.at(-2),
        atMissing: merged.at(99),
        logOne: Math.log(1),
        logZero: Math.log(0),
        randomIsNumber: typeof random === "number",
        randomInRange: random >= 0 && random < 1,
        randomIsFinite: random === random && random !== (1 / 0) && random !== (-1 / 0),
      });
    `,
  },
  {
    name: 'Array splice flat and flatMap helpers',
    source: `
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
    `,
  },
  {
    name: 'sparse array holes across helpers and JSON',
    source: `
      const values = [1, , undefined, 4];
      const callbackIndexes = [];
      values.forEach((value, index) => {
        callbackIndexes[callbackIndexes.length] = index;
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
        joined: values.join('-'),
        json: JSON.stringify(values),
        callbackIndexes,
        slicedKeys: Object.keys(sliced),
        mappedKeys: Object.keys(mapped),
        mappedHasHole: 1 in mapped,
        mergedKeys: Object.keys(merged),
        mergedTail: merged[5],
      });
    `,
  },
  {
    name: 'string helpers',
    source: `
      const value = "  MiXeD Example  ";
      const csv = "alpha,beta,gamma";
      [
        value.trim(),
        value.includes("XeD"),
        value.startsWith("Mi", 2),
        value.endsWith("ple  "),
        value.slice(2, -2),
        value.substring(8, 3),
        value.toLowerCase(),
        value.toUpperCase(),
        csv.split(",", 2),
        value.replace("MiXeD", "Mixed"),
        "a-b-a".replaceAll("a", "z"),
        value.search("Example"),
        value.match("Example"),
      ];
    `,
  },
  {
    name: 'string relational comparisons',
    source: `
      const left = "az";
      const right = "ba";
      [
        left < right,
        left <= left,
        right > left,
        right >= right,
      ];
    `,
  },
  {
    name: 'RegExp helpers and callback replacements',
    source: `
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
    `,
  },
  {
    name: 'Object and Math helpers',
    source: `
      const object = { alpha: 2, zebra: 1 };
      const array = [4, 5];
      array.extra = 6;
      const assignedTarget = { alpha: 2 };
      const assigned = Object.assign(
        assignedTarget,
        { zebra: 1 },
        undefined,
        { beta: 3 },
      );
      const assignedArrayTarget = [4];
      assignedArrayTarget.label = 7;
      const assignedArray = Object.assign(
        assignedArrayTarget,
        { 1: 5 },
        [6],
        null,
        { extra: 8 },
      );
      ({
        keys: Object.keys(object),
        values: Object.values(object),
        entries: Object.entries(array),
        hasOwn: Object.hasOwn(object, "alpha"),
        assignObject: [
          assigned === assignedTarget,
          assigned.alpha,
          assigned.beta,
          assigned.zebra,
        ],
        assignArray: [
          assignedArray === assignedArrayTarget,
          assignedArray[0],
          assignedArray[1],
          assignedArray.extra,
          assignedArray.label,
        ],
        pow: Math.pow(2, 5),
        sqrt: Math.sqrt(81),
        trunc: Math.trunc(-3.9),
        sign: Math.sign(-12),
      });
    `,
  },
  {
    name: 'sequence expressions and exponentiation',
    source: `
      let steps = 0;
      const number = (steps = steps + 1, steps = steps + 2, 2 ** 3 ** 2);
      ({
        number,
        steps,
        bigint: String(2n ** 5n),
      });
    `,
  },
  {
    name: 'conservative in operator surface',
    source: `
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
    `,
  },
  {
    name: 'conservative instanceof surface',
    source: `
      function Box() {}
      ({
        array: [] instanceof Array,
        arrayObject: [] instanceof Object,
        map: new Map() instanceof Map,
        mapObject: new Map() instanceof Object,
        set: new Set() instanceof Set,
        promise: Promise.resolve(1) instanceof Promise,
        error: new TypeError("boom") instanceof Error,
        guestFunction: ({}) instanceof Box,
      });
    `,
  },
  {
    name: 'callable metadata and constructor links',
    source: `
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
    `,
  },
  {
    name: 'Array Object and primitive-wrapper constructor semantics',
    source: `
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
    `,
  },
  {
    name: 'property order and JSON.stringify rendering',
    source: `
      const object = {};
      object.beta = 2;
      object[10] = 10;
      object.alpha = 1;
      object[2] = 3;
      object["01"] = 4;
      const values = [1, 2];
      values["01"] = 7;
      values[4294967295] = 9;
      ({
        objectKeys: Object.keys(object),
        objectValues: Object.values(object),
        objectEntries: Object.entries(object),
        arrayKeys: Object.keys(values),
        arrayEntries: Object.entries(values),
        arrayProps: [values["01"], values[4294967295], values.length],
        stringifiedObject: JSON.stringify(object),
        stringifiedArray: JSON.stringify(values),
        stringifiedMixed: JSON.stringify({
          keep: 1,
          skipUndefined: undefined,
          skipFunction: () => 1,
          values: [undefined, () => 2, -0, (1 / 0)],
        }),
      });
    `,
  },
  {
    name: 'reduce callback this and Date millisecond truncation',
    source: `
      const seed = { tag: "seed" };
      const reduced = [1].reduce(function (acc, value) {
        return {
          same: this === acc,
          thisType: typeof this,
          thisTag: this && this.tag,
          accTag: acc.tag,
          value,
        };
      }, seed);
      const now = Date.now();
      const current = new Date().getTime();
      ({
        reduced,
        nowIsInteger: now === Math.trunc(now),
        currentIsInteger: current === Math.trunc(current),
        positive: new Date(1.9).getTime(),
        negative: new Date(-1.9).getTime(),
        parsed: new Date("2026-04-10T14:00:00.123456789Z").getTime(),
      });
    `,
  },
  {
    name: 'supported iterable surfaces',
    source: `
      const map = new Map([['alpha', 1], ['beta', 2], ['alpha', 3]]);
      const set = new Set('abba');
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
      ({
        size: map.size,
        alpha: map.get('alpha'),
        setSize: set.size,
        chars,
        setChars,
        pair: [pair.value[0], pair.value[1], pair.done],
        seen,
      });
    `,
  },
  {
    name: 'optional call on nullish and callable values',
    source: `
      const fn = (value) => value + 1;
      const missing = null;
      [fn?.(2), missing?.(2)];
    `,
  },
  {
    name: 'template literals',
    source: `
      const prefix = 'hi';
      \`value=\${prefix}-\${2 + 3}\`;
    `,
  },
  {
    name: 'object mutation through computed members',
    source: `
      const key = 'value';
      const obj = {};
      obj[key] = 3;
      obj.other = 4;
      ({ total: obj[key] + obj.other, key: obj[key] });
    `,
  },
  {
    name: 'object literals with computed keys, methods, and spread',
    source: `
      const key = 'value';
      const extra = [3];
      extra.label = 'ok';
      const obj = {
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
      ({ value: obj.value, zero: obj[0], label: obj.label, total: obj.total(5) });
    `,
  },
  {
    name: 'array spread and spread arguments over supported iterables',
    source: `
      const values = [2, 4];
      const sparse = [2, , 4];
      const extra = new Set('ab');
      const box = {
        base: 10,
        total(...args) {
          return this.base + args[0] + args[1] + args[2] + args[3];
        },
      };
      ({
        array: [1, ...sparse, ...extra, 5],
        total: box.total(...values, 6),
        spreadValue: [1, ...sparse][2],
        max: Math.max(...values, 3),
        built: new Array(...values, 9),
        missing: ({ maybe: null }).maybe?.(...values),
      });
    `,
  },
  {
    name: 'array growth through indexed writes',
    source: `
      const values = [1, 2];
      values[values.length] = 4;
      ({ first: values[0], third: values[2], size: values.length });
    `,
  },
  {
    name: 'Math and JSON built-ins',
    source: `
      const parsed = JSON.parse('{"a":2,"b":[1,3]}');
      Math.max(parsed.a, parsed.b[1]) + Math.abs(-4);
    `,
  },
  {
    name: 'try, catch, and finally with thrown primitives',
    source: `
      let events = [];
      function run(flag) {
        try {
          events[events.length] = 'body';
          if (flag) {
            throw 'boom';
          }
          return 'ok';
        } catch (error) {
          events[events.length] = error;
          return 'caught';
        } finally {
          events[events.length] = 'finally';
        }
      }
      [run(true), run(false), events];
    `,
  },
  {
    name: 'built-in error constructors',
    source: `
      const error = new TypeError('boom');
      ({ name: error.name, message: error.message });
    `,
  },
  {
    name: 'recursion over a supported call depth',
    source: `
      function fact(value) {
        if (value <= 1) {
          return 1;
        }
        return value * fact(value - 1);
      }
      fact(5);
    `,
  },
  {
    name: 'Array.isArray and Math combinations',
    source: `
      [
        Array.isArray([1, 2, 3]),
        Array.isArray({ length: 3 }),
        Math.min(4, -3, 8) + Math.round(2.4),
      ];
    `,
  },
  {
    name: 'array for...of observes order and growth',
    source: `
      const values = [1, 2];
      const seen = [];
      for (let value of values) {
        seen[seen.length] = value;
        if (value === 1) {
          values[values.length] = 3;
        }
      }
      seen;
    `,
  },
  {
    name: 'array for...of creates fresh iteration bindings',
    source: `
      const fns = [];
      for (const [value] of [[1], [2]]) {
        fns[fns.length] = () => value;
      }
      [fns[0](), fns[1]()];
    `,
  },
  {
    name: 'for...of assignment-target headers reuse existing bindings',
    source: `
      let value = 0;
      const fns = [];
      for (value of [1, 2]) {
        fns[fns.length] = () => value;
      }
      [fns[0](), fns[1](), value];
    `,
  },
  {
    name: 'for...of assignment-target member headers reevaluate each iteration',
    source: `
      const boxes = [{ current: 0 }, { current: 0 }];
      let index = 0;
      for (boxes[index].current of [3, 4]) {
        index += 1;
      }
      [boxes[0].current, boxes[1].current, index];
    `,
  },
  {
    name: 'for await...of awaits yielded values from supported iterables',
    source: `
      async function main() {
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
      main();
    `,
  },
  {
    name: 'Map lookup and SameValueZero semantics',
    source: `
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
        map.get(shared),
        map.delete('missing'),
        map.delete(nan),
        map.has(nan),
        map.size,
      ];
    `,
  },
  {
    name: 'Set membership and clear semantics',
    source: `
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
    `,
  },
];

for (const { name, source } of DIFFERENTIAL_CASES) {
  test(`matches Node for ${name}`, async () => {
    await assertDifferential(source);
  });
}
