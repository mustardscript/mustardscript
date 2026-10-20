'use strict';
const {assert, runtime, test} = require('./support/helpers.js');
const {assertDifferential} = require('./runtime-oracle.js');

test('ordinary global regexp workloads fit the default instruction budget', async () => {
  for (const expression of [
    `text.replace(/a/g, 'b').length`,
    `text.replace(/(a)/g, '$1b').length`,
    `text.replace(/(?<letter>a)/g, (all, letter) => letter + 'b').length`,
    `text.match(/(a)/g).length`,
    `text.split(/(a)/g).length`,
    `Array.from(text.matchAll(/(a)/dg)).length`,
  ]) {
    const source = `const text = 'a '.repeat(1500); ${expression};`;
    await assertDifferential(source);
  }
});

test('global regexp cursors preserve Unicode offsets, nested indices and empty matches', async () => {
  // MustardScript's existing RegExp offsets count Unicode scalars, not UTF-16.
  assert.deepEqual(await runtime(`Array.from('😀é a😀'.matchAll(/(?<outer>(é)|(a))/dg)).map(m=>[m.index,m.indices,m.indices.groups.outer===m.indices[1]]);`).run(), [
    [1, [[1,2],[1,2],[1,2],undefined], true],
    [3, [[3,4],[3,4],undefined,[3,4]], true],
  ]);
  assert.deepEqual(await runtime(`Array.from('😀é'.matchAll(/(?:)/g)).map(m=>m.index);`).run(), [0,1,2]);
  for (const text of ['', 'abc']) {
    await assertDifferential(`const text=${JSON.stringify(text)}; [Array.from(text.matchAll(/(?:)/g)).map(m=>m.index), text.replace(/(?:)/g,'!'), text.split(/(?:)/g), text.match(/(?:)/g)];`);
  }
  await assertDifferential(`const re=/(a)/dgy; re.lastIndex=2; Array.from('x aa'.matchAll(re)).map(m=>[m.index,m.indices]);`);
});

test('regexp scanning and capture output still exhaust genuinely small budgets', async () => {
  for (const source of [`text.search(/z/);`, `text.match(/(a+)/dg);`, `text.replace(/a/g,'b');`]) {
    await assert.rejects(runtime(source).run({inputs:{text:'a'.repeat(3000)},limits:{instructionBudget:100}}), /instruction budget/);
  }
});

test('shift drains ordinary queues without splice allocations or suffix charges', async () => {
  assert.equal(await runtime(`const q=Array.from({length:1500},(_,i)=>i); let total=0; while(q.length) total+=q.shift(); total;`).run(), 1124250);
  // Tight allocation allowance after array construction: shift must not create
  // one throwaway guest array per removal.
  assert.equal(await runtime(`let total=0; while(q.length) total+=q.shift(); total;`).run({inputs:{q:Array.from({length:1500},(_,i)=>i)},limits:{allocationBudget:64}}),1124250);
  await assertDifferential(`const q=[,undefined,{x:1},,3]; q.note='keep'; const result=[]; while(q.length) {result.push([q.shift(),Object.keys(q),q.length]);} [result,q.note,q.shift()];`);
});

test('global exec cursors resolve lastIndex past the char-counting block on large non-ASCII subjects', async () => {
  // `lastIndex` is resolved to a byte offset with a block-counting helper.
  // Every hit here sits beyond the first 32 KiB block, and the subject uses
  // BMP characters only so MustardScript's scalar offsets agree with V8's.
  await assertDifferential(`const text = 'é'.repeat(20000) + ' alpha.ts ' + 'ü'.repeat(20000) + ' beta.ts ' + 'ö'.repeat(100);
    const re = /[a-z]+\\.ts/g; const out = []; let m;
    while ((m = re.exec(text)) !== null) { out.push([m.index, re.lastIndex, m[0]]); }
    re.lastIndex = 20005; out.push(re.exec(text).index, re.lastIndex);
    re.lastIndex = text.length; out.push(re.exec(text), re.lastIndex);
    re.lastIndex = text.length + 5; out.push(re.exec(text), re.lastIndex);
    [out, text.slice(20004, 20012), text.slice(40010, 40020), text.lastIndexOf('ö')];`);
});

test('exec and test accept non-string subjects without changing coercion', async () => {
  await assertDifferential(`const re = /1(2)/g; const out = [];
    for (const subject of [12, null, undefined, true, ['x12'], 'z12']) {
      out.push([re.exec(subject)?.index ?? null, re.lastIndex, re.test(subject), re.lastIndex]);
      re.lastIndex = 0;
    }
    out;`);
});
