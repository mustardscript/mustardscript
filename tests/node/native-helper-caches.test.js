'use strict';
const {assert, Mustard, Progress, runtime, test} = require('./support/helpers.js');
const {assertDifferential} = require('./runtime-oracle.js');

test('cached regexes preserve independent lastIndex and flag state', async () => {
  await assertDifferential(`const patterns=[/a/g,/b/g,/c/g,/d/g,/e/g,/a/i,/a/y,/a/dg,/a/m];const result=[];
    for(let i=0;i<30;i++){for(const re of patterns){result.push([re.test('abcde'),re.lastIndex]);}}
    result;`);
});

test('cached collators honor all options, option mutations and invalid profiles', async () => {
  await assertDifferential(`const words=['a','B','ä','A','a10','a2','a-b','ab'];const results=[];
    for(const numeric of [false,true]) for(const caseFirst of ['false','upper','lower']) for(const sensitivity of ['base','accent','case','variant']) for(const ignorePunctuation of [false,true]) {
      const options={numeric,caseFirst,sensitivity,ignorePunctuation};
      results.push(words.toSorted((a,b)=>a.localeCompare(b,'en-US',options)).join('|'));
    }
    results;`);
  await assertDifferential(`const options={numeric:false}; const result=['a10'.localeCompare('a2','en-US',options)];options.numeric=true;result.push('a10'.localeCompare('a2','en-US',options));result.map(Math.sign);`);
  await assert.rejects(runtime(`'a'.localeCompare('b'); 'a'.localeCompare('b','fr');`).run(),/en-US|locale/);
});

test('native caches rebuild after snapshot restore without changing results', () => {
  const source=`const re=/a/g; re.test('a a'); 'a10'.localeCompare('a2','en-US',{numeric:true});checkpoint();
    [re.exec('a a').index,'a10'.localeCompare('a2','en-US',{numeric:true})>0];`;
  const options={capabilities:{checkpoint(){}},limits:{},snapshotKey:Buffer.from('native-cache-restore')};
  let step=new Mustard(source).start(options);assert.ok(step instanceof Progress);
  step=Progress.load(step.dump(),options).resume(undefined);assert.deepEqual(step,[2,true]);
});
