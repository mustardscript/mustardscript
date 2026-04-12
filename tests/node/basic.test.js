const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError, Progress } = require('../../index.js');

function assertGuestSafeMessage(message) {
  assert.ok(!message.includes(process.cwd()));
  assert.ok(!message.includes('crates/jslite'));
  assert.ok(!message.includes('.rs'));
}

test('run executes sync programs', async () => {
  const j = new Jslite(`
    const values = [1, 2, 3];
    values[0] + values[2];
  `);

  const result = await j.run();
  assert.equal(result, 4);
});

test('run supports conservative array, string, object, and Math helper surface', async () => {
  const j = new Jslite(`
    const values = [1, 2];
    values.push(3);
    const object = { zebra: 1, alpha: 2 };
    const arrayEntries = Object.entries(values);
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
      arrayEntries: arrayEntries,
      pow: Math.pow(2, 5),
      sqrt: Math.sqrt(81),
      trunc: Math.trunc(-3.9),
      sign: Math.sign(-0),
    });
  `);

  const result = await j.run();
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
    objectKeys: ['alpha', 'zebra'],
    objectValues: [2, 1],
    hasOwn: true,
    arrayEntries: [['0', 1], ['1', 2], ['2', 3]],
    pow: 32,
    sqrt: 9,
    trunc: -3,
    sign: -0,
  });
  assert.ok(Object.is(result.sign, -0));
});

test('run supports callback-heavy array helpers and thisArg for supported callbacks', async () => {
  const j = new Jslite(`
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
  `);

  const result = await j.run();
  assert.deepEqual(result, [[5, 7, 9], [1, 3], 3, 2, true, true, 11, 36]);
});

test('array callback helpers fail closed for invalid callbacks and synchronous host suspensions', async () => {
  await assert.rejects(
    () => new Jslite('([1]).map(1);').run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('Array.prototype.map expects a callable callback'),
  );

  await assert.rejects(
    () =>
      new Jslite('[1].map(fetch_data);').run({
        capabilities: {
          fetch_data(value) {
            return value + 1;
          },
        },
      }),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('array callback helpers do not support synchronous host suspensions'),
  );

  await assert.rejects(
    () => new Jslite('[].reduce((acc, value) => acc + value);').run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('Array.prototype.reduce requires an initial value for empty arrays'),
  );
});

test('run supports RegExp helpers, regex string patterns, and callback replacements', async () => {
  const j = new Jslite(`
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
  `);

  const result = await j.run();
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
    () => new Jslite('new RegExp("a", "dg");').run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('unsupported regular expression flag `d`'),
  );

  await assert.rejects(
    () => new Jslite('"abc".replaceAll(/a/, "z");').run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('String.prototype.replaceAll requires a global RegExp'),
  );

  await assert.rejects(
    () =>
      new Jslite('"abc".replace("a", fetch_data);').run({
        capabilities: {
          fetch_data(value) {
            return value;
          },
        },
      }),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes(
        'String.prototype.replace callback replacements do not support host suspensions',
      ),
  );
});

test('run applies nullish assignment only to nullish identifiers and members', async () => {
  const j = new Jslite(`
    let missing;
    missing ??= 7;
    const box = { present: 5, absent: undefined };
    box.present ??= 9;
    box.absent ??= 11;
    const key = 'dynamic';
    box[key] ??= 13;
    [missing, box.present, box.absent, box.dynamic];
  `);

  const result = await j.run();
  assert.deepEqual(result, [7, 5, 11, 13]);
});

test('run binds member-call receivers for guest functions', async () => {
  const j = new Jslite(`
    const method = function (delta) {
      return this.base + delta;
    };
    const obj = { base: 40, method: method };
    obj.method(2);
  `);

  const result = await j.run();
  assert.equal(result, 42);
});

test('run binds rest parameters for functions and arrows', async () => {
  const j = new Jslite(`
    function collect(head, ...tail) {
      return [head, tail.length, tail[0], tail[1]];
    }
    const sumFirstTwo = (...[first, second]) => first + second;
    [collect(1, 2, 3), sumFirstTwo(4, 5, 6)];
  `);

  const result = await j.run();
  assert.deepEqual(result, [[1, 2, 2, 3], 9]);
});

test('run exposes structured inputs with preserved numeric edge cases', async () => {
  const j = new Jslite(`
    ({ value, inf, negZero, nan });
  `);

  const result = await j.run({
    inputs: {
      value: 7,
      inf: Infinity,
      negZero: -0,
      nan: NaN,
    },
  });

  assert.equal(result.value, 7);
  assert.equal(result.inf, Infinity);
  assert.ok(Object.is(result.negZero, -0));
  assert.ok(Number.isNaN(result.nan));
});

test('run drives host capabilities', async () => {
  const j = new Jslite(`
    const response = fetch_data(9);
    response + 1;
  `);

  const result = await j.run({
    capabilities: {
      fetch_data(value) {
        return value;
      },
    },
  });

  assert.equal(result, 10);
});

test('run awaits async host capabilities', async () => {
  const j = new Jslite(`
    const response = fetch_data(5);
    response * 3;
  `);

  const result = await j.run({
    capabilities: {
      async fetch_data(value) {
        return Promise.resolve(value);
      },
    },
  });

  assert.equal(result, 15);
});

test('run routes deterministic console callbacks and ignores host return values', async () => {
  const events = [];
  const j = new Jslite(`
    const first = console.log('alpha', 1);
    const second = console.warn({ ok: true });
    const third = console.error('omega');
    [first, second, third];
  `);

  const result = await j.run({
    console: {
      log(...args) {
        events.push(['log', args]);
        return 'ignored';
      },
      warn(...args) {
        events.push(['warn', args]);
        return 42;
      },
      error(...args) {
        events.push(['error', args]);
        return { ignored: true };
      },
    },
  });

  assert.deepEqual(events, [
    ['log', ['alpha', 1]],
    ['warn', [{ ok: true }]],
    ['error', ['omega']],
  ]);
  assert.deepEqual(result, [undefined, undefined, undefined]);
});

test('start exposes console callbacks as suspension points with undefined guest results', () => {
  const j = new Jslite(`
    const logged = console.log('alpha');
    logged === undefined ? 2 : 0;
  `);

  const progress = j.start({
    console: {
      log() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'console.log');
  assert.deepEqual(progress.args, ['alpha']);
  assert.equal(progress.resume('ignored by guest semantics'), 2);
});

test('console methods fail guest-safely when callbacks are not registered', async () => {
  const j = new Jslite(`
    console.log('alpha');
  `);

  await assert.rejects(
    j.run(),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      /value is not callable/.test(error.message),
  );
});

test('run surfaces sanitized host capability errors', async () => {
  const j = new Jslite(`
    fetch_data(1);
  `);

  await assert.rejects(
    j.run({
      capabilities: {
        fetch_data() {
          const error = new Error('upstream failed');
          error.name = 'CapabilityError';
          error.code = 'E_UPSTREAM';
          error.details = { retriable: false };
          throw error;
        },
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      /CapabilityError: upstream failed \[code=E_UPSTREAM\]/.test(error.message),
  );
});

test('run executes throw, try/catch, finally, and Error constructors', async () => {
  const j = new Jslite(`
    let log = [];
    try {
      log[log.length] = 'body';
      throw new TypeError('boom');
    } catch (error) {
      log[log.length] = error.name;
      log[log.length] = error.message;
    } finally {
      log[log.length] = 'finally';
    }
    log;
  `);

  const result = await j.run();
  assert.deepEqual(result, ['body', 'TypeError', 'boom', 'finally']);
});

test('run catches runtime failures as guest-visible errors', async () => {
  const j = new Jslite(`
    let captured;
    try {
      const value = null;
      value.answer;
    } catch (error) {
      captured = [error.name, error.message];
    }
    captured;
  `);

  const result = await j.run();
  assert.deepEqual(result, [
    'TypeError',
    'cannot read properties of nullish value',
  ]);
});

test('run catches resumed host capability errors inside guest try/catch', async () => {
  const j = new Jslite(`
    let captured;
    try {
      fetch_data(1);
    } catch (error) {
      captured = [error.name, error.message, error.code, error.details.status];
    }
    captured;
  `);

  const result = await j.run({
    capabilities: {
      async fetch_data() {
        const error = new Error('upstream failed');
        error.name = 'CapabilityError';
        error.code = 'E_UPSTREAM';
        error.details = { status: 503 };
        throw error;
      },
    },
  });

  assert.deepEqual(result, [
    'CapabilityError',
    'upstream failed',
    'E_UPSTREAM',
    503,
  ]);
});

test('finally runs for return, break, and continue completions', async () => {
  const j = new Jslite(`
    let events = [];
    function earlyReturn() {
      try {
        return 'body';
      } finally {
        events[events.length] = 'return';
      }
    }
    let index = 0;
    while (index < 2) {
      index += 1;
      try {
        if (index === 1) {
          continue;
        }
        break;
      } finally {
        events[events.length] = index;
      }
    }
    [earlyReturn(), events];
  `);

  const result = await j.run();
  assert.deepEqual(result, ['body', [1, 2, 'return']]);
});

test('nested exception unwinds preserve catch and finally ordering', async () => {
  const j = new Jslite(`
    let events = [];
    function nested() {
      try {
        try {
          events[events.length] = 'inner-body';
          throw new Error('boom');
        } catch (error) {
          events[events.length] = error.message;
          throw new TypeError('wrapped');
        } finally {
          events[events.length] = 'inner-finally';
        }
      } catch (error) {
        events[events.length] = error.name;
      } finally {
        events[events.length] = 'outer-finally';
      }
      return events;
    }
    nested();
  `);

  const result = await j.run();
  assert.deepEqual(result, [
    'inner-body',
    'boom',
    'inner-finally',
    'TypeError',
    'outer-finally',
  ]);
});

test('constructor converts native parse failures into typed errors', () => {
  assert.throws(
    () => new Jslite('const value = ;'),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteParseError' &&
      error.kind === 'Parse' &&
      error.message.length > 0 &&
      (assertGuestSafeMessage(error.message), true),
  );
});

test('constructor converts native validation failures into typed errors', () => {
  assert.throws(
    () => new Jslite('export const value = 1;'),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteValidationError' &&
      error.kind === 'Validation' &&
      /module syntax is not supported/.test(error.message),
  );
});

test('constructor rejects unsupported default params, destructuring defaults, and free arguments', () => {
  assert.throws(
    () => new Jslite('function wrap(value = 1) { return value; }'),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteValidationError' &&
      error.kind === 'Validation' &&
      /default parameters are not supported/.test(error.message),
  );

  assert.throws(
    () => new Jslite('const { value = 1 } = {};'),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteValidationError' &&
      error.kind === 'Validation' &&
      /default destructuring is not supported/.test(error.message),
  );

  assert.throws(
    () => new Jslite('function wrap() { return arguments[0]; }'),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteValidationError' &&
      error.kind === 'Validation' &&
      /forbidden ambient global `arguments`/.test(error.message),
  );
});

test('capability calls reject guest functions across the host boundary', async () => {
  const j = new Jslite(`
    fetch_data(() => 1);
  `);

  await assert.rejects(
    j.run({
      capabilities: {
        fetch_data() {
          return 1;
        },
      },
    }),
    /functions cannot cross the structured host boundary/,
  );
});

test('run surfaces limit failures as typed errors', async () => {
  const j = new Jslite('while (true) {}');
  await assert.rejects(
    j.run({
      limits: {
        instructionBudget: 100,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      /instruction budget exhausted/.test(error.message),
  );
});

test('run surfaces heap and allocation limit failures as typed errors', async () => {
  const j = new Jslite('1;');

  await assert.rejects(
    j.run({
      limits: {
        heapLimitBytes: 1,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      /heap limit exceeded/.test(error.message),
  );

  await assert.rejects(
    j.run({
      limits: {
        allocationBudget: 1,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      /allocation budget exhausted/.test(error.message),
  );
});

test('run surfaces call-depth limit failures as typed errors', async () => {
  const j = new Jslite(`
    function recurse(value) {
      if (value === 0) {
        return 0;
      }
      return recurse(value - 1);
    }
    recurse(3);
  `);

  await assert.rejects(
    j.run({
      limits: {
        callDepthLimit: 3,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      /call depth limit exceeded/.test(error.message),
  );
});

test('start returns resumable progress objects', () => {
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'fetch_data');
  assert.deepEqual(progress.args, [4]);

  const finalValue = progress.resume(4);
  assert.equal(finalValue, 8);
});

test('progress objects are single-use', () => {
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.equal(progress.resume(4), 8);
  assert.throws(
    () => progress.resume(4),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      /single-use/.test(error.message),
  );
});

test('progress dump and load preserve suspended execution state', () => {
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      fetch_data() {},
    },
  });

  const restored = Progress.load(progress.dump());
  assert.ok(restored instanceof Progress);
  assert.equal(restored.capability, 'fetch_data');
  assert.deepEqual(restored.args, [4]);
  assert.equal(restored.resume(4), 8);
});

test('start snapshots guest state before async host futures exist', () => {
  let calls = 0;
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      async fetch_data() {
        calls += 1;
        return 4;
      },
    },
  });

  assert.equal(calls, 0);
  const dumped = progress.dump();
  const restored = Progress.load(dumped);
  assert.equal(restored.resume(4), 8);
});

test('progress.load rejects reused snapshots in the same process', () => {
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      fetch_data() {},
    },
  });
  const dumped = progress.dump();
  assert.equal(progress.resume(4), 8);

  const restored = Progress.load(dumped);
  assert.throws(
    () => restored.resume(4),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      /single-use/.test(error.message),
  );
});

test('progress load surfaces snapshot failures as typed errors', () => {
  assert.throws(
    () =>
      Progress.load(
        {
          snapshot: Buffer.from('not-a-valid-snapshot'),
        },
        {
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        },
      ),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteSerializationError' &&
      error.kind === 'Serialization',
  );
});

test('runtime errors do not leak host internals in guest tracebacks', async () => {
  const j = new Jslite(`
    function outer() {
      const value = null;
      return value.answer;
    }
    outer();
  `);

  await assert.rejects(
    j.run(),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      (assertGuestSafeMessage(error.message), true),
  );
});

test('limit errors do not leak host internals', async () => {
  const runaway = new Jslite('while (true) {}');
  await assert.rejects(
    runaway.run({
      limits: {
        instructionBudget: 100,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      (assertGuestSafeMessage(error.message), true),
  );

  const tinyHeap = new Jslite('1;');
  await assert.rejects(
    tinyHeap.run({
      limits: {
        heapLimitBytes: 1,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      (assertGuestSafeMessage(error.message), true),
  );
});

test('serialization errors do not leak host internals', () => {
  assert.throws(
    () =>
      Progress.load(
        {
          snapshot: Buffer.from('not-a-valid-snapshot'),
        },
        {
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        },
      ),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteSerializationError' &&
      error.kind === 'Serialization' &&
      (assertGuestSafeMessage(error.message), true),
  );
});

test('dump and load preserve compiled programs', async () => {
  const j = new Jslite('Math.max(1, 8, 2);');
  const copy = Jslite.load(j.dump());
  const result = await copy.run();
  assert.equal(result, 8);
});
