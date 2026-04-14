'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const {
  createHeadlineSeedScenarioDefinitions,
} = require('./ptc-headline-seed-fixtures.ts');

const REPO_ROOT = path.join(__dirname, '..');

let expectedResultsCache = null;

function loadExpectedHeadlineSeedResults() {
  if (expectedResultsCache) {
    return expectedResultsCache;
  }

  const output = execFileSync(
    process.execPath,
    [path.join(REPO_ROOT, 'scripts', 'audit-ptc-headline-seeds.ts'), '--json'],
    {
      cwd: REPO_ROOT,
      encoding: 'utf8',
    },
  );
  const summary = JSON.parse(output);
  expectedResultsCache = Object.fromEntries(
    summary.results
      .filter((result) => result.ok)
      .map((result) => [result.metricName, result.value]),
  );
  return expectedResultsCache;
}

function createHeadlineSeedScenarios() {
  const scenarios = createHeadlineSeedScenarioDefinitions();
  const expectedResults = loadExpectedHeadlineSeedResults();
  return Object.fromEntries(
    Object.entries(scenarios).map(([metricName, scenario]) => {
      const expectedResult = expectedResults[metricName];
      if (expectedResult === undefined) {
        throw new Error(`Missing skewed headline expected result for ${metricName}`);
      }
      return [metricName, {
        ...scenario,
        assertResult(result) {
          assert.deepStrictEqual(result, expectedResult);
        },
      }];
    }),
  );
}

module.exports = {
  createHeadlineSeedScenarios,
  loadExpectedHeadlineSeedResults,
};
