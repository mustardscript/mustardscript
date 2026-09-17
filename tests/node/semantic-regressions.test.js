'use strict';

const { assert, Mustard, Progress, runtime, test } = require('./support/helpers.js');
const { assertDifferential } = require('./runtime-oracle.js');

test('loose equality rejects at validation instead of silently using strict equality', async () => {
  for (const source of ['undefined == null;', '"1" == 1;', '1 != "1";', 'if (false) { 0 == 0; }']) {
    for (const lenientMode of [false, true]) {
      const check = error => error.kind === 'Validation' && /loose equality/.test(error.message);
      assert.throws(() => runtime(source, { lenientMode }), check);
      assert.throws(() => Mustard.validateProgram(source, { lenientMode }), check);
    }
  }
  await assertDifferential('[undefined === null, "1" === 1, Number("1") === 1, undefined === null || undefined === undefined];');
});

test('classic for let creates independent bindings at initialization and update boundaries', async () => {
  const sources = [
    'const f=[]; for(let i=0;i<3;i++) f.push(()=>i); f.map(fn=>fn());',
    `const f=[]; let initial; for(let i=0, get=()=>i;i<3;i++) {
       initial=get; f.push(()=>i);
     } [initial(), f.map(fn=>fn())];`,
    `const body=[], update=[], test=[];
     for(let i=0;(test.push(()=>i),i<3);(update.push(()=>i),i++)) body.push(()=>i);
     [body.map(f=>f()), update.map(f=>f()), test.map(f=>f())];`,
    `const f=[]; for(let [i,j]=[0,10], {k=i}={};i<3;i++,j++,k++) f.push(()=>[i,j,k]);
     f.map(fn=>fn());`,
    `const f=[]; for(let i=0;i<2;i++) { f.push(()=>++i); f.push(()=>i); }
     [f[0](),f[1](),f[2](),f[3]()];`,
    `const f=[]; for(let i=0;i<3;i++) { let i=10; f.push(()=>++i); }
     f.map(fn=>fn());`,
    `const f=[]; outer: for(let i=0;i<4;i++) {
       try { for(let j=0;j<2;j++) { f.push(()=>[i,j]); continue outer; } }
       finally { i++; }
     } f.map(fn=>fn());`,
    `const f=[]; for(let i=0;i<4;i++) { f.push(()=>i); if(i===1) break; } f.map(fn=>fn());`,
    `let i; const f=[]; for(i=0;i<3;i++) f.push(()=>i); f.map(fn=>fn());`,
    `const f=[]; for(const value={n:0};value.n<3;value.n++) f.push(()=>value.n); f.map(fn=>fn());`,
    `const f=[]; for(let process=0;process<2;process++) f.push(()=>process); f.map(fn=>fn());`,
    `globalThis.later=9; (()=>{try {for(let first=later,later=1;false;) {}} catch(e) {return e.name;}})();`,
    `(()=>{try {for(const i=0;i<1;i++) {}} catch(e) {return e.name;}})();`,
  ];
  for (const source of sources) await assertDifferential(source);
  assert.throws(() => runtime('for(let process=0;process<1;process++) {} process;'), /forbidden ambient global/);
  assert.throws(() => runtime('for(let i=0,i=1;i<2;i++) {}'), /already been declared/);
});

test('per-iteration closure cells survive loaded programs, suspension, and GC', async () => {
  const source = `
    const f=[];
    for(let i=0, box={n:0};i<6;i++,box={n:i}) {
      f.push(()=>[i,box.n]);
      try { checkpoint(i); continue; }
      finally { checkpoint(box.n); }
    }
    f.map(fn=>fn());
  `;
  const options = {capabilities:{checkpoint() {}}, snapshotKey:Buffer.from('semantic-regression-key'), limits:{}};
  let step = Mustard.load(runtime(source).dump()).start(options);
  let checkpoints=0;
  while (step instanceof Progress) {
    step = Progress.load(step.dump(), options).resume(undefined);
    checkpoints++;
  }
  assert.equal(checkpoints, 12);
  assert.deepEqual(step, Array.from({length:6},(_,i)=>[i,i]));
  const gcSource = `const f=[]; for(let i=0;i<40;i++) {
    f.push(()=>i); for(let j=0;j<30;j++) { const garbage=[j,j,j]; }
  } f.every((fn,i)=>fn()===i);`;
  assert.equal(await runtime(gcSource).run({limits:{heapLimitBytes:65536}}), true);
  await assert.rejects(runtime('for(let i=0;;i++) {}').run({limits:{instructionBudget:200}}), /instruction budget/);
});
