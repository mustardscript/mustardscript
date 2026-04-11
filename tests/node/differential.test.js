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
    name: 'string helpers',
    source: `
      const value = "  MiXeD Example  ";
      [
        value.trim(),
        value.includes("XeD"),
        value.startsWith("Mi", 2),
        value.endsWith("ple  "),
        value.slice(2, -2),
        value.substring(8, 3),
        value.toLowerCase(),
        value.toUpperCase(),
      ];
    `,
  },
  {
    name: 'Object and Math helpers',
    source: `
      const object = { zebra: 1, alpha: 2 };
      const array = [4, 5];
      array.extra = 6;
      ({
        keys: Object.keys(object),
        values: Object.values(object),
        entries: Object.entries(array),
        hasOwn: Object.hasOwn(object, "alpha"),
        pow: Math.pow(2, 5),
        sqrt: Math.sqrt(81),
        trunc: Math.trunc(-3.9),
        sign: Math.sign(-12),
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
