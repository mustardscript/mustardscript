'use strict';
const { assert, Mustard, Progress, runtime, test } = require('./support/helpers.js');
const { assertDifferential } = require('./runtime-oracle.js');

test('sparse array builders merge correctly across expression branches', async () => {
  for (const source of [
    '1 ? [1,,3] : 0;', '0 || [...[1,,3]];', 'true ? {x:[1,,3]} : null;',
    'false ? null : [1,,3];', '1 && [1,,3];', 'null ?? [1,,3];',
    '[[1,,3],true ? [4,,6] : [7,,9]];',
    'let a=0; a ||= [1,,3]; a;',
    'let a; a ??= [...[1,,3]]; a;',
    'let a=true; a &&= {x:[1,,3]}; a;',
    '(()=> true ? [1,,...[2,,3]] : [,,])();',
    'const out=[]; for(let i=0;i<2;i++) out.push(i ? [1,,3] : [,,]); out;',
  ]) {
    await assertDifferential(source);
    const expected=await runtime(source).run();
    assert.deepEqual(await Mustard.load(runtime(source).dump()).run(),expected);
  }
});

test('sparse branch builders preserve holes across suspension snapshots', () => {
  const options={capabilities:{value(){}},limits:{},snapshotKey:Buffer.from('sparse-branch-regression')};
  let step=runtime('true ? {x:[value(),,value()]} : null;').start(options);
  for (const value of [1,3]) { assert.ok(step instanceof Progress); step=Progress.load(step.dump(),options).resume(value); }
  assert.deepEqual(step,{x:[1,,3]});
});
