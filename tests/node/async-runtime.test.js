'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Mustard, MustardError, Progress } = require('../../index.ts');
const { assertDifferential } = require('./runtime-oracle.js');

test('run executes guest async functions, await, and Promise microtasks in order', async () => {
  const runtime = new Mustard(`
    let events = [];
    async function tick(label, value) {
      events[events.length] = label + ':start';
      const resolved = await Promise.resolve(value);
      events[events.length] = label + ':end:' + resolved;
      return resolved;
    }
    async function main() {
      const first = tick('a', 1);
      const second = tick('b', 2);
      events[events.length] = 'sync';
      return [await first, await second, events];
    }
    main();
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    1,
    2,
    ['a:start', 'b:start', 'sync', 'a:end:1', 'b:end:2'],
  ]);
});

test('start and resume drive async host capability suspension inside guest async functions', () => {
  const runtime = new Mustard(`
    async function load(value) {
      const resolved = await fetch_data(value);
      return resolved * 3;
    }
    load(7);
  `);

  const progress = runtime.start({
    capabilities: {
      fetch_data() {
        throw new Error('start should suspend before invoking JS handlers');
      },
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'fetch_data');
  assert.deepEqual(progress.args, [7]);
  assert.equal(progress.resume(7), 21);
});

test('run composes async host rejections with guest try/catch', async () => {
  const runtime = new Mustard(`
    async function load() {
      try {
        await fetch_data(1);
      } catch (error) {
        return [error.name, error.message, error.code, error.details.reason];
      }
    }
    load();
  `);

  const result = await runtime.run({
    capabilities: {
      async fetch_data() {
        const error = new Error('upstream failed');
        error.name = 'CapabilityError';
        error.code = 'E_UPSTREAM';
        error.details = { reason: 'timeout' };
        throw error;
      },
    },
  });

  assert.deepEqual(result, [
    'CapabilityError',
    'upstream failed',
    'E_UPSTREAM',
    'timeout',
  ]);
});

test('run enforces maxOutstandingHostCalls for guest async fan-out', async () => {
  const runtime = new Mustard(`
    async function fanOut() {
      const first = fetch_data(1);
      const second = fetch_data(2);
      return (await first) + (await second);
    }
    fanOut();
  `);

  await assert.rejects(
    runtime.run({
      limits: {
        maxOutstandingHostCalls: 1,
      },
      capabilities: {
        async fetch_data(value) {
          return value;
        },
      },
    }),
    (error) =>
      error instanceof MustardError &&
      error.kind === 'Limit' &&
      /outstanding host-call limit exhausted/.test(error.message),
  );
});

test('run supports Promise instance methods and combinators for the documented surface', async () => {
  const runtime = new Mustard(`
    async function main() {
      let events = [];
      const chained = await Promise.resolve(3)
        .then((value) => {
          events[events.length] = 'then:' + value;
          return value + 4;
        })
        .finally(() => {
          events[events.length] = 'finally';
        });
      const recovered = await Promise.reject('boom').catch((reason) => {
        events[events.length] = 'catch:' + reason;
        return reason + ':handled';
      });
      const all = await Promise.all([1, Promise.resolve(2), chained]);
      const race = await Promise.race([Promise.resolve('fast'), Promise.resolve('slow')]);
      const any = await Promise.any([Promise.reject('x'), Promise.resolve('winner')]);
      const settled = await Promise.allSettled([Promise.resolve(1), Promise.reject('nope')]);
      return [chained, recovered, all, race, any, settled, events];
    }
    main();
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    7,
    'boom:handled',
    [1, 2, 7],
    'fast',
    'winner',
    [
      { status: 'fulfilled', value: 1 },
      { status: 'rejected', reason: 'nope' },
    ],
    ['then:3', 'finally', 'catch:boom'],
  ]);
});

test('run rejects Promise.any with AggregateError details when every input rejects', async () => {
  const runtime = new Mustard(`
    async function main() {
      try {
        await Promise.any([Promise.reject('alpha'), Promise.reject('beta')]);
        return 'unreachable';
      } catch (error) {
        return [error.name, error.message, error.errors];
      }
    }
    main();
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    'AggregateError',
    'All promises were rejected',
    ['alpha', 'beta'],
  ]);
});

test('start and resume drive host capability suspension from Promise callbacks', () => {
  const runtime = new Mustard(`
    async function main() {
      return await Promise.resolve(7).then(fetch_data);
    }
    main();
  `);

  const progress = runtime.start({
    capabilities: {
      fetch_data() {
        throw new Error('start should suspend before invoking JS handlers');
      },
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'fetch_data');
  assert.deepEqual(progress.args, [7]);
  assert.equal(progress.resume(21), 21);
});

test('start and resume support Promise constructors for approval bridges and thenable adoption', () => {
  const runtime = new Mustard(`
    function wrapDouble(value) {
      return new Promise((resolve, reject) => {
        Promise.resolve(value)
          .then((resolved) => resolve(resolved * 2))
          .catch(reject);
      });
    }
    async function waitForApproval(ticketId) {
      return await new Promise((resolve, reject) => {
        fetch_decision(ticketId)
          .then((decision) => {
            if (decision.approved) {
              resolve(decision.ticketId);
            } else {
              reject(decision.reason);
            }
          })
          .catch(reject);
      });
    }
    async function main() {
      const thenable = {};
      thenable.then = function (resolve) {
        resolve(wrapDouble(5));
      };
      return [await Promise.resolve(thenable), await waitForApproval('A-9')];
    }
    main();
  `);

  const progress = runtime.start({
    capabilities: {
      fetch_decision() {
        throw new Error('start should suspend before invoking JS handlers');
      },
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'fetch_decision');
  assert.deepEqual(progress.args, ['A-9']);
  assert.deepEqual(progress.resume({ approved: true, ticketId: 'A-9:approved' }), [
    10,
    'A-9:approved',
  ]);
});

test('run preserves Promise constructor rejection propagation and cleanup ordering', async () => {
  const runtime = new Mustard(`
    async function main() {
      let events = [];
      const denied = await new Promise((resolve, reject) => {
        events[events.length] = 'executor:start';
        reject('manual-review');
        resolve('ignored');
        events[events.length] = 'executor:cleanup';
        throw new Error('ignored');
      }).catch((reason) => {
        events[events.length] = 'catch:' + reason;
        return reason;
      });
      const thenable = {};
      thenable.then = function (resolve, reject) {
        events[events.length] = 'thenable:start';
        reject('thenable:no');
        resolve('ignored');
        events[events.length] = 'thenable:cleanup';
        throw new Error('ignored');
      };

      const adopted = await Promise.resolve(thenable).catch((reason) => {
        events[events.length] = 'adopted:' + reason;
        return reason;
      });

      return [denied, adopted, events];
    }
    main();
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    'manual-review',
    'thenable:no',
    [
      'executor:start',
      'executor:cleanup',
      'catch:manual-review',
      'thenable:start',
      'thenable:cleanup',
      'adopted:thenable:no',
    ],
  ]);
});

test('run preserves thrown values from Promise executors and adopted thenables', async () => {
  const runtime = new Mustard(`
    async function main() {
      const thrown = await new Promise((resolve, reject) => {
        throw 'boom';
      }).catch((reason) => reason);

      const thenable = {};
      thenable.then = function (resolve, reject) {
        throw 'thenable:explode';
      };
      const adopted = await Promise.resolve(thenable).catch((reason) => reason);

      return [thrown, adopted];
    }
    main();
  `);

  const result = await runtime.run();
  assert.deepEqual(result, ['boom', 'thenable:explode']);
});

test('matches Node for supported async microtask ordering', async () => {
  await assertDifferential(`
    let events = [];
    async function tick(label, value) {
      events[events.length] = label + ':start';
      const resolved = await Promise.resolve(value);
      events[events.length] = label + ':end:' + resolved;
      return resolved;
    }
    async function main() {
      const first = tick('a', 1);
      const second = tick('b', 2);
      events[events.length] = 'sync';
      return [await first, await second, events];
    }
    main();
  `);
});

test('matches Node for supported Promise instance methods and combinators', async () => {
  await assertDifferential(`
    async function main() {
      let events = [];
      const chained = await Promise.resolve(3)
        .then((value) => {
          events[events.length] = 'then:' + value;
          return value + 4;
        })
        .finally(() => {
          events[events.length] = 'finally';
        });
      const recovered = await Promise.reject('boom').catch((reason) => {
        events[events.length] = 'catch:' + reason;
        return reason + ':handled';
      });
      const all = await Promise.all([1, Promise.resolve(2), chained]);
      const race = await Promise.race([Promise.resolve('fast'), Promise.resolve('slow')]);
      const any = await Promise.any([Promise.reject('x'), Promise.resolve('winner')]);
      const settled = await Promise.allSettled([Promise.resolve(1), Promise.reject('nope')]);
      return [chained, recovered, all, race, any, settled, events];
    }
    main();
  `);
});

test('matches Node for supported Promise constructors and thenable adoption', async () => {
  await assertDifferential(`
    function wrapDouble(value) {
      return new Promise((resolve, reject) => {
        Promise.resolve(value)
          .then((resolved) => resolve(resolved * 2))
          .catch(reject);
      });
    }
    async function main() {
      let events = [];
      const thenable = {};
      thenable.then = function (resolve) {
        events[events.length] = 'thenable:start';
        resolve(wrapDouble(5));
        events[events.length] = 'thenable:cleanup';
      };
      const denied = await new Promise((resolve, reject) => {
        events[events.length] = 'executor:start';
        reject('manual-review');
        resolve('ignored');
        events[events.length] = 'executor:cleanup';
        throw new Error('ignored');
      }).catch((reason) => {
        events[events.length] = 'catch:' + reason;
        return reason;
      });
      return [await Promise.resolve(thenable), denied, events];
    }
    main();
  `);
});
