'use strict';

const fc = require('fast-check');
const {
  DIAGNOSTIC_CATEGORY,
  FORBIDDEN_AMBIENT_GLOBALS,
  REJECT_PHASE,
  RUNTIME_REJECT_CASES,
  VALIDATION_REJECT_CASES,
} = require('./conformance-contract.js');

const PROPERTY_RUNS = process.env.CI ? 100 : 50;
const IDENTIFIERS = ['alpha', 'beta', 'gamma', 'delta', 'omega', 'theta'];
const ASCII_CHARS = ['a', 'b', 'c', 'd', 'e', 'x', 'y', 'z', ' ', '-', ',', '_'];
const LETTER_CHARS = ['a', 'b', 'c', 'd', 'e', 'x', 'y', 'z'];

const identifierArbitrary = fc.constantFrom(...IDENTIFIERS);
const smallStringArbitrary = fc
  .array(fc.constantFrom(...ASCII_CHARS), { maxLength: 6 })
  .map((chars) => chars.join(''));
const wordStringArbitrary = fc
  .array(fc.constantFrom(...LETTER_CHARS), { minLength: 1, maxLength: 5 })
  .map((chars) => chars.join(''));
const finiteIntegerArbitrary = fc.integer({ min: -6, max: 6 });
const positiveFiniteIntegerArbitrary = fc.integer({ min: 1, max: 6 });
const numericEdgeCaseArbitrary = fc.constantFrom(-0, NaN, Infinity, -Infinity);
const supportedNumberArbitrary = fc.oneof(finiteIntegerArbitrary, numericEdgeCaseArbitrary);
const smallIntegerArrayArbitrary = fc.array(finiteIntegerArbitrary, { minLength: 1, maxLength: 4 });
const sortedObjectEntryArbitrary = fc
  .uniqueArray(fc.tuple(identifierArbitrary, finiteIntegerArbitrary), {
    selector: ([key]) => key,
    minLength: 1,
    maxLength: 4,
  })
  .map((entries) => [...entries].sort(([left], [right]) => left.localeCompare(right)));
const orderedPropertyKeyArbitrary = fc.constantFrom(
  'alpha',
  'beta',
  'zebra',
  'middle',
  '0',
  '1',
  '2',
  '10',
  '01',
  '4294967294',
  '4294967295',
);
const orderedObjectEntryArbitrary = fc.uniqueArray(
  fc.tuple(orderedPropertyKeyArbitrary, finiteIntegerArbitrary),
  {
    selector: ([key]) => key,
    minLength: 3,
    maxLength: 5,
  },
);
const mapKeyArbitrary = fc.oneof(
  identifierArbitrary,
  finiteIntegerArbitrary,
  fc.constant(null),
  fc.constant(-0),
  fc.constant(NaN),
);
const mapValueArbitrary = fc.oneof(finiteIntegerArbitrary, smallStringArbitrary, fc.boolean());
const mapEntriesArbitrary = fc.array(fc.tuple(mapKeyArbitrary, mapValueArbitrary), {
  minLength: 1,
  maxLength: 4,
});

function renderNumberLiteral(value) {
  if (Number.isNaN(value)) {
    return '(0 / 0)';
  }
  if (Object.is(value, -0)) {
    return '(-0)';
  }
  if (value === Infinity) {
    return '(1 / 0)';
  }
  if (value === -Infinity) {
    return '(-1 / 0)';
  }
  return String(value);
}

function renderLiteral(value) {
  if (value === undefined) {
    return 'undefined';
  }
  if (value === null) {
    return 'null';
  }
  if (typeof value === 'boolean') {
    return value ? 'true' : 'false';
  }
  if (typeof value === 'number') {
    return renderNumberLiteral(value);
  }
  if (typeof value === 'bigint') {
    return `${value}n`;
  }
  if (typeof value === 'string') {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map(renderLiteral).join(', ')}]`;
  }
  return `{ ${Object.keys(value)
    .sort()
    .map((key) => `${JSON.stringify(key)}: ${renderLiteral(value[key])}`)
    .join(', ')} }`;
}

function validationCase(source, messageIncludes, category = DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX) {
  return {
    source,
    messageIncludes,
    phase: REJECT_PHASE.CONSTRUCTOR,
    category,
  };
}

function parityCase(source, options = undefined) {
  return options === undefined ? { source: source.trim() } : { source: source.trim(), options };
}

const coreParityCaseArbitraries = [
  fc
    .record({
      left: supportedNumberArbitrary,
      right: supportedNumberArbitrary,
      divisor: positiveFiniteIntegerArbitrary,
      fallback: finiteIntegerArbitrary,
    })
    .map(({ left, right, divisor, fallback }) =>
      parityCase(`
        const left = ${renderNumberLiteral(left)};
        const right = ${renderNumberLiteral(right)};
        ({
          add: left + right,
          sub: left - right,
          mul: right * ${divisor},
          div: left / ${divisor},
          rem: left % ${divisor},
          cmp: [left < right, left <= right, left > right, left >= right],
          strict: [left === right, left !== right],
          logical: [left && right, left || right, null ?? ${fallback}],
          unary: [typeof left, !right, void left],
        });
      `),
    ),
  fc
    .record({
      base: finiteIntegerArbitrary,
      delta: finiteIntegerArbitrary,
      value: finiteIntegerArbitrary,
      bonus: finiteIntegerArbitrary,
    })
    .map(({ base, delta, value, bonus }) =>
      parityCase(`
        const base = ${base};
        function outer(delta, ...rest) {
          let captured = base + ${bonus} + rest[0];
          function inner(value) {
            return captured + delta + value;
          }
          return inner(${value});
        }
        const toolkit = {
          base,
          add: function (step) {
            return this.base + step;
          },
        };
        ({ result: outer(${delta}, 1), base, method: toolkit.add(${delta}) });
      `),
    ),
  fc
    .record({
      left: finiteIntegerArbitrary,
      right: finiteIntegerArbitrary,
      fallback: finiteIntegerArbitrary,
      missing: finiteIntegerArbitrary,
    })
    .map(({ left, right, fallback, missing }) =>
      parityCase(`
        const pair = [${left}, ${right}];
        const record = { alpha: pair[0], nested: { value: pair[1] }, missing: undefined };
        let [first, second] = pair;
        let { alpha, nested: { value } } = record;
        let absent;
        absent ??= ${missing};
        record.alpha ??= ${fallback};
        record.missing ??= ${fallback};
        [
          first,
          second,
          alpha,
          value,
          record?.nested?.value ?? ${fallback},
          ({ maybe: undefined }).maybe ?? ${fallback},
          absent,
          record.alpha,
          record.missing,
        ];
      `),
    ),
  fc
    .record({
      raw: smallStringArbitrary,
      needle: fc.constantFrom('a', 'b', ' ', '-', ','),
      replacement: fc.constantFrom('x', 'y', 'z'),
      start: fc.integer({ min: -3, max: 3 }),
      end: fc.integer({ min: -3, max: 6 }),
      limit: fc.integer({ min: 0, max: 4 }),
    })
    .map(({ raw, needle, replacement, start, end, limit }) =>
      parityCase(`
        const value = ${JSON.stringify(` ${raw} `)};
        [
          value.trim(),
          value.includes(${JSON.stringify(needle)}),
          value.startsWith(${JSON.stringify(needle)}),
          value.endsWith(${JSON.stringify(needle)}),
          value.slice(${start}, ${end}),
          value.substring(${Math.abs(start)}, ${Math.abs(end)}),
          value.split(${JSON.stringify(needle)}, ${limit}),
          value.replace(${JSON.stringify(needle)}, ${JSON.stringify(replacement)}),
          value.replaceAll(${JSON.stringify(needle)}, ${JSON.stringify(replacement)}),
          value.search(${JSON.stringify(needle)}),
          value.match(${JSON.stringify(needle)}),
        ];
      `),
    ),
  fc
    .record({
      first: wordStringArbitrary,
      second: wordStringArbitrary,
      replacement: fc.constantFrom('x', 'y', 'z'),
    })
    .map(({ first, second, replacement }) =>
      parityCase(`
        const combined = ${JSON.stringify(`${first}12${second}34`)};
        const matches = Array.from(
          combined.matchAll(/(?<letters>[a-z]+)(\\d+)/g),
        ).map((match) => [
          match[0],
          match[1],
          match[2],
          match.index,
          match.groups.letters,
        ]);
        const tested = (() => {
          const regex = /a/g;
          return [regex.test('ba'), regex.lastIndex, regex.test('ba'), regex.lastIndex];
        })();
        ({
          matches,
          exec: (() => {
            const regex = /(?<letters>[a-z]+)(\\d+)/g;
            const first = regex.exec(combined);
            const second = regex.exec(combined);
            return [
              [first[0], first[1], first[2], first.index, first.groups.letters, regex.lastIndex],
              [second[0], second[1], second[2], second.index, second.groups.letters, regex.lastIndex],
            ];
          })(),
          replace: combined.replaceAll(
            /([a-z]+)(\\d+)/g,
            (match, letters, digits, offset) =>
              letters.toUpperCase() + ${JSON.stringify(replacement)} + digits + ':' + offset,
          ),
          tested,
        });
      `),
    ),
  fc
    .record({
      first: fc.integer({ min: -1_000, max: 1_000 }),
      second: fc.integer({ min: -1_000, max: 1_000 }),
    })
    .map(({ first, second }) =>
      parityCase(`
        const initial = new Date(${first});
        const cloned = new Date(initial);
        const other = new Date(${second});
        [initial.getTime(), cloned.getTime(), other.getTime()];
      `),
    ),
  fc
    .record({
      left: fc.integer({ min: -12, max: 12 }),
      right: fc.integer({ min: 1, max: 12 }),
      extra: fc.integer({ min: -12, max: 12 }),
    })
    .map(({ left, right, extra }) =>
      parityCase(`
        const left = ${left}n;
        const right = ${right}n;
        const extra = ${extra}n;
        ({
          sum: String(left + right),
          diff: String(left - extra),
          product: String(right * extra),
          quotient: String((left + right) / right),
          remainder: String((left + right) % right),
          compare: [left < right, left >= extra, typeof left],
        });
      `),
    ),
  fc
    .record({
      alpha: finiteIntegerArbitrary,
      beta: finiteIntegerArbitrary,
    })
    .map(({ alpha, beta }) =>
      parityCase(`
        const payload = JSON.parse(${JSON.stringify(`{"alpha":${alpha},"beta":[${beta}]}`)});
        \`payload=\${payload.alpha}-\${payload.beta[0]}\`;
      `),
    ),
  fc
    .record({
      base: finiteIntegerArbitrary,
      delta: finiteIntegerArbitrary,
      exponent: fc.integer({ min: 0, max: 5 }),
    })
    .map(({ base, delta, exponent }) =>
      parityCase(`
        let steps = 0;
        const base = ${base};
        const value = (steps = steps + 1, steps = steps + ${delta}, base ** ${exponent});
        ({ value, steps });
      `),
    ),
  fc.constant(
    parityCase(`
      let value = 0;
      let other = 3;
      [value ||= 4, other &&= 5, value, other];
    `),
  ),
];

const controlFlowParityCaseArbitraries = [
  fc
    .record({
      limit: positiveFiniteIntegerArbitrary,
      continueOn: finiteIntegerArbitrary,
      branchOn: finiteIntegerArbitrary,
    })
    .map(({ limit, continueOn, branchOn }) =>
      parityCase(`
        let total = 0;
        let steps = [];
        for (let index = 0; index < ${limit + 2}; index += 1) {
          if (index === ${Math.abs(continueOn) % (limit + 2)}) {
            continue;
          }
          total += index;
          steps[steps.length] = index;
        }
        switch (total > ${branchOn} ? 'large' : 'small') {
          case 'large':
            total += 3;
            break;
          default:
            total -= 2;
        }
        ({ total, steps });
      `),
    ),
  fc
    .record({
      outerLimit: fc.integer({ min: 2, max: 4 }),
      innerLimit: fc.integer({ min: 2, max: 5 }),
      continueOn: fc.integer({ min: 0, max: 4 }),
      breakOn: fc.integer({ min: 0, max: 4 }),
    })
    .map(({ outerLimit, innerLimit, continueOn, breakOn }) =>
      parityCase(`
        let total = 0;
        let events = [];
        let outer = 0;
        while (outer < ${outerLimit}) {
          outer += 1;
          for (let inner = 0; inner < ${innerLimit}; inner += 1) {
            try {
              if (inner === ${continueOn % innerLimit}) {
                events[events.length] = ['continue', outer, inner];
                continue;
              }
              total += outer + inner;
              events[events.length] = ['step', outer, inner, total];
              if (inner === ${breakOn % innerLimit}) {
                events[events.length] = ['break', outer, inner];
                break;
              }
            } finally {
              events[events.length] = ['finally', outer, inner];
            }
          }
        }
        ({ total, events });
      `),
    ),
  fc
    .record({
      values: smallIntegerArrayArbitrary,
      skip: finiteIntegerArbitrary,
      limit: fc.integer({ min: 0, max: 20 }),
    })
    .map(({ values, skip, limit }) =>
      parityCase(`
        const values = ${renderLiteral(values)};
        let total = 0;
        let events = [];
        for (const value of values) {
          try {
            if (value === ${skip}) {
              events[events.length] = 'continue:' + value;
              continue;
            }
            total += value;
            events[events.length] = 'step:' + value + ':' + total;
            if (total > ${limit}) {
              events[events.length] = 'break:' + value;
              break;
            }
          } finally {
            events[events.length] = 'finally:' + value;
          }
        }
        ({ total, events });
      `),
    ),
];

const exceptionParityCaseArbitraries = [
  fc
    .record({
      message: wordStringArbitrary,
    })
    .map(({ message }) =>
      parityCase(`
        let events = [];
        function run(flag) {
          try {
            events[events.length] = 'body';
            if (flag) {
              throw new RangeError(${JSON.stringify(message)});
            }
            return 'ok';
          } catch (error) {
            events[events.length] = error.name + ':' + error.message;
            return [error.name, error.message];
          } finally {
            events[events.length] = 'finally';
          }
        }
        [run(true), run(false), events];
      `),
    ),
  fc
    .record({
      first: wordStringArbitrary,
      second: wordStringArbitrary,
    })
    .map(({ first, second }) =>
      parityCase(`
        function run() {
          let events = [];
          try {
            try {
              events[events.length] = 'body';
              throw new Error(${JSON.stringify(first)});
            } catch (error) {
              events[events.length] = error.message;
              throw new TypeError(${JSON.stringify(second)});
            } finally {
              events[events.length] = 'inner-finally';
            }
          } catch (error) {
            events[events.length] = error.name + ':' + error.message;
            return events;
          } finally {
            events[events.length] = 'outer-finally';
          }
        }
        run();
      `),
    ),
];

const objectsArraysParityCaseArbitraries = [
  fc
    .record({
      values: fc.array(finiteIntegerArbitrary, { minLength: 1, maxLength: 4 }),
      extra: finiteIntegerArbitrary,
      threshold: finiteIntegerArbitrary,
      seed: finiteIntegerArbitrary,
    })
    .map(({ values, extra, threshold, seed }) => {
      const renderedValues = `[${values.join(', ')}]`;
      return parityCase(`
        const values = ${renderedValues};
        const mapped = values.map(function (value, index) {
          return value + index + this.offset;
        }, { offset: ${extra} });
        ({
          mapped,
          filtered: values.filter((value) => value > ${threshold}),
          found: values.find((value) => value > ${threshold}),
          foundIndex: values.findIndex((value) => value > ${threshold}),
          some: values.some((value) => value === ${extra}),
          every: values.every((value) => value >= ${threshold}),
          reduced: values.reduce((acc, value) => acc + value, ${seed}),
        });
      `);
    }),
  fc
    .record({
      values: smallIntegerArrayArbitrary,
      offset: finiteIntegerArbitrary,
    })
    .map(({ values, offset }) => {
      const renderedValues = renderLiteral(values);
      return parityCase(`
        const values = ${renderedValues};
        const iterated = Array.from(values.entries()).map((entry) => [entry[0], entry[1]]);
        const fromSet = Array.from(new Set(values), function (value, index) {
          return value + index + this.offset;
        }, { offset: ${offset} });
        const sorted = values.slice().sort((left, right) => left - right);
        ({ iterated, fromSet, sorted });
      `);
    }),
  fc
    .record({
      values: smallIntegerArrayArbitrary,
      extra: finiteIntegerArbitrary,
      suffix: finiteIntegerArbitrary,
    })
    .map(({ values, extra, suffix }) => {
      const renderedValues = renderLiteral(values);
      return parityCase(`
        const values = ${renderedValues};
        const doubled = new Set(values.map((value) => value + ${extra}));
        const box = {
          base: ${suffix},
          total(...args) {
            return args.reduce((sum, value) => sum + value, this.base);
          },
        };
        ({
          spread: [${suffix}, ...values, ...doubled],
          total: box.total(...values, ...doubled),
          built: new Array(...values, ${suffix}),
        });
      `);
    }),
  fc
    .record({
      value: finiteIntegerArbitrary,
      step: finiteIntegerArbitrary,
      label: smallStringArbitrary,
    })
    .map(({ value, step, label }) =>
      parityCase(`
        const key = "value";
        const extra = [${value}];
        extra.label = ${JSON.stringify(label)};
        const obj = {
          alpha: ${step},
          [key]: ${value},
          total(amount) {
            return this.alpha + this[key] + amount;
          },
          ...null,
          ...extra,
        };
        ({ value: obj.value, zero: obj[0], label: obj.label, total: obj.total(${step}) });
      `),
    ),
  fc.constant(
    parityCase(`
      const values = [1, 2];
      values.push(3, 4);
      [
        values.pop(),
        values.slice(1, 3),
        values.join('-'),
        values.includes(2),
        values.indexOf(3),
      ];
    `),
  ),
  fc.constant(
    parityCase(`
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
    `),
  ),
  fc.constant(
    parityCase(`
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
    `),
  ),
  fc
    .record({
      entries: sortedObjectEntryArbitrary,
    })
    .map(({ entries }) => {
      const renderedEntries = renderLiteral(entries);
      const objectLiteral = `{ ${entries.map(([key, value]) => `${key}: ${value}`).join(', ')} }`;
      return parityCase(`
        const object = ${objectLiteral};
        const rebuilt = Object.fromEntries(${renderedEntries});
        ({
          keys: Object.keys(object),
          values: Object.values(object),
          entries: Object.entries(object),
          rebuilt: [rebuilt.alpha, rebuilt.beta, rebuilt.gamma, rebuilt.delta, rebuilt.omega, rebuilt.theta],
          hasOwn: [Object.hasOwn(object, 'alpha'), Object.hasOwn(object, 'missing')],
        });
      `);
    }),
  fc
    .record({
      entries: orderedObjectEntryArbitrary,
    })
    .map(({ entries }) => {
      const assignments = entries
        .map(([key, value]) => `object[${JSON.stringify(key)}] = ${renderNumberLiteral(value)};`)
        .join('\n');
      return parityCase(`
        const object = {};
        ${assignments}
        ({
          keys: Object.keys(object),
          values: Object.values(object),
          entries: Object.entries(object),
          stringified: JSON.stringify(object),
        });
      `);
    }),
  fc
    .record({
      first: finiteIntegerArbitrary,
      second: finiteIntegerArbitrary,
      leadingZero: finiteIntegerArbitrary,
      maxKey: finiteIntegerArbitrary,
      extra: wordStringArbitrary,
    })
    .map(({ first, second, leadingZero, maxKey, extra }) =>
      parityCase(`
        const values = [${first}, ${second}];
        values["01"] = ${leadingZero};
        values[4294967295] = ${maxKey};
        values[${JSON.stringify(extra)}] = ${leadingZero + maxKey};
        ({
          keys: Object.keys(values),
          entries: Object.entries(values),
          props: [values["01"], values[4294967295], values[${JSON.stringify(extra)}], values.length],
          stringified: JSON.stringify(values),
        });
      `),
    ),
];

const keyedCollectionsParityCaseArbitraries = [
  fc
    .record({
      entries: fc.array(fc.tuple(identifierArbitrary, finiteIntegerArbitrary), {
        minLength: 1,
        maxLength: 4,
      }),
      setChars: smallStringArbitrary,
    })
    .map(({ entries, setChars }) => {
      const renderedEntries = renderLiteral(entries);
      return parityCase(`
        const map = new Map(${renderedEntries});
        const set = new Set(${JSON.stringify(setChars)});
        const seen = [];
        for (const [key, value] of map) {
          seen[seen.length] = key + ':' + value;
        }
        let chars = '';
        for (const value of set.values()) {
          chars += value;
        }
        const pair = [10, 20].entries().next();
        ({
          mapSize: map.size,
          setSize: set.size,
          seen,
          chars,
          pair: [pair.value[0], pair.value[1], pair.done],
        });
      `);
    }),
  fc
    .record({
      entries: mapEntriesArbitrary,
      updateKey: mapKeyArbitrary,
      updateValue: mapValueArbitrary,
      deleteKey: mapKeyArbitrary,
      lookupKey: mapKeyArbitrary,
    })
    .map(({ entries, updateKey, updateValue, deleteKey, lookupKey }) =>
      parityCase(`
        const map = new Map(${renderLiteral(entries)});
        map.set(${renderLiteral(updateKey)}, ${renderLiteral(updateValue)});
        const beforeClear = Array.from(map.entries());
        const lookup = map.get(${renderLiteral(lookupKey)});
        const deleted = map.delete(${renderLiteral(deleteKey)});
        const afterDelete = Array.from(map.keys());
        const sizeAfterDelete = map.size;
        map.clear();
        ({ beforeClear, lookup, deleted, afterDelete, sizeAfterDelete, finalSize: map.size });
      `),
    ),
  fc
    .record({
      values: fc.array(mapKeyArbitrary, { minLength: 1, maxLength: 5 }),
      extra: mapKeyArbitrary,
      deleted: mapKeyArbitrary,
    })
    .map(({ values, extra, deleted }) =>
      parityCase(`
        const set = new Set(${renderLiteral(values)});
        set.add(${renderLiteral(extra)});
        const beforeDelete = Array.from(set.values());
        const hadDeleted = set.delete(${renderLiteral(deleted)});
        const afterDelete = Array.from(set.entries()).map((entry) => [entry[0], entry[1]]);
        const sizeAfterDelete = set.size;
        set.clear();
        ({ beforeDelete, hadDeleted, afterDelete, sizeAfterDelete, finalSize: set.size });
      `),
    ),
  fc
    .record({
      keys: fc.uniqueArray(wordStringArbitrary, {
        selector: (value) => value,
        minLength: 2,
        maxLength: 2,
      }),
    })
    .map(({ keys: [first, second] }) =>
      parityCase(`
        const functions = [];
        const seen = [];
        for (const [key, value] of new Map([
          ['${first}', 1],
          ['${second}', 2],
        ])) {
          functions[functions.length] = () => key + ':' + value;
          seen[seen.length] = key;
        }
        let letters = '';
        for (const value of '${first}${second}') {
          letters += value;
        }
        [functions[0](), functions[1](), seen, letters];
      `),
    ),
  fc
    .record({
      entries: fc.uniqueArray(fc.tuple(identifierArbitrary, finiteIntegerArbitrary), {
        selector: ([key]) => key,
        minLength: 2,
        maxLength: 4,
      }),
      injectedValue: finiteIntegerArbitrary,
    })
    .map(({ entries, injectedValue }) => {
      const [firstKey, secondKey] = entries.map(([key]) => key);
      return parityCase(`
        const map = new Map(${renderLiteral(entries)});
        const seen = [];
        for (const [key, value] of map) {
          seen[seen.length] = [key, value];
          if (key === ${JSON.stringify(firstKey)}) {
            map.set("injected", ${renderLiteral(injectedValue)});
          }
          if (key === ${JSON.stringify(secondKey)}) {
            map.delete(${JSON.stringify(firstKey)});
          }
        }
        const set = new Set(${renderLiteral(entries.map(([key]) => key))});
        const setSeen = [];
        for (const value of set) {
          setSeen[setSeen.length] = value;
          if (value === ${JSON.stringify(firstKey)}) {
            set.add("tail");
          }
          if (value === ${JSON.stringify(secondKey)}) {
            set.delete(${JSON.stringify(firstKey)});
          }
        }
        ({
          seen,
          finalMap: Array.from(map.entries()),
          setSeen,
          finalSet: Array.from(set.values()),
        });
      `);
    }),
];

const asyncPromiseParityCaseArbitraries = [
  fc
    .record({
      left: finiteIntegerArbitrary,
      right: finiteIntegerArbitrary,
      rejected: smallStringArbitrary,
    })
    .map(({ left, right, rejected }) =>
      parityCase(`
        async function main() {
          const chained = await Promise.resolve(${left})
            .then((value) => value + ${right})
            .finally(() => undefined);
          const recovered = await Promise.reject(${JSON.stringify(rejected)}).catch((reason) => {
            return reason + ':handled';
          });
          const all = await Promise.all([Promise.resolve(${left}), Promise.resolve(${right})]);
          const settled = await Promise.allSettled([
            Promise.resolve(${left}),
            Promise.reject(${JSON.stringify(rejected)}),
          ]);
          return [chained, recovered, all, settled];
        }
        main();
      `),
    ),
  fc
    .record({
      alpha: finiteIntegerArbitrary,
      beta: finiteIntegerArbitrary,
      rejected: wordStringArbitrary,
    })
    .map(({ alpha, beta, rejected }) =>
      parityCase(`
        async function main() {
          let events = [];
          const first = Promise.resolve(${alpha}).then((value) => {
            events[events.length] = 'then:' + value;
            return value + ${beta};
          });
          const second = Promise.reject(${JSON.stringify(rejected)}).catch((reason) => {
            events[events.length] = 'catch:' + reason;
            return reason + ':handled';
          });
          const settled = await Promise.allSettled([first, second]);
          return [await first, await second, settled, events];
        }
        main();
      `),
    ),
  fc
    .record({
      adopted: finiteIntegerArbitrary,
    })
    .map(({ adopted }) =>
      parityCase(`
        async function main() {
          const thenable = {};
          thenable.then = function (resolve) {
            resolve(${adopted});
          };
          try {
            await Promise.any([Promise.reject('alpha'), Promise.reject('beta')]);
            return 'unreachable';
          } catch (error) {
            return [await Promise.resolve(thenable), error.name, error.message, error.errors];
          }
        }
        main();
      `),
    ),
  fc
    .record({
      first: finiteIntegerArbitrary,
      second: finiteIntegerArbitrary,
      rejected: wordStringArbitrary,
    })
    .map(({ first, second, rejected }) =>
      parityCase(`
        async function nested(value) {
          return await Promise.resolve(await Promise.resolve(value + 1));
        }
        async function main() {
          let events = [];
          const raced = await Promise.race([
            Promise.resolve(${first}).then((value) => {
              events[events.length] = 'race:' + value;
              return value;
            }),
            Promise.resolve(${second}).then((value) => {
              events[events.length] = 'after:' + value;
              return value;
            }),
          ]);
          const recovered = await Promise.reject(${JSON.stringify(rejected)})
            .finally(() => {
              events[events.length] = 'finally';
            })
            .catch((reason) => reason + ':handled');
          return [await nested(${first}), raced, recovered, events];
        }
        main();
      `),
    ),
];

const capabilityTraceParityCaseArbitraries = [
  fc
    .record({
      input: finiteIntegerArbitrary,
      delta: finiteIntegerArbitrary,
      label: wordStringArbitrary,
    })
    .map(({ input, delta, label }) =>
      parityCase(
        `
          function main() {
            const first = fetch_data(${input});
            console.log('first', first);
            const second = probe(first + ${delta});
            console.warn(${JSON.stringify(label)}, second);
            return { first, second, label: ${JSON.stringify(label)} };
          }
          main();
        `,
        {
          capabilities: {
            fetch_data(value) {
              return value + 1;
            },
            probe(value) {
              return value * 2;
            },
          },
          console: {
            log() {
              return 'ignored';
            },
            warn() {
              return 'ignored';
            },
          },
        },
      ),
    ),
  fc
    .record({
      value: finiteIntegerArbitrary,
      reason: wordStringArbitrary,
    })
    .map(({ value, reason }) =>
      parityCase(
        `
          function main() {
            try {
              console.error('before', ${value});
              lookup(${value});
              return 'unreachable';
            } catch (error) {
              console.log('caught', error.name, error.message);
              return [error.name, error.message];
            }
          }
          main();
        `,
        {
          capabilities: {
            lookup(entry) {
              const error = new Error(`${reason}:${entry}`);
              error.name = 'CapabilityError';
              throw error;
            },
          },
          console: {
            error() {
              return 'ignored';
            },
            log() {
              return 'ignored';
            },
          },
        },
      ),
    ),
];

const SUPPORTED_PARITY_FAMILIES = Object.freeze([
  {
    id: 'core-language',
    title: 'core language and builtins',
    mode: 'differential',
    numRuns: process.env.CI ? 40 : 20,
    arbitrary: fc.oneof(...coreParityCaseArbitraries),
  },
  {
    id: 'control-flow',
    title: 'control flow',
    mode: 'differential',
    numRuns: process.env.CI ? 40 : 20,
    arbitrary: fc.oneof(...controlFlowParityCaseArbitraries),
  },
  {
    id: 'exceptions',
    title: 'exceptions',
    mode: 'differential',
    numRuns: process.env.CI ? 32 : 16,
    arbitrary: fc.oneof(...exceptionParityCaseArbitraries),
  },
  {
    id: 'objects-arrays',
    title: 'objects and arrays',
    mode: 'differential',
    numRuns: process.env.CI ? 40 : 20,
    arbitrary: fc.oneof(...objectsArraysParityCaseArbitraries),
  },
  {
    id: 'keyed-collections',
    title: 'keyed collections',
    mode: 'differential',
    numRuns: process.env.CI ? 36 : 18,
    arbitrary: fc.oneof(...keyedCollectionsParityCaseArbitraries),
  },
  {
    id: 'async-promises',
    title: 'async promises',
    mode: 'differential',
    numRuns: process.env.CI ? 32 : 16,
    arbitrary: fc.oneof(...asyncPromiseParityCaseArbitraries),
  },
  {
    id: 'capability-traces',
    title: 'capability traces',
    mode: 'progress-trace',
    numRuns: process.env.CI ? 24 : 12,
    arbitrary: fc.oneof(...capabilityTraceParityCaseArbitraries),
  },
]);

const supportedProgramArbitrary = fc.oneof(
  ...SUPPORTED_PARITY_FAMILIES.filter((family) => family.mode === 'differential').map((family) =>
    family.arbitrary.map((entry) => entry.source),
  ),
);

const contractRuntimeCaseArbitraries = RUNTIME_REJECT_CASES.map((entry) => fc.constant(entry));

function contractCaseArbitrariesFor(cases, category) {
  return cases
    .filter((entry) => entry.category === category)
    .map((entry) => fc.constant(entry));
}

const unsupportedSyntaxValidationCaseArbitraries = [
  identifierArbitrary.map((name) => ({
    source: `function ${name}(value = 1) { return value; }`,
    messageIncludes: 'default parameters are not supported in v1',
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  })),
  identifierArbitrary.map((name) => ({
    source: `const { ${name} = 1 } = {};`,
    messageIncludes: 'default destructuring is not supported in v1',
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  })),
  ...contractCaseArbitrariesFor(VALIDATION_REJECT_CASES, DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX),
];

const ambientGlobalValidationCaseArbitraries = [
  fc.constant(
    validationCase(
      'function wrap() { return arguments[0]; }',
      'forbidden ambient global `arguments`',
      DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
    ),
  ),
  fc.constant(
    validationCase(
      'eval("1");',
      'forbidden ambient global `eval`',
      DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
    ),
  ),
  fc.constant(
    validationCase(
      'Function("return 1");',
      'forbidden ambient global `Function`',
      DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
    ),
  ),
  fc.constantFrom(...FORBIDDEN_AMBIENT_GLOBALS).map((name) =>
    validationCase(
      `${name};`,
      `forbidden ambient global \`${name}\``,
      DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
    ),
  ),
  ...contractCaseArbitrariesFor(VALIDATION_REJECT_CASES, DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL),
];

const unsupportedBindingValidationCaseArbitraries = [
  fc.constant(
    validationCase(
      'for (let value = 1 of [1, 2]) { value; }',
      'for...of binding initializers are not supported',
      DIAGNOSTIC_CATEGORY.UNSUPPORTED_BINDING,
    ),
  ),
  ...contractCaseArbitrariesFor(VALIDATION_REJECT_CASES, DIAGNOSTIC_CATEGORY.UNSUPPORTED_BINDING),
];

const unsupportedOperatorValidationCaseArbitraries = contractCaseArbitrariesFor(
  VALIDATION_REJECT_CASES,
  DIAGNOSTIC_CATEGORY.UNSUPPORTED_OPERATOR,
);

const unsupportedValidationCaseArbitraries = [
  ...unsupportedSyntaxValidationCaseArbitraries,
  ...ambientGlobalValidationCaseArbitraries,
  ...unsupportedBindingValidationCaseArbitraries,
  ...unsupportedOperatorValidationCaseArbitraries,
];

const REJECTION_FAMILIES = Object.freeze([
  {
    id: 'constructor-unsupported-syntax',
    title: 'constructor-time unsupported syntax',
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
    numRuns: process.env.CI ? 40 : 20,
    arbitrary: fc.oneof(...unsupportedSyntaxValidationCaseArbitraries),
  },
  {
    id: 'constructor-ambient-globals',
    title: 'constructor-time ambient globals',
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
    numRuns: process.env.CI ? 32 : 16,
    arbitrary: fc.oneof(...ambientGlobalValidationCaseArbitraries),
  },
  {
    id: 'constructor-unsupported-bindings',
    title: 'constructor-time unsupported bindings',
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_BINDING,
    numRuns: process.env.CI ? 24 : 12,
    arbitrary: fc.oneof(...unsupportedBindingValidationCaseArbitraries),
  },
  {
    id: 'constructor-unsupported-operators',
    title: 'constructor-time unsupported operators',
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_OPERATOR,
    numRuns: process.env.CI ? 24 : 12,
    arbitrary: fc.oneof(...unsupportedOperatorValidationCaseArbitraries),
  },
  {
    id: 'runtime-unsupported-surface',
    title: 'runtime unsupported surface',
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_RUNTIME_SURFACE,
    numRuns: process.env.CI ? 24 : 12,
    arbitrary: fc.oneof(
      ...contractCaseArbitrariesFor(
        RUNTIME_REJECT_CASES,
        DIAGNOSTIC_CATEGORY.UNSUPPORTED_RUNTIME_SURFACE,
      ),
    ),
  },
  {
    id: 'runtime-unsupported-global-builtin',
    title: 'runtime unsupported global builtins',
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_GLOBAL_BUILTIN,
    numRuns: process.env.CI ? 24 : 12,
    arbitrary: fc.oneof(
      ...contractCaseArbitrariesFor(
        RUNTIME_REJECT_CASES,
        DIAGNOSTIC_CATEGORY.UNSUPPORTED_GLOBAL_BUILTIN,
      ),
    ),
  },
]);

const unsupportedValidationCaseArbitrary = fc.oneof(...unsupportedValidationCaseArbitraries);
const unsupportedRuntimeCaseArbitrary = fc.oneof(...contractRuntimeCaseArbitraries);
const conformanceCaseArbitrary = fc.oneof(
  supportedProgramArbitrary.map((source) => ({ source })),
  unsupportedValidationCaseArbitrary,
);

const holeMarker = Symbol('structured-hole');

const structuredValueArbitrary = fc.letrec((tie) => ({
  array: fc
    .array(fc.oneof(tie('value'), fc.constant(holeMarker)), { maxLength: 3 })
    .map((entries) => {
      const array = new Array(entries.length);
      entries.forEach((entry, index) => {
        if (entry !== holeMarker) {
          array[index] = entry;
        }
      });
      return array;
    }),
  value: fc.oneof(
    fc.constant(undefined),
    fc.constant(null),
    fc.boolean(),
    supportedNumberArbitrary,
    smallStringArbitrary,
    tie('array'),
    fc.dictionary(identifierArbitrary, tie('value'), { maxKeys: 3 }),
  ),
})).value;

const unsupportedHostValueCaseArbitrary = fc.constantFrom(
  {
    value: () => 1,
    messageIncludes: 'Unsupported host value',
  },
  {
    value: Symbol('edge'),
    messageIncludes: 'Unsupported host value',
  },
  {
    value: 1n,
    messageIncludes: 'Unsupported host value',
  },
  {
    value: new Map([['alpha', 1]]),
    messageIncludes: 'only plain objects and arrays can cross the host boundary',
  },
  {
    value: new Set([1, 2]),
    messageIncludes: 'only plain objects and arrays can cross the host boundary',
  },
  {
    value: new Date(0),
    messageIncludes: 'only plain objects and arrays can cross the host boundary',
  },
  {
    value: Object.create({ inherited: true }),
    messageIncludes: 'only plain objects and arrays can cross the host boundary',
  },
  {
    value: new (class Box {
      constructor() {
        this.value = 1;
      }
    })(),
    messageIncludes: 'only plain objects and arrays can cross the host boundary',
  },
);

const progressActionArbitrary = fc.constantFrom('resume', 'resumeError', 'cancel');

module.exports = {
  PROPERTY_RUNS,
  REJECTION_FAMILIES,
  SUPPORTED_PARITY_FAMILIES,
  conformanceCaseArbitrary,
  fc,
  progressActionArbitrary,
  structuredValueArbitrary,
  supportedProgramArbitrary,
  unsupportedHostValueCaseArbitrary,
  unsupportedRuntimeCaseArbitrary,
  unsupportedValidationCaseArbitrary,
};
