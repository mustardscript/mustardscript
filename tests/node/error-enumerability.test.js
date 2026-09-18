'use strict';
const {assert, Mustard, Progress, runtime, test} = require('./support/helpers.js');
const {assertDifferential} = require('./runtime-oracle.js');

test('Error enumeration follows own property attributes, not reserved names', async () => {
  for (const ctor of ['Error','TypeError','SyntaxError','RangeError','ReferenceError','URIError','EvalError']) {
    await assertDifferential(`
      function inspect(e) {return [Object.keys(e),Object.values(e),Object.entries(e),{...e},JSON.stringify(e),['name','message','stack','cause','errors'].map(k=>Object.hasOwn(e,k))];}
      const e=new ${ctor}(); e.name='ValidationError'; e.message='assigned';e.cause='user';e.errors=[1];e.stack='hidden';
      const result=[inspect(e),e.toString()];
      delete e.stack;e.stack='visible';result.push(inspect(e));
      const defined=new ${ctor}('initial',{cause:0}); defined.message='hidden';defined.cause='hidden';defined.name='OwnName';
      result.push(inspect(defined));delete defined.message;defined.message='visible';delete defined.cause;defined.cause='visible';
      result.push(inspect(defined));JSON.stringify(result);
    `);
  }
  await assertDifferential(`const e=new Error(undefined,{}); [Object.hasOwn(e,'cause'),Object.hasOwn(e,'message'),Object.hasOwn(e,'name'),'name' in e,'message' in e];`);
});

test('AggregateError errors stay hidden until deleted and newly assigned', async () => {
  await assertDifferential(`const e=new AggregateError([1],'message');e.errors=[2];e.name='Own'; const before=[JSON.stringify(e),Object.keys(e),e.errors];delete e.errors;e.errors=[3];[before,JSON.stringify(e),Object.keys(e)];`);
  await assertDifferential(`(async()=>{let result;try{await Promise.any([Promise.reject(1)]);}catch(e){e.name='Own';result=[Object.keys(e),JSON.stringify(e),e.errors];}return result;})();`);
  await assertDifferential(`const e=new Error('private',{cause:1}); e.name='Own'; JSON.stringify(e,['name','message','cause']);`);
});

test('Error attributes and newly enumerable values survive GC and snapshots', async () => {
  const options={capabilities:{checkpoint(){}},limits:{heapLimitBytes:128*1024},snapshotKey:Buffer.from('error-enumerability')};
  const source=`const e=new Error('hidden',{cause:{x:1}}); e.name='Own'; e.errors={kept:42}; delete e.stack; e.stack='visible';
    for(let i=0;i<300;i++) { const garbage={i,text:'x'.repeat(300)}; }
    checkpoint(); [Object.keys(e),JSON.stringify(e),e.cause.x];`;
  let step=new Mustard(source).start(options);assert.ok(step instanceof Progress);
  step=Progress.load(step.dump(),options).resume(undefined);
  assert.deepEqual(step,[['name','errors','stack'],'{"name":"Own","errors":{"kept":42},"stack":"visible"}',1]);
});
