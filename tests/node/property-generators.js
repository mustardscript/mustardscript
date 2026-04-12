'use strict';

const fc = require('fast-check');
const {
  FORBIDDEN_AMBIENT_GLOBALS,
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

function validationCase(source, messageIncludes) {
  return { source, messageIncludes };
}

const supportedProgramArbitraries = [
  fc
    .record({
      left: supportedNumberArbitrary,
      right: supportedNumberArbitrary,
      divisor: positiveFiniteIntegerArbitrary,
      fallback: finiteIntegerArbitrary,
    })
    .map(
      ({ left, right, divisor, fallback }) => `
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
      `,
    ),
  fc
    .record({
      base: finiteIntegerArbitrary,
      delta: finiteIntegerArbitrary,
      value: finiteIntegerArbitrary,
      bonus: finiteIntegerArbitrary,
    })
    .map(
      ({ base, delta, value, bonus }) => `
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
      `,
    ),
  fc
    .record({
      limit: positiveFiniteIntegerArbitrary,
      continueOn: finiteIntegerArbitrary,
      branchOn: finiteIntegerArbitrary,
    })
    .map(({ limit, continueOn, branchOn }) => `
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
  fc
    .record({
      left: finiteIntegerArbitrary,
      right: finiteIntegerArbitrary,
      fallback: finiteIntegerArbitrary,
      missing: finiteIntegerArbitrary,
    })
    .map(({ left, right, fallback, missing }) => `
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
  fc
    .record({
      values: fc.array(finiteIntegerArbitrary, { minLength: 1, maxLength: 4 }),
      extra: finiteIntegerArbitrary,
      threshold: finiteIntegerArbitrary,
      seed: finiteIntegerArbitrary,
    })
    .map(({ values, extra, threshold, seed }) => {
      const renderedValues = `[${values.join(', ')}]`;
      return `
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
      `;
    }),
  fc
    .record({
      values: smallIntegerArrayArbitrary,
      offset: finiteIntegerArbitrary,
    })
    .map(({ values, offset }) => {
      const renderedValues = renderLiteral(values);
      return `
        const values = ${renderedValues};
        const iterated = Array.from(values.entries()).map((entry) => [entry[0], entry[1]]);
        const fromSet = Array.from(new Set(values), function (value, index) {
          return value + index + this.offset;
        }, { offset: ${offset} });
        const sorted = values.slice().sort((left, right) => left - right);
        ({ iterated, fromSet, sorted });
      `;
    }),
  fc
    .record({
      entries: fc.array(
        fc.tuple(identifierArbitrary, finiteIntegerArbitrary),
        { minLength: 1, maxLength: 4 },
      ),
      setChars: smallStringArbitrary,
    })
    .map(({ entries, setChars }) => {
      const renderedEntries = renderLiteral(entries);
      return `
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
      `;
    }),
  fc
    .record({
      raw: smallStringArbitrary,
      needle: fc.constantFrom('a', 'b', ' ', '-', ','),
      replacement: fc.constantFrom('x', 'y', 'z'),
      start: fc.integer({ min: -3, max: 3 }),
      end: fc.integer({ min: -3, max: 6 }),
      limit: fc.integer({ min: 0, max: 4 }),
    })
    .map(({ raw, needle, replacement, start, end, limit }) => `
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
  fc
    .record({
      first: wordStringArbitrary,
      second: wordStringArbitrary,
      replacement: fc.constantFrom('x', 'y', 'z'),
    })
    .map(({ first, second, replacement }) => `
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
          (match, letters, digits, offset) => letters.toUpperCase() + ${JSON.stringify(replacement)} + digits + ':' + offset,
        ),
        tested,
      });
    `),
  fc
    .record({
      entries: sortedObjectEntryArbitrary,
    })
    .map(({ entries }) => {
      const renderedEntries = renderLiteral(entries);
      const objectLiteral = `{ ${entries
        .map(([key, value]) => `${key}: ${value}`)
        .join(', ')} }`;
      return `
        const object = ${objectLiteral};
        const rebuilt = Object.fromEntries(${renderedEntries});
        ({
          keys: Object.keys(object),
          values: Object.values(object),
          entries: Object.entries(object),
          rebuilt: [rebuilt.alpha, rebuilt.beta, rebuilt.gamma, rebuilt.delta, rebuilt.omega, rebuilt.theta],
          hasOwn: [Object.hasOwn(object, 'alpha'), Object.hasOwn(object, 'missing')],
        });
      `;
    }),
  fc
    .record({
      entries: orderedObjectEntryArbitrary,
    })
    .map(({ entries }) => {
      const assignments = entries
        .map(
          ([key, value]) => `object[${JSON.stringify(key)}] = ${renderNumberLiteral(value)};`,
        )
        .join('\n');
      return `
        const object = {};
        ${assignments}
        ({
          keys: Object.keys(object),
          values: Object.values(object),
          entries: Object.entries(object),
          stringified: JSON.stringify(object),
        });
      `;
    }),
  fc
    .record({
      first: finiteIntegerArbitrary,
      second: finiteIntegerArbitrary,
      leadingZero: finiteIntegerArbitrary,
      maxKey: finiteIntegerArbitrary,
      extra: wordStringArbitrary,
    })
    .map(({ first, second, leadingZero, maxKey, extra }) => `
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
  fc
    .record({
      entries: mapEntriesArbitrary,
      updateKey: mapKeyArbitrary,
      updateValue: mapValueArbitrary,
      deleteKey: mapKeyArbitrary,
      lookupKey: mapKeyArbitrary,
    })
    .map(({ entries, updateKey, updateValue, deleteKey, lookupKey }) => `
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
  fc
    .record({
      values: fc.array(mapKeyArbitrary, { minLength: 1, maxLength: 5 }),
      extra: mapKeyArbitrary,
      deleted: mapKeyArbitrary,
    })
    .map(({ values, extra, deleted }) => `
      const set = new Set(${renderLiteral(values)});
      set.add(${renderLiteral(extra)});
      const beforeDelete = Array.from(set.values());
      const hadDeleted = set.delete(${renderLiteral(deleted)});
      const afterDelete = Array.from(set.entries()).map((entry) => [entry[0], entry[1]]);
      const sizeAfterDelete = set.size;
      set.clear();
      ({ beforeDelete, hadDeleted, afterDelete, sizeAfterDelete, finalSize: set.size });
    `),
  fc
    .record({
      first: wordStringArbitrary,
      second: wordStringArbitrary,
    })
    .map(({ first, second }) => `
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
  fc
    .record({
      left: finiteIntegerArbitrary,
      right: finiteIntegerArbitrary,
      rejected: smallStringArbitrary,
    })
    .map(({ left, right, rejected }) => `
      async function main() {
        const chained = await Promise.resolve(${left})
          .then((value) => value + ${right})
          .finally(() => undefined);
        const recovered = await Promise.reject(${JSON.stringify(rejected)}).catch((reason) => {
          return reason + ':handled';
        });
        const all = await Promise.all([Promise.resolve(${left}), Promise.resolve(${right})]);
        const settled = await Promise.allSettled([Promise.resolve(${left}), Promise.reject(${JSON.stringify(rejected)})]);
        return [chained, recovered, all, settled];
      }
      main();
    `),
  fc
    .record({
      left: fc.integer({ min: -12, max: 12 }),
      right: fc.integer({ min: 1, max: 12 }),
      extra: fc.integer({ min: -12, max: 12 }),
    })
    .map(({ left, right, extra }) => `
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
  fc
    .record({
      base: finiteIntegerArbitrary,
      delta: finiteIntegerArbitrary,
      exponent: fc.integer({ min: 0, max: 5 }),
    })
    .map(({ base, delta, exponent }) => `
      let steps = 0;
      const base = ${base};
      const value = (steps = steps + 1, steps = steps + ${delta}, base ** ${exponent});
      ({ value, steps });
    `),
  fc
    .record({
      value: finiteIntegerArbitrary,
      step: finiteIntegerArbitrary,
      label: smallStringArbitrary,
    })
    .map(({ value, step, label }) => `
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
  fc
    .record({
      message: wordStringArbitrary,
    })
    .map(({ message }) => `
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
  fc
    .record({
      first: fc.integer({ min: -1_000, max: 1_000 }),
      second: fc.integer({ min: -1_000, max: 1_000 }),
    })
    .map(({ first, second }) => `
      const initial = new Date(${first});
      const cloned = new Date(initial);
      const other = new Date(${second});
      [initial.getTime(), cloned.getTime(), other.getTime()];
    `),
  fc
    .record({
      alpha: finiteIntegerArbitrary,
      beta: finiteIntegerArbitrary,
      rejected: wordStringArbitrary,
    })
    .map(({ alpha, beta, rejected }) => `
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
  fc
    .record({
      adopted: finiteIntegerArbitrary,
    })
    .map(({ adopted }) => `
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
  fc
    .record({
      alpha: finiteIntegerArbitrary,
      beta: finiteIntegerArbitrary,
    })
    .map(({ alpha, beta }) => `
      const payload = JSON.parse(${JSON.stringify(`{"alpha":${alpha},"beta":[${beta}]}`)});
      \`payload=\${payload.alpha}-\${payload.beta[0]}\`;
    `),
];

const supportedProgramArbitrary = fc.oneof(...supportedProgramArbitraries);

const contractValidationCaseArbitraries = VALIDATION_REJECT_CASES.map(({ source, messageIncludes }) =>
  fc.constant({ source, messageIncludes }),
);

const unsupportedValidationCaseArbitraries = [
  identifierArbitrary.map((name) => ({
    source: `function ${name}(value = 1) { return value; }`,
    messageIncludes: 'default parameters are not supported in v1',
  })),
  identifierArbitrary.map((name) => ({
    source: `const { ${name} = 1 } = {};`,
    messageIncludes: 'default destructuring is not supported in v1',
  })),
  fc.constant(validationCase('function wrap() { return arguments[0]; }', 'forbidden ambient global `arguments`')),
  fc.constant(validationCase('eval("1");', 'forbidden ambient global `eval`')),
  fc.constant(validationCase('Function("return 1");', 'forbidden ambient global `Function`')),
  fc.constantFrom(...FORBIDDEN_AMBIENT_GLOBALS).map((name) =>
    validationCase(`${name};`, `forbidden ambient global \`${name}\``),
  ),
  fc.constant(validationCase('for (let value = 1 of [1, 2]) { value; }', 'for...of binding initializers are not supported')),
  fc.constant(validationCase('[1, , 2];', 'array holes are not supported in v1')),
  ...contractValidationCaseArbitraries,
];

const unsupportedValidationCaseArbitrary = fc.oneof(...unsupportedValidationCaseArbitraries);
const conformanceCaseArbitrary = fc.oneof(
  supportedProgramArbitrary.map((source) => ({ source })),
  unsupportedValidationCaseArbitrary,
);

const structuredValueArbitrary = fc.letrec((tie) => ({
  value: fc.oneof(
    fc.constant(undefined),
    fc.constant(null),
    fc.boolean(),
    supportedNumberArbitrary,
    smallStringArbitrary,
    fc.array(tie('value'), { maxLength: 3 }),
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
  conformanceCaseArbitrary,
  fc,
  progressActionArbitrary,
  structuredValueArbitrary,
  supportedProgramArbitrary,
  unsupportedHostValueCaseArbitrary,
  unsupportedValidationCaseArbitrary,
};
