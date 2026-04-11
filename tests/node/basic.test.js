const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, Progress } = require('../../index.js');

test('run executes sync programs', async () => {
  const j = new Jslite(`
    const values = [1, 2, 3];
    values[0] + values[2];
  `);

  const result = await j.run();
  assert.equal(result, 4);
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

test('dump and load preserve compiled programs', async () => {
  const j = new Jslite('Math.max(1, 8, 2);');
  const copy = Jslite.load(j.dump());
  const result = await copy.run();
  assert.equal(result, 8);
});
