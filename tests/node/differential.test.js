const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');

const { Jslite } = require('../../index.js');

async function runJslite(source) {
  const runtime = new Jslite(source);
  return runtime.run();
}

function runNode(source) {
  return vm.runInNewContext(`"use strict";\n${source}`, Object.create(null));
}

async function assertDifferential(source) {
  const [actual, expected] = await Promise.all([
    runJslite(source),
    Promise.resolve(runNode(source)),
  ]);
  assert.deepEqual(actual, expected);
}

test('matches Node for arithmetic and locals', async () => {
  await assertDifferential(`
    const a = 4;
    const b = 3;
    a * b + 2;
  `);
});

test('matches Node for closures and calls', async () => {
  await assertDifferential(`
    function makeAdder(x) {
      return (y) => x + y;
    }
    const add2 = makeAdder(2);
    add2(5);
  `);
});

test('matches Node for branching, loops, and switch', async () => {
  await assertDifferential(`
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
  `);
});
