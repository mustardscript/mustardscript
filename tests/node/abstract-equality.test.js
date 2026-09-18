'use strict';

const { assert, Mustard, Progress, runtime, test } = require('./support/helpers.js');
const { assertDifferential } = require('./runtime-oracle.js');

test('abstract equality matches Node across primitive types and string grammars', async () => {
  const values = `undefined, null, false, true, -0, 0, 1, -1, 1.5, NaN, Infinity, -Infinity,
    '', ' ', '\\t\\n\\ufeff', '0', '-0', '+1', '01', '1.5', '1e0', '0x10', '0o10', '0b10',
    '+0x10', '1n', 'NaN', 'Infinity', '-Infinity', '.', '+', 'false', '\\u0085',
    0n, 1n, -1n, 8n, 16n, 9007199254740992, 9007199254740993n, '9007199254740993'`;
  await assertDifferential(`const values=[${values}];
    values.map(a=>values.map(b=>[a==b,a!=b,a===b,a!==b]));`);
});

test('BigInt and Number equality compares exact mathematical values without rounding', async () => {
  await assertDifferential(`const exact=BigInt(Number.MAX_VALUE), two53=9007199254740992n;
    [exact==Number.MAX_VALUE, exact+1n==Number.MAX_VALUE, exact-1n==Number.MAX_VALUE,
     -exact==-Number.MAX_VALUE, two53==9007199254740992, two53+1n==9007199254740992,
     0n==Number.MIN_VALUE, 0n==-0, 1n==1.0000000000000002, 0n==NaN, exact==Infinity,
     123n=='123', 123n=='123.0', 123n=='123e0', 16n=='0X10', -16n=='-0x10'];`);
});

test('ordinary objects, boxed primitives, arrays, Dates, and callables use ToPrimitive', async () => {
  const sources = [
    `const values=[{}, [], [0], [1], [null], [undefined], [[1]], new Number(1), new Number(NaN),
      new String('1'), new Boolean(false), new Map(), new Set(), /a/gi, new Date(0), new Error('x'),
      function f(){return 1;}, Object.groupBy([],x=>x)];
     const primitives=[undefined,null,false,true,0,1,'','1','[object Object]','[object Map]','/a/gi','Error: x'];
     values.map(a=>primitives.map(b=>{try{return [a==b,b==a,a!=b,b!=a];}catch(e){return e.name;}}));`,
    `const a={valueOf(){throw 'unexpected';}}, b={toString(){throw 'unexpected';}};
     [a==a,a==b,a==null,undefined==a,[]==[],new Number(1)==new Number(1),new Number(1)==1];`,
    `const d=new Date(0); [d==d.toString(), d==0, d!=0, new Date(NaN)=='Invalid Date'];`,
    `function f(){return 1;} const bound=f.bind(null);
     [f==f.toString(), bound==bound.toString(), Math.abs==Math.abs.toString(), f==0];`,
    `[Number.prototype==0,String.prototype=='',Boolean.prototype==false,Array.prototype==''];`,
    `const n=new Number(1); n.valueOf=()=>2; const s=new String('1'); s.valueOf=undefined; s.toString=()=>2;
     const b=new Boolean(false); b.valueOf=()=>1; [n==2,n==1,s==2,b==true];`,
    `const a=[]; a.join=()=>7; const b=[]; b.join=undefined;
     [a==7,a=='7',b=='[object Array]',a.toString()];`,
    `const a=[]; a.push(a); const b=[1]; b.push(b,2); [a=='',b=='1,,2'];`,
  ];
  for (const source of sources) await assertDifferential(source);
});

test('array equality coerces live elements with string hints and bounded cycle handling', async () => {
  const sources = [
    `const log=[]; const v={valueOf(){log.push('v');return 9;},toString(){log.push('s');return '7';}}; [[v]=='7',log];`,
    `const a=[0,1]; a[0]={toString(){a[1]=7;a.push(8);return '0';}}; a=='0,7';`,
    `const a=[0,1,2]; a[0]={toString(){a.length=1;return '0';}}; a=='0,,';`,
    `const a=[1]; a.toString=()=>8; [a]==8;`,
    `const a=[{toString:null,valueOf(){return undefined;}}]; a=='undefined';`,
    `const a=[{toString(){return {};},valueOf(){return null;}}]; a=='null';`,
    `const a=[{toString(){throw 9;}}]; try{a=='9';}catch(e){e;}`,
    `const a=[1]; const b=[a,2]; a.push(b,3); [a=='1,,2,3',b=='1,,3,2'];`,
    `const a=[1]; a[0]={toString(){return a==''?'ok':'bad';}}; a=='ok';`,
    `let x=1; const a={valueOf(){x=2;return 1;}}; [x,a==1,x];`,
  ];
  for (const source of sources) await assertDifferential(source);
});

test('coercion handles bound array methods and RegExp string representations', async () => {
  const sources = [
    `const a=[{toString(){return '7';}}]; a.toString=a.toString.bind(a); a=='7';`,
    `const a=[1,2]; a.toString=a.join.bind(a,';'); a=='1;2';`,
    `const a=[1,2]; a.toString=a.join.bind(a,{toString(){a[1]=7;return ';';}}); a=='1;7';`,
    `const o={source:{toString(){o.flags='g';return '7';}},flags:'i',toString:/a/.toString}; o=='/7/g';`,
    `RegExp.prototype=='/(?:)/';`,
  ];
  for (const pattern of ['', 'a/b', '\\/', '[/]', '\n', '\\\n', '\r', '\u2028', '\u2029']) {
    const expected = new RegExp(pattern).toString();
    sources.push(`new RegExp(${JSON.stringify(pattern)})==${JSON.stringify(expected)};`);
  }
  for (const source of sources) await assertDifferential(source);
});

test('coercion preserves evaluation order, method lookup, this binding, and exceptions', async () => {
  const sources = [
    `const log=[]; const value={valueOf(){log.push('valueOf'); return 1;},toString(){log.push('toString');return '2';}};
     function left(){log.push('left');return value;} function right(){log.push('right');return '1';}
     [left()==right(),log];`,
    `const log=[]; const o={valueOf(){log.push('valueOf'); this.toString=()=>{log.push('new');return '1';}; return this;},
       toString(){log.push('old');return '2';}}; [1==o,log];`,
    `const log=[]; const o={valueOf(){log.push(this===o);delete this.valueOf;return {};},
       toString(){log.push(this===o);return '1';}}; [o==1,o==1,log];`,
    `const a={valueOf:0,toString(){return '2';}}, b={valueOf(){return null;}}, c={valueOf(){return undefined;}};
     [a==2,b==0,b==null,c==undefined,c==0];`,
    `const results=[]; for(const a of [{valueOf(){return {};},toString(){return {};}},
       {valueOf:null,toString:null},{valueOf(){throw 7;}},{valueOf(){return {};},toString(){throw 8;}}]) {
       try{results.push(a==1);}catch(e){results.push(typeof e==='number'?e:e.name);}
     } [results,({valueOf(){return 1;}})==1];`,
    `const o={x:3,valueOf:function(){return this.x;}.bind({x:7})}; [o==7,3==o];`,
    `const d=new Date(0),log=[]; d.toString=function(){log.push('string');return this;};
     d.valueOf=function(){log.push('number');return 7;}; [d==7,log];`,
    `const a={valueOf:async()=>1,toString:()=>2}; [a==2,a==1];`,
    `function compare(n){if(n===0)return 1;return {valueOf(){return compare(n-1)==1?1:0;}};}
     compare(40)==1;`,
    `const log=[]; try {try {({valueOf(){throw 7;}})==1;} finally{log.push('finally');}}catch(e){log.push(e);}
     [log,({toString(){return '2';}})==2];`,
  ];
  for (const source of sources) await assertDifferential(source);
});

test('equality coercion survives program and execution snapshots without replaying hooks', () => {
  const options={capabilities:{checkpoint(){}},snapshotKey:Buffer.from('abstract-equality-test-key'),limits:{}};
  const cases = [
    [`const log=[]; const o={valueOf(){log.push('v');return checkpoint('v');},toString(){log.push('s');return checkpoint('s');}};
      [o==7,log];`, [{}, '7'], [true,['v','s']]],
    ['({valueOf:checkpoint})!=7;', ['7'], false],
    [`let result;try {({valueOf:checkpoint})==7;}catch(e){result=e.message;}
      [result,({valueOf(){return 7;}})==7];`, ['throw'], ['host equality error',true]],
    [`let result;try {try{}finally{({valueOf:checkpoint})==7;}}catch(e){result=e.message;}
      [result,1==1];`, ['throw'], ['host equality error',true]],
    ['({valueOf(){return ({valueOf:checkpoint})==7?9:0;}})==9;', [7], true],
    [`const a=[{toString(){return checkpoint('first');}}, {toString(){return checkpoint('second');}}]; a=='1,2';`, [1,2], true],
    [`const a=[[{toString:checkpoint}],3]; a=='2,3';`, [2], true],
    [`let error;try{[{toString:checkpoint}]=='x';}catch(e){error=e.message;} [error,1=='1'];`, ['throw'], ['host equality error',true]],
    [`const a=[1,2];a.toString=a.join.bind(a,{toString:checkpoint});a=='1;2';`, [';'], true],
    [`const o={source:{toString:checkpoint},flags:{toString:checkpoint},toString:/a/.toString};o=='/7/g';`, [7,'g'], true],
    ['await checkpoint(); ({valueOf:async()=>1,toString:()=>2})==2;', [undefined], true],
  ];
  for (const [source,replies,expected] of cases) {
    let step=Mustard.load(runtime(source).dump()).start(options);
    for (const reply of replies) {
      assert.ok(step instanceof Progress,source);
      const restored=Progress.load(step.dump(),options);
      step=reply==='throw'?restored.resumeError(new Error('host equality error')):restored.resume(reply);
    }
    assert.deepEqual(step,expected,source);
  }
});

test('coercion operands stay rooted under GC and obey work and call-depth limits', async () => {
  const source=`({n:7,valueOf(){delete this.valueOf;for(let i=0;i<400;i++){const garbage=[i,i,i];}return this;},
    toString(){for(let i=0;i<400;i++){const garbage={i};}return this.n;}})==7;`;
  assert.equal(await runtime(source).run({limits:{heapLimitBytes:32768}}),true);
  await assert.rejects(runtime(`({valueOf(){while(true){}}})==1;`).run({limits:{instructionBudget:200}}),/instruction budget/);
  await assert.rejects(runtime(`function again(){return {valueOf(){return again()==1?1:0;}};} again()==1;`)
    .run({limits:{callDepthLimit:32}}),/call depth/);
  await assert.rejects(runtime('let a=0; for(let i=0;i<140;i++) a=[a]; a==0;').run(),/nesting limit/);
  await assert.rejects(runtime('let a=Array.from({length:5000},()=>"long element"); a=="";')
    .run({limits:{heapLimitBytes:65536}}),/heap limit/);
  await assert.rejects(runtime('input==1n;').run({inputs:{input:'9'.repeat(20000)},limits:{instructionBudget:10000}}),/instruction budget/);
});
