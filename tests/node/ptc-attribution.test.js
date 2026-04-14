'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  annotateCollectionCallSites,
  offsetToLineColumn,
} = require('../../benchmarks/ptc-attribution.ts');

test('offsetToLineColumn resolves one-based positions from source offsets', () => {
  const source = 'alpha\nbeta\ngamma';

  assert.deepEqual(offsetToLineColumn(source, 0), { line: 1, column: 1 });
  assert.deepEqual(offsetToLineColumn(source, 6), { line: 2, column: 1 });
  assert.deepEqual(offsetToLineColumn(source, source.length), { line: 3, column: 6 });
});

test('annotateCollectionCallSites sorts hotspots and adds source locations', () => {
  const scenario = {
    sourceFile: 'examples/programmatic-tool-calls/analytics/investigate-fraud-ring.js',
    source: [
      'const map = new Map();',
      'for (const row of rows) {',
      '  total += map.get(row.id);',
      '}',
      'set.add(total);',
      '',
    ].join('\n'),
  };
  const metrics = {
    collection_call_sites: [
      {
        function_name: null,
        instruction_offset: 9,
        span: { start: scenario.source.indexOf('set.add'), end: scenario.source.indexOf('set.add') + 'set.add(total)'.length },
        map_get_calls: 0,
        map_set_calls: 0,
        set_add_calls: 1,
        set_has_calls: 0,
      },
      {
        function_name: null,
        instruction_offset: 4,
        span: { start: scenario.source.indexOf('map.get'), end: scenario.source.indexOf('map.get') + 'map.get(row.id)'.length },
        map_get_calls: 7,
        map_set_calls: 0,
        set_add_calls: 0,
        set_has_calls: 0,
      },
    ],
  };

  const annotated = annotateCollectionCallSites(metrics, scenario, 4);

  assert.equal(annotated.collection_hotspots.length, 2);
  assert.equal(annotated.collection_hotspots[0].total_calls, 7);
  assert.equal(annotated.collection_hotspots[0].start_line, 3);
  assert.equal(annotated.collection_hotspots[0].source_file, scenario.sourceFile);
  assert.match(annotated.collection_hotspots[0].snippet, /map\.get/);
  assert.equal(annotated.collection_hotspots[1].start_line, 5);
  assert.match(annotated.collection_hotspots[1].snippet, /set\.add/);
});
