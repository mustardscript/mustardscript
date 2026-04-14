'use strict';

const { Mustard, MustardError } = require('../index.ts');
const {
  createHeadlineSeedScenarioDefinitions,
} = require('../benchmarks/ptc-headline-seed-fixtures.ts');

function normalizeError(error) {
  if (error instanceof MustardError) {
    return {
      kind: error.kind,
      name: error.name,
      message: error.message,
    };
  }
  if (error instanceof Error) {
    return {
      kind: 'Host',
      name: error.name,
      message: error.message,
    };
  }
  return {
    kind: 'Host',
    name: 'Error',
    message: String(error),
  };
}

async function executeScenario(scenario) {
  try {
    const runtime = new Mustard(scenario.source);
    const value = await runtime.run({
      inputs: scenario.inputs,
      capabilities: scenario.createCapabilities(),
    });
    return {
      ok: true,
      metricName: scenario.metricName,
      laneId: scenario.laneId,
      skewPatterns: scenario.skewPatterns,
      value,
    };
  } catch (error) {
    return {
      ok: false,
      metricName: scenario.metricName,
      laneId: scenario.laneId,
      skewPatterns: scenario.skewPatterns,
      error: normalizeError(error),
    };
  }
}

async function main() {
  const scenarios = Object.values(createHeadlineSeedScenarioDefinitions());
  const results = [];

  for (const scenario of scenarios) {
    results.push(await executeScenario(scenario));
  }

  const failures = results.filter((result) => !result.ok);
  const summary = {
    total: results.length,
    passed: results.length - failures.length,
    failed: failures.length,
    results,
  };

  if (process.argv.includes('--json')) {
    process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
    return;
  }

  console.log(`Audited ${summary.total} skewed headline seed scenarios`);
  console.log(`Passed: ${summary.passed}`);
  console.log(`Failed: ${summary.failed}`);
  if (failures.length > 0) {
    console.log('');
    for (const failure of failures) {
      console.log(`${failure.metricName} -> ${failure.error.kind}: ${failure.error.message}`);
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
