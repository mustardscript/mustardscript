'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const {
  USE_CASE_METADATA,
} = require('./ptc-portfolio.ts');
const {
  createCapabilitiesForDescriptor,
  descriptorInputs,
  loadExpectedGalleryResults,
  loadGalleryDescriptors,
} = require('./ptc-gallery.ts');

const REPO_ROOT = path.join(__dirname, '..');

const DURABLE_GALLERY_USE_CASES = Object.freeze({
  'plan-database-failover': {
    checkpointCapability: 'request_operator_approval',
  },
  'privacy-erasure-orchestration': {
    checkpointCapability: 'queue_erasure_job',
  },
});

function durableMetricNameForUseCase(id) {
  return `ptc_${id}_durable_medium`;
}

function createDurableGalleryScenario(descriptor, durableConfig) {
  const metadata = USE_CASE_METADATA[descriptor.id];
  if (!metadata) {
    throw new Error(`Missing durable benchmark metadata for ${descriptor.id}`);
  }

  const expectedResults = loadExpectedGalleryResults();
  const expectedResult = expectedResults[descriptor.id];
  if (expectedResult === undefined) {
    throw new Error(`Missing audited expected result for durable lane ${descriptor.id}`);
  }

  const sourceRef = path.relative(REPO_ROOT, descriptor.absoluteFile);
  const capabilityNames = descriptor.options?.capabilities
    ? Object.keys(descriptor.options.capabilities)
    : Object.keys(descriptor.startPlan?.capabilities ?? {});

  return {
    metricName: durableMetricNameForUseCase(descriptor.id),
    laneId: descriptor.id,
    category: descriptor.category,
    sizeName: 'medium',
    checkpointCapability: durableConfig.checkpointCapability,
    sourceFile: sourceRef,
    source: fs.readFileSync(descriptor.absoluteFile, 'utf8'),
    inputs: descriptorInputs(descriptor),
    shape: {
      sourceRef,
      toolFamilyCount: capabilityNames.length,
      logicalPeakFanout: metadata.logicalPeakFanout,
      compactionExpectation: metadata.compactionExpectation,
      finalAction: metadata.shapes.finalActionWriteback,
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

function createDurableGalleryScenarios() {
  const descriptorsById = new Map(
    loadGalleryDescriptors().map((descriptor) => [descriptor.id, descriptor]),
  );
  return Object.fromEntries(
    Object.entries(DURABLE_GALLERY_USE_CASES).map(([useCaseId, durableConfig]) => {
      const descriptor = descriptorsById.get(useCaseId);
      if (!descriptor) {
        throw new Error(`Missing audited descriptor for durable lane ${useCaseId}`);
      }
      const scenario = createDurableGalleryScenario(descriptor, durableConfig);
      return [scenario.metricName, scenario];
    }),
  );
}

module.exports = {
  DURABLE_GALLERY_USE_CASES,
  createDurableGalleryScenarios,
  durableMetricNameForUseCase,
};
