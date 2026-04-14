'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const {
  metricNameForUseCase,
  USE_CASE_METADATA,
} = require('./ptc-portfolio.ts');

const REPO_ROOT = path.join(__dirname, '..');
const EXAMPLES_ROOT = path.join(REPO_ROOT, 'examples', 'programmatic-tool-calls');

let expectedResultsCache = null;

function loadCatalog(relativePath) {
  const catalogPath = path.join(EXAMPLES_ROOT, relativePath, 'catalog.ts');
  const entries = require(catalogPath);
  return entries.map((entry) => ({
    ...entry,
    category: relativePath,
    absoluteFile: path.join(EXAMPLES_ROOT, relativePath, entry.file),
  }));
}

function loadGalleryDescriptors() {
  return [
    ...loadCatalog('analytics'),
    ...loadCatalog('operations'),
    ...loadCatalog('workflows'),
  ];
}

function loadExpectedGalleryResults() {
  if (expectedResultsCache) {
    return expectedResultsCache;
  }

  const output = execFileSync(
    process.execPath,
    [path.join(REPO_ROOT, 'scripts', 'audit-use-cases.ts'), '--json'],
    {
      cwd: REPO_ROOT,
      encoding: 'utf8',
    },
  );
  const summary = JSON.parse(output);
  expectedResultsCache = Object.fromEntries(
    summary.results
      .filter((result) => result.ok)
      .map((result) => [result.id, result.value]),
  );
  return expectedResultsCache;
}

function cloneValue(value) {
  return value === undefined ? undefined : structuredClone(value);
}

function normalizeRepoRelativePath(relativePath) {
  return relativePath.replaceAll('\\', '/');
}

function repoRelativeSourceRef(absoluteFile) {
  return normalizeRepoRelativePath(path.relative(REPO_ROOT, absoluteFile));
}

function descriptorInputs(descriptor) {
  return cloneValue(descriptor.inputs ?? descriptor.options?.inputs ?? {});
}

function createCapabilitiesFromStartPlan(descriptor) {
  const startPlan = descriptor.startPlan ?? {};
  const capabilityNames = Object.keys(startPlan.capabilities ?? {});
  const queue = (startPlan.resumes ?? []).map((entry, index) => {
    if (
      entry &&
      typeof entry === 'object' &&
      !Array.isArray(entry) &&
      'capability' in entry &&
      'value' in entry
    ) {
      return {
        capability: entry.capability,
        value: cloneValue(entry.value),
      };
    }

    return {
      capability: capabilityNames[index] ?? null,
      value: cloneValue(entry),
    };
  });

  return Object.fromEntries(
    capabilityNames.map((name) => [
      name,
      () => {
        const next = queue.shift();
        assert.ok(next, `No planned resume payload remains for ${descriptor.id}`);
        assert.equal(
          next.capability,
          name,
          `${descriptor.id} expected ${next.capability} before ${name}`,
        );
        return cloneValue(next.value);
      },
    ]),
  );
}

function createCapabilitiesForDescriptor(descriptor) {
  if (descriptor.options?.capabilities) {
    return descriptor.options.capabilities;
  }
  if (descriptor.startPlan) {
    return createCapabilitiesFromStartPlan(descriptor);
  }
  return {};
}

function createGalleryScenario(descriptor) {
  const metadata = USE_CASE_METADATA[descriptor.id];
  if (!metadata) {
    throw new Error(`Missing phase-2 benchmark metadata for ${descriptor.id}`);
  }

  const sourceRef = repoRelativeSourceRef(descriptor.absoluteFile);
  const expectedResults = loadExpectedGalleryResults();
  const expectedResult = expectedResults[descriptor.id];
  if (expectedResult === undefined) {
    throw new Error(`Missing audited expected result for ${descriptor.id}`);
  }

  const capabilityNames = descriptor.options?.capabilities
    ? Object.keys(descriptor.options.capabilities)
    : Object.keys(descriptor.startPlan?.capabilities ?? {});

  return {
    metricName: metricNameForUseCase(descriptor.id),
    laneId: descriptor.id,
    category: descriptor.category,
    sizeName: 'medium',
    sourceFile: sourceRef,
    source: fs.readFileSync(descriptor.absoluteFile, 'utf8'),
    inputs: descriptorInputs(descriptor),
    shape: {
      sourceRef,
      toolFamilyCount: capabilityNames.length,
      logicalPeakFanout: metadata.logicalPeakFanout,
      compactionExpectation: metadata.compactionExpectation,
      ...metadata.shapes,
    },
    createCapabilities() {
      return createCapabilitiesForDescriptor(descriptor);
    },
    assertResult(result) {
      assert.deepStrictEqual(result, expectedResult);
    },
  };
}

function createGalleryScenarios() {
  const scenarios = {};
  for (const descriptor of loadGalleryDescriptors()) {
    const scenario = createGalleryScenario(descriptor);
    scenarios[scenario.metricName] = scenario;
  }
  return scenarios;
}

module.exports = {
  createGalleryScenarios,
  createCapabilitiesForDescriptor,
  descriptorInputs,
  loadExpectedGalleryResults,
  loadGalleryDescriptors,
  normalizeRepoRelativePath,
  repoRelativeSourceRef,
};
