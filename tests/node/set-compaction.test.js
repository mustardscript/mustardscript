'use strict';

const { assert, Mustard, Progress, runtime, test } = require('./support/helpers.js');
const { assertDifferential } = require('./runtime-oracle.js');

test('Set add/delete churn retains bounded storage, including nonempty sets', async () => {
  for (const initial of ['[]', '["permanent"]']) {
    const source = `const s = new Set(${initial});
      for (let i=0; i<30000; i++) { s.add(i); s.delete(i); }
      Array.from(s);`;
    assert.deepEqual(await runtime(source).run({limits:{heapLimitBytes:512*1024,instructionBudget:10000000}}), initial === '[]' ? [] : ['permanent']);
  }
});

test('Set compaction preserves cursors before, within, and after deleted ranges', async () => {
  const source = `
    const s = new Set(Array.from({length:100}, (_,i)=>i));
    const first=s.values(), middle=s.entries(), end=s.keys(), done=s.values();
    first.next();
    for (let i=0;i<50;i++) middle.next();
    for (let i=0;i<100;i++) { end.next(); done.next(); }
    done.next();
    for (let i=0;i<80;i++) s.delete(i);
    s.add(100);
    const result=[Array.from(first),Array.from(middle),Array.from(end),Array.from(done)];
    s.clear(); s.add(101);
    result.push(first.next(), done.next());
    result;
  `;
  await assertDifferential(source);
});

test('Set forEach and algebra callbacks see appended values exactly once during compaction', async () => {
  for (const traversal of [
    's.forEach(visit);',
    's.isSubsetOf({size:100000,has:visit,keys:()=>[].values()});',
  ]) {
    await assertDifferential(`
      const s=new Set(Array.from({length:100},(_,i)=>i)); const seen=[];
      function visit(v) {
        seen.push(v);
        if(v===0) { for(let i=0;i<80;i++) s.delete(i); s.add(100); }
        if(v===90) { s.delete(90); s.add(101); }
        return true;
      }
      ${traversal}
      [seen,Array.from(s)];
    `);
  }
  await assertDifferential(`
    const s=new Set([0]); let n=0;
    s.forEach(v=>{s.delete(v); if(++n<300) s.add(n);});
    [n,s.size];
  `);
});

test('compacted Set cursors survive snapshots and subsequent clear', () => {
  const source = `const s=new Set(Array.from({length:100},(_,i)=>i));
    const it=s.values(); it.next();
    for(let i=0;i<80;i++) s.delete(i);
    checkpoint(); s.add(100); const a=Array.from(it);
    const again=s.values(); again.next(); s.clear(); s.add(200);
    checkpoint(); [a,Array.from(again)];`;
  const options={capabilities:{checkpoint(){}},limits:{},snapshotKey:Buffer.from('set-compaction-regression')};
  let step=Mustard.load(runtime(source).dump()).start(options);
  while(step instanceof Progress) step=Progress.load(step.dump(),options).resume(undefined);
  assert.deepEqual(step,[Array.from({length:21},(_,i)=>80+i),[200]]);
});

test('Set allocation pressure fails cleanly before an unaccounted mutation', async () => {
  await assert.rejects(runtime(`const s=new Set(); for(let i=0;i<1000;i++) s.add('x'.repeat(4096)+i); s.size;`)
    .run({limits:{heapLimitBytes:512*1024,instructionBudget:10000000}}), /heap limit exceeded/);
});
