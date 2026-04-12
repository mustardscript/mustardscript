'use strict';

const fc = require('fast-check');

const PROPERTY_RUNS = process.env.CI ? 100 : 50;
const IDENTIFIERS = ['alpha', 'beta', 'gamma', 'delta', 'omega'];
const ASCII_CHARS = ['a', 'b', 'c', 'd', 'e', 'x', 'y', 'z', ' ', '-', ',', '_'];

const identifierArbitrary = fc.constantFrom(...IDENTIFIERS);
const smallStringArbitrary = fc
  .array(fc.constantFrom(...ASCII_CHARS), { maxLength: 6 })
  .map((chars) => chars.join(''));
const finiteIntegerArbitrary = fc.integer({ min: -6, max: 6 });
const numericEdgeCaseArbitrary = fc.constantFrom(-0, NaN, Infinity, -Infinity);
const supportedNumberArbitrary = fc.oneof(finiteIntegerArbitrary, numericEdgeCaseArbitrary);

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

const supportedProgramArbitrary = fc.oneof(
  fc
    .record({
      left: supportedNumberArbitrary,
      right: supportedNumberArbitrary,
      factor: finiteIntegerArbitrary,
    })
    .map(
      ({ left, right, factor }) => `
        const left = ${renderNumberLiteral(left)};
        const right = ${renderNumberLiteral(right)};
        const factor = ${renderNumberLiteral(factor)};
        [left + right, left - right, right * factor];
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
        function outer(delta) {
          let captured = base + ${bonus};
          function inner(value) {
            return captured + delta + value;
          }
          return inner(${value});
        }
        ({ result: outer(${delta}), base });
      `,
    ),
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
      ({
        sequence: (steps = steps + 1, steps = steps + ${delta}, steps),
        exponent: base ** ${exponent},
      });
    `),
);

const unsupportedValidationCaseArbitrary = fc.oneof(
  identifierArbitrary.map((name) => ({
    source: `function ${name}(value = 1) { return value; }`,
    messageIncludes: 'default parameter',
  })),
  identifierArbitrary.map((name) => ({
    source: `const { ${name} = 1 } = {};`,
    messageIncludes: 'default destructuring',
  })),
  fc.constant({
    source: 'function wrap() { return arguments[0]; }',
    messageIncludes: 'arguments',
  }),
  fc.constant({
    source: 'for (const key in { alpha: 1 }) { key; }',
    messageIncludes: 'for...in',
  }),
  fc.constant({
    source: 'class Example {}',
    messageIncludes: 'class',
  }),
  fc.constant({
    source: 'function* make() { yield 1; }',
    messageIncludes: 'generator',
  }),
  fc.constant({
    source: '({ ...value });',
    messageIncludes: 'object spread',
  }),
  fc.constant({
    source: '[...value];',
    messageIncludes: 'array spread',
  }),
  fc.constant({
    source: '[1, , 2];',
    messageIncludes: 'array hole',
  }),
  fc.constant({
    source: 'let value = 2; value **= 3;',
    messageIncludes: 'assignment operator',
  }),
  fc.constant({
    source: 'label: 1;',
    messageIncludes: 'label',
  }),
  fc.constant({
    source: 'debugger;',
    messageIncludes: 'debugger',
  }),
  fc.constant({
    source: 'with ({ alpha: 1 }) { alpha; }',
    messageIncludes: 'with',
  }),
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
const conformanceCaseArbitrary = fc.oneof(
  supportedProgramArbitrary.map((source) => ({ source })),
  unsupportedValidationCaseArbitrary,
);

module.exports = {
  conformanceCaseArbitrary,
  PROPERTY_RUNS,
  fc,
  progressActionArbitrary,
  structuredValueArbitrary,
  supportedProgramArbitrary,
  unsupportedHostValueCaseArbitrary,
  unsupportedValidationCaseArbitrary,
};
