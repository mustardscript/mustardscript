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

test('script completion values match Node for branches, blocks, loops, switch, and try', async () => {
  const sources = [
    'if (true) { 42; }', 'if (false) { 1; } else { 2; }',
    '7; if (false) { 1; }', '7; if (true) {}',
    'if (true) { 1; const ignored=9; ; {} }', 'if (true) { 1; undefined; }',
    '7; {}', '7; ; const ignored=1; function unused() {}',
    '"text";', '"use strict"; "last directive"; const ignored=1;',
    '7; label: { break label; }', 'label: { 1; { 2; break label; } 3; }',
    'try { 42; } catch(e) { 9; }', 'try { throw 1; } catch(e) { e+1; }',
    'try { 1; throw 2; } catch(e) {}', '7; try {} finally { 8; }',
    'try { 1; } finally { 2; }', 'try { throw 1; } catch(e) { e+1; } finally { 3; }',
    'try { 1; } finally { try { 2; } finally { 3; } }',
    'try { 1; } finally { label: { 2; break label; } }',
    'outer: { try { 1; break outer; } finally { 2; } }',
    'outer: { 7; try { 8; } finally { break outer; } }',
    'outer: { try { 1; } finally { 2; break outer; } }',
    'try { try { 1; } finally { throw 2; } } catch(e) { e+1; }',
    'let i=0; while(i++<3) { i*10; }', '7; while(false) { 1; }',
    'do { 9; } while(false);', 'for(let i=0;i<3;i++) { i; }',
    'for(const value of [1,2,3]) { value; }', '7; for(const value of []) {}',
    'for(const key in {a:1,b:2}) { key; }',
    'for(let i=0;i<2;i++) { 1; if(i===1) continue; 2; }',
    'for(let i=0;i<2;i++) { try { i; } finally { if(i===0) continue; } }',
    'switch(1) { case 1: 42; break; default: 9; }',
    '7; switch(0) { case 1: 42; }',
    'switch(1) { case 1: 10; case 2: 20; break; }',
    'switch(2) { default: 1; break; case 2: 2; break; }',
    'switch(3) { case 1: 1; break; default: 2; case 3: 3; }',
    'switch(9) { case 1: 1; break; default: 2; case 3: ; }',
    'switch(1) { case 1: const x=42; x; break; default: 9; }',
    'switch(1) { case 1: try { 7; break; } finally { 8; } }',
    '7; function f() { 99; } const ignored=f();',
  ];
  for (const source of sources) await assertDifferential(source);
  for (const lenientMode of [false, true]) {
    assert.equal(await runtime('try { if(true) { 42; } } finally { 0; }', {lenientMode}).run(), 42);
  }
  assert.equal(await runtime('if(true) { 42; } return;', {lenientMode:true}).run(), undefined);
  assert.equal(await runtime('if(true) { 1; } return 42;', {lenientMode:true}).run(), 42);
  await assert.rejects(runtime('try { 1; } finally { throw new Error("cleanup failure"); }').run(), /cleanup failure/);
});

test('statement results survive compilation, await, snapshot restore, and finally GC', async () => {
  const cases = [
    ['if(true) { checkpoint(); 42; }', [undefined], 42],
    ['try { 42; } finally { checkpoint(); 99; }', [undefined], 42],
    ['try { checkpoint(); } catch(e) { 42; } finally { checkpoint(); }', ['throw', undefined], 42],
    ['if(true) { await checkpoint(); 42; }', [undefined], 42],
    ['try { 42; } finally { await checkpoint(); 99; }', [undefined], 42],
    ['label: { try { 42; break label; } finally { checkpoint(); } }', [undefined], 42],
    ['const f=[]; for(let i=0;i<3;i++) f.push(()=>i); try { f.map(fn=>fn()); } finally { checkpoint(); }', [undefined], [0,1,2]],
  ];
  const options = {capabilities:{checkpoint() {}}, snapshotKey:Buffer.from('statement-completion-key'), limits:{}};
  for (const [source, replies, expected] of cases) {
    let step = Mustard.load(runtime(source).dump()).start(options);
    for (const reply of replies) {
      assert.ok(step instanceof Progress, source);
      const restored = Progress.load(step.dump(), options);
      step = reply === 'throw' ? restored.resumeError(new Error('host failure')) : restored.resume(reply);
    }
    assert.deepEqual(step, expected, source);
  }
  const source = `try { ({answer:42}); } finally {
    for(let i=0;i<400;i++) { const garbage=[i,i,i]; }
  }`;
  assert.deepEqual(await runtime(source).run({limits:{heapLimitBytes:32768}}), {answer:42});
});

test('nested empty, normal, and abrupt completion combinations match Node', async () => {
  const atoms = ['', ';', '1;', 'undefined;', 'let x=0;', 'break out;', 'throw 4;'];
  for (const first of atoms) {
    for (const second of atoms) {
      const next = second.replace('let x', 'let y');
      for (const body of [
        `try {${first}} finally {${next}}`,
        `try {${first}} catch(e) {${next}}`,
        `try {${first}} finally {try {${next}} finally {9;}}`,
        `for(let i=0;i<2;i++) {${first} try {${next}} finally {9;}}`,
        `switch(1) {default: ${first} case 1: ${next}}`,
      ]) {
        await assertDifferential(`try {out: {8; ${body}}} catch(e) {e;}`);
      }
    }
  }
});
