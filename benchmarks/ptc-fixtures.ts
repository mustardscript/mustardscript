'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const EXAMPLES_ROOT = path.join(__dirname, '..', 'examples', 'programmatic-tool-calls');

const PTC_WEIGHTS = Object.freeze({
  ptc_incident_triage_medium: 0.4,
  ptc_fraud_investigation_medium: 0.35,
  ptc_vendor_review_medium: 0.25,
});

const WEBSITE_SIZE_CONFIGS = Object.freeze({
  small: { alertCount: 4, deployCount: 2, logCount: 9 },
  medium: { alertCount: 10, deployCount: 4, logCount: 24 },
  large: { alertCount: 24, deployCount: 8, logCount: 60 },
});

const INCIDENT_SIZE_CONFIGS = Object.freeze({
  small: { regionCount: 3, alertsPerRegion: 2, samplesPerRegion: 2 },
  medium: { regionCount: 6, alertsPerRegion: 3, samplesPerRegion: 3 },
  large: { regionCount: 12, alertsPerRegion: 4, samplesPerRegion: 4 },
});

const FRAUD_SIZE_CONFIGS = Object.freeze({
  small: { transactionCount: 24, accountCount: 8, entityCount: 6, cardCount: 5 },
  medium: { transactionCount: 96, accountCount: 24, entityCount: 16, cardCount: 12 },
  large: { transactionCount: 384, accountCount: 96, entityCount: 64, cardCount: 48 },
});

const VENDOR_SIZE_CONFIGS = Object.freeze({
  small: { frameworkCount: 3, flowCount: 4, subprocessorCount: 4 },
  medium: { frameworkCount: 5, flowCount: 10, subprocessorCount: 10 },
  large: { frameworkCount: 7, flowCount: 24, subprocessorCount: 24 },
});

function loadExampleSource(relativePath) {
  return fs.readFileSync(path.join(EXAMPLES_ROOT, relativePath), 'utf8');
}

function structuredByteLength(value) {
  return Buffer.byteLength(JSON.stringify(value), 'utf8');
}

function createCapabilityTransferProbe(capabilities) {
  const stats = {
    toolCallCount: 0,
    toolBytesIn: 0,
    toolFamilyCount: Object.keys(capabilities).length,
  };

  const wrapped = Object.fromEntries(
    Object.entries(capabilities).map(([name, handler]) => [
      name,
      (...args) => {
        stats.toolCallCount += 1;
        const value = handler(...args);
        if (value && typeof value.then === 'function') {
          return Promise.resolve(value).then((resolved) => {
            stats.toolBytesIn += structuredByteLength(resolved);
            return resolved;
          });
        }
        stats.toolBytesIn += structuredByteLength(value);
        return value;
      },
    ]),
  );

  return {
    capabilities: wrapped,
    finalize(result) {
      const resultBytesOut = structuredByteLength(result);
      return {
        toolCallCount: stats.toolCallCount,
        toolFamilyCount: stats.toolFamilyCount,
        toolBytesIn: stats.toolBytesIn,
        resultBytesOut,
        reductionRatio:
          resultBytesOut === 0 ? Number.POSITIVE_INFINITY : stats.toolBytesIn / resultBytesOut,
      };
    },
  };
}

function summarizePtcWeightedScore(latencyByName) {
  let weightedMedianMs = 0;
  let weightedP95Ms = 0;
  for (const [metricName, weight] of Object.entries(PTC_WEIGHTS)) {
    const metric = latencyByName[metricName];
    if (!metric) {
      throw new Error(`Missing latency metric for weighted PTC score: ${metricName}`);
    }
    weightedMedianMs += metric.medianMs * weight;
    weightedP95Ms += metric.p95Ms * weight;
  }
  return {
    medianMs: weightedMedianMs,
    p95Ms: weightedP95Ms,
  };
}

function createWebsiteDemoScenario(sizeName) {
  const config = WEBSITE_SIZE_CONFIGS[sizeName];
  const degradedMetricNames =
    sizeName === 'large'
      ? ['cpu_saturation', 'error_rate', 'p95_latency']
      : ['error_rate', 'p95_latency'];
  const degradedMetricSet = new Set(degradedMetricNames);
  const inputs = {
    service: 'auth-gateway',
  };

  return {
    metricName: `ptc_website_demo_${sizeName}`,
    laneId: 'website_demo',
    sizeName,
    sourceFile: 'operations/triage-production-incident.js',
    source: loadExampleSource('operations/triage-production-incident.js'),
    inputs,
    shape: {
      sourceRef: 'examples/programmatic-tool-calls/operations/triage-production-incident.js',
      toolFamilyCount: 4,
      logicalPeakFanout: 1,
      finalAction: false,
      usesPromiseAll: false,
      usesMap: false,
      usesSet: false,
      scale: config,
    },
    createCapabilities() {
      return {
        list_recent_alerts() {
          return Array.from({ length: config.alertCount }, (_, index) => ({
            id: `alert_${sizeName}_${index}`,
            severity: index % 3 === 0 ? 'critical' : 'high',
            summary: `Alert ${index} for ${inputs.service}`,
          }));
        },
        list_recent_deploys() {
          return Array.from({ length: config.deployCount }, (_, index) => ({
            id: `deploy_${sizeName}_${index}`,
            version: `2026.04.${10 + index}`,
            minutesAgo: 10 + index * 7,
          }));
        },
        search_logs() {
          const tokens = ['timeout', 'saturation', 'rollback'];
          return Array.from({ length: config.logCount }, (_, index) => (
            `${tokens[index % tokens.length]} detected in shard-${index % 4} for ${inputs.service}`
          ));
        },
        fetch_metric_window(_service, metric) {
          return {
            metric,
            status: degradedMetricSet.has(metric) ? 'degraded' : 'healthy',
            latest: metric === 'p95_latency' ? 820 : 0.028,
            baseline: metric === 'p95_latency' ? 410 : 0.011,
          };
        },
      };
    },
    assertResult(result) {
      assert.equal(result.service, inputs.service);
      assert.equal(result.alertCount, config.alertCount);
      assert.equal(result.recentDeploys, config.deployCount);
      assert.deepEqual([...result.degradedMetrics].sort(), degradedMetricNames);
      assert.equal(result.timeoutSignals.length, config.logCount);
    },
  };
}

function createIncidentScenario(sizeName) {
  const config = INCIDENT_SIZE_CONFIGS[sizeName];
  const inputs = {
    incidentId: `inc_ptc_${sizeName}`,
    service: 'auth-gateway',
    regions: Array.from({ length: config.regionCount }, (_, index) => `region-${index}`),
  };
  const regionIndexByName = new Map(inputs.regions.map((region, index) => [region, index]));

  return {
    metricName: `ptc_incident_triage_${sizeName}`,
    laneId: 'incident_triage',
    sizeName,
    sourceFile: 'operations/triage-multi-region-auth-outage.js',
    source: loadExampleSource('operations/triage-multi-region-auth-outage.js'),
    inputs,
    shape: {
      sourceRef: 'examples/programmatic-tool-calls/operations/triage-multi-region-auth-outage.js',
      toolFamilyCount: 5,
      logicalPeakFanout: config.regionCount * 3,
      finalAction: false,
      usesPromiseAll: true,
      usesMap: true,
      usesSet: true,
      scale: config,
    },
    createCapabilities() {
      return {
        load_incident_timeline() {
          return [
            {
              ts: '2026-04-11T07:05:00Z',
              kind: 'deploy',
              note: 'deploy reached 100 percent in the primary region',
            },
            {
              ts: '2026-04-11T07:09:00Z',
              kind: 'symptom',
              note: 'certificate refresh lag started after the deploy',
            },
            {
              ts: '2026-04-11T07:13:00Z',
              kind: 'symptom',
              note: 'dns fallback path timed out for the signer endpoint',
            },
            {
              ts: '2026-04-11T07:18:00Z',
              kind: 'mitigation',
              note: 'rollback remains under consideration',
            },
          ];
        },
        list_regional_alerts(_service, region) {
          const index = regionIndexByName.get(region);
          return Array.from({ length: config.alertsPerRegion }, (_, alertIndex) => ({
            severity: index % 4 === 0 && alertIndex === 0 ? 'critical' : 'high',
            signal:
              alertIndex % 2 === 0 ? 'token_validation_failure' : 'dns_lookup_errors',
            fingerprint: `fp-${Math.floor(index / 2)}-${alertIndex % 2}`,
            summary: `Regional alert ${alertIndex} for ${region}`,
          }));
        },
        fetch_service_slo(_service, region) {
          const index = regionIndexByName.get(region);
          return {
            region,
            availability: index % 2 === 0 ? 98.8 : 99.72,
            latencyP95: index % 3 === 0 ? 1220 : 640,
            errorRate: index % 2 === 0 ? 0.061 : 0.018,
          };
        },
        search_error_samples(_service, _incidentId, region) {
          const index = regionIndexByName.get(region);
          const samples = [
            'jwks cache timeout after certificate rotation',
            'dns lookup timeout talking to signer endpoint',
            'provider rate limit while refreshing token',
            'downstream timeout waiting for key propagation',
          ];
          return Array.from({ length: config.samplesPerRegion }, (_, sampleIndex) => (
            `${samples[(index + sampleIndex) % samples.length]} in ${region}`
          ));
        },
        get_mitigation_runbook() {
          return {
            owners: ['identity-oncall', 'traffic-manager'],
            immediateActions: [
              'confirm blast radius by region',
              'pause rollout progression',
              'prepare rollback and traffic shift',
            ],
            rollbackTriggers: ['token validation failures continue after key sync'],
          };
        },
      };
    },
    assertResult(result) {
      assert.equal(result.service, inputs.service);
      assert.equal(result.regionalFindings.length, config.regionCount);
      assert.equal(result.severity, 'critical');
      assert.ok(result.alertCount >= config.regionCount);
      assert.ok(result.impactedRegions.length >= Math.ceil(config.regionCount / 2));
      assert.ok(result.suspectedCauses.includes('identity_key_distribution'));
      assert.ok(result.suspectedCauses.includes('regional_dns_path'));
      assert.ok(result.suspectedCauses.includes('downstream_dependency_timeout'));
      assert.ok(result.runbookOwners.includes('identity-oncall'));
      assert.ok(
        result.immediateActions.includes(
          'verify the newest signing keys and cert chain in every region',
        ),
      );
    },
  };
}

function createFraudTransactions(config) {
  return Array.from({ length: config.transactionCount }, (_, index) => ({
    id: `txn_${index}`,
    accountId: `acct_${index % config.accountCount}`,
    entityId: `ent_${index % config.entityCount}`,
    cardFingerprint: `card_${index % config.cardCount}`,
    amount: 320 + ((index * 37) % 900),
    outcome: index % 5 === 0 ? 'declined' : 'approved',
    ipAddress: `198.51.100.${10 + (index % 5)}`,
    email:
      index % 3 === 0
        ? `user+${index}@relaymail.test`
        : `buyer-test-${index}@relaymail.test`,
    deviceId: `dev_${Math.floor(index / 2) % Math.max(4, Math.floor(config.accountCount / 2))}`,
    timestamp: `2026-04-${String((index % 9) + 1).padStart(2, '0')}T10:00:00Z`,
  }));
}

function createFraudScenario(sizeName) {
  const config = FRAUD_SIZE_CONFIGS[sizeName];
  const transactions = createFraudTransactions(config);
  const inputs = {
    caseId: `fraud_case_${sizeName}`,
    lookbackDays: 21,
  };

  return {
    metricName: `ptc_fraud_investigation_${sizeName}`,
    laneId: 'fraud_investigation',
    sizeName,
    sourceFile: 'analytics/investigate-fraud-ring.js',
    source: loadExampleSource('analytics/investigate-fraud-ring.js'),
    inputs,
    shape: {
      sourceRef: 'examples/programmatic-tool-calls/analytics/investigate-fraud-ring.js',
      toolFamilyCount: 6,
      logicalPeakFanout: 4,
      finalAction: false,
      usesPromiseAll: true,
      usesMap: true,
      usesSet: true,
      scale: config,
    },
    createCapabilities() {
      return {
        load_alert_case(caseId) {
          return {
            id: caseId,
            queue: 'card_fraud',
            primaryReason: 'velocity_spike',
            flaggedAccountIds: transactions.slice(0, 3).map((entry) => entry.accountId),
          };
        },
        list_related_transactions() {
          return transactions.map((entry) => ({ ...entry }));
        },
        fetch_device_clusters(accountIds) {
          const filtered = [...new Set(accountIds)].sort();
          const clusters = [];
          for (let index = 0; index < filtered.length; index += 3) {
            const accounts = filtered.slice(index, index + 3);
            if (accounts.length === 0) {
              continue;
            }
            clusters.push({
              clusterId: `cluster_${index / 3}`,
              accounts,
              devices: accounts.map((accountId, deviceIndex) => `device_${accountId}_${deviceIndex}`),
              riskLabel: index % 2 === 0 ? 'high' : 'medium',
            });
          }
          return clusters;
        },
        fetch_chargeback_history(cardFingerprints) {
          return [...new Set(cardFingerprints)].sort().map((cardFingerprint, index) => ({
            cardFingerprint,
            chargebackRate: index % 2 === 0 ? 0.24 : 0.07,
            disputedAmount: 4000 + index * 175,
            count: 2 + (index % 4),
          }));
        },
        lookup_identity_signals(entityIds) {
          return [...new Set(entityIds)].sort().map((entityId, index) => ({
            entityId,
            syntheticRisk: index % 2 === 0 ? 0.91 : 0.54,
            watchlistHits: index % 3 === 0 ? 1 : 0,
            documentMismatch: index % 4 === 0,
          }));
        },
        search_internal_notes() {
          return [
            {
              source: 'case_history',
              body: 'Prior refund mule language linked this cohort to synthetic identity abuse.',
            },
            {
              source: 'risk_review',
              body: 'Synthetic identity signatures appeared again with collusion indicators.',
            },
            {
              source: 'ops',
              body: 'Analysts noted likely collusion between linked accounts.',
            },
          ];
        },
      };
    },
    assertResult(result) {
      assert.equal(result.caseId, inputs.caseId);
      assert.equal(result.transactionCount, config.transactionCount);
      assert.equal(result.recommendedDisposition, 'escalate_to_fraud_ops');
      assert.ok(result.accountCount >= Math.ceil(config.accountCount / 2));
      assert.ok(result.suspiciousTransactionCount > 0);
      assert.ok(result.rapidReuseCount > 0);
      assert.ok(result.narrativeSignals.includes('refund_mule_language'));
      assert.ok(result.narrativeSignals.includes('synthetic_identity_language'));
      assert.ok(result.narrativeSignals.includes('collusion_language'));
      assert.ok(result.escalationReasons.includes('historical_case_overlap'));
      assert.ok(result.escalationReasons.includes('shared_infrastructure'));
    },
  };
}

function createVendorScenario(sizeName) {
  const config = VENDOR_SIZE_CONFIGS[sizeName];
  const requiredFrameworks = [
    'SOC2',
    'ISO27001',
    'DPA',
    'PCI',
    'GDPR',
    'CCPA',
    'HIPAA',
  ].slice(0, config.frameworkCount);
  const inputs = {
    vendorId: `vendor_${sizeName}`,
    reviewCycleId: `vendor_review_${sizeName}`,
    requiredFrameworks,
  };

  return {
    metricName: `ptc_vendor_review_${sizeName}`,
    laneId: 'vendor_review',
    sizeName,
    sourceFile: 'workflows/vendor-compliance-renewal.js',
    source: loadExampleSource('workflows/vendor-compliance-renewal.js'),
    inputs,
    shape: {
      sourceRef: 'examples/programmatic-tool-calls/workflows/vendor-compliance-renewal.js',
      toolFamilyCount: 5,
      logicalPeakFanout: 4,
      finalAction: true,
      usesPromiseAll: true,
      usesMap: false,
      usesSet: false,
      scale: config,
    },
    createCapabilities() {
      return {
        fetch_vendor_master(vendorId) {
          return {
            id: vendorId,
            name: `Vendor ${sizeName}`,
            serviceTier: 'high',
            hostsCustomerData: true,
            primaryCountry: 'US',
          };
        },
        fetch_control_evidence() {
          const evidence = [];
          for (const [index, framework] of requiredFrameworks.entries()) {
            evidence.push({
              framework,
              type: 'attestation',
              status: index < Math.max(1, requiredFrameworks.length - 2) ? 'current' : 'expired',
              ageDays: index < Math.max(1, requiredFrameworks.length - 2) ? 120 : 480,
            });
            if (index % 2 === 0) {
              evidence.push({
                framework,
                type: 'penetration_test',
                status: 'current',
                ageDays: 90 + index * 10,
              });
            }
          }
          return evidence;
        },
        fetch_data_flow_inventory() {
          return Array.from({ length: config.flowCount }, (_, index) => ({
            system: `system_${index}`,
            originCountry: 'US',
            destinationCountry: index % 3 === 0 ? 'DE' : 'US',
            dataClasses:
              index % 2 === 0 ? ['customer_pii', 'security_logs'] : ['billing_records'],
          }));
        },
        fetch_subprocessor_list() {
          return Array.from({ length: config.subprocessorCount }, (_, index) => ({
            name: `subprocessor_${index}`,
            country: index % 3 === 0 ? 'DE' : 'US',
            hasDpa: index % 4 !== 0,
            countryRisk: index % 5 === 0 ? 'medium' : 'low',
          }));
        },
        file_vendor_review(payload) {
          return {
            reviewRecordId: `review_${sizeName}_${payload.missingFrameworks.length}_${payload.riskySubprocessors.length}`,
            state: 'filed',
          };
        },
      };
    },
    assertResult(result) {
      assert.equal(result.vendorId, inputs.vendorId);
      assert.equal(result.reviewCycleId, inputs.reviewCycleId);
      assert.equal(result.state, 'filed');
      assert.equal(result.recommendedDecision, 'manual_review');
      assert.ok(result.reviewRecordId.startsWith(`review_${sizeName}_`));
      assert.ok(result.missingFrameworks.length > 0);
      assert.ok(result.riskySubprocessors.length > 0);
      assert.ok(result.crossBorderFlows.length > 0);
    },
  };
}

function createVendorDurableScenario(sizeName) {
  const config = VENDOR_SIZE_CONFIGS[sizeName];
  const requiredFrameworks = [
    'SOC2',
    'ISO27001',
    'DPA',
    'PCI',
    'GDPR',
    'CCPA',
    'HIPAA',
  ].slice(0, config.frameworkCount);
  const inputs = {
    vendorId: `vendor_${sizeName}`,
    reviewCycleId: `vendor_review_${sizeName}`,
    requiredFrameworks,
  };

  return {
    metricName: `ptc_vendor_review_durable_${sizeName}`,
    laneId: 'vendor_review_durable',
    sizeName,
    sourceFile: 'workflows/vendor-compliance-renewal-durable.js',
    source: loadExampleSource('workflows/vendor-compliance-renewal-durable.js'),
    inputs,
    shape: {
      sourceRef: 'examples/programmatic-tool-calls/workflows/vendor-compliance-renewal-durable.js',
      toolFamilyCount: 6,
      logicalPeakFanout: 4,
      finalAction: true,
      durableBoundary: true,
      usesPromiseAll: true,
      usesMap: false,
      usesSet: false,
      scale: config,
    },
    createCapabilities() {
      return {
        fetch_vendor_master(vendorId) {
          return {
            id: vendorId,
            name: `Vendor ${sizeName}`,
            serviceTier: 'high',
            hostsCustomerData: true,
            primaryCountry: 'US',
          };
        },
        fetch_control_evidence() {
          const evidence = [];
          for (const [index, framework] of requiredFrameworks.entries()) {
            evidence.push({
              framework,
              type: 'attestation',
              status: index < Math.max(1, requiredFrameworks.length - 2) ? 'current' : 'expired',
              ageDays: index < Math.max(1, requiredFrameworks.length - 2) ? 120 : 480,
            });
            if (index % 2 === 0) {
              evidence.push({
                framework,
                type: 'penetration_test',
                status: 'current',
                ageDays: 90 + index * 10,
              });
            }
          }
          return evidence;
        },
        fetch_data_flow_inventory() {
          return Array.from({ length: config.flowCount }, (_, index) => ({
            system: `system_${index}`,
            originCountry: 'US',
            destinationCountry: index % 3 === 0 ? 'DE' : 'US',
            dataClasses:
              index % 2 === 0 ? ['customer_pii', 'security_logs'] : ['billing_records'],
          }));
        },
        fetch_subprocessor_list() {
          return Array.from({ length: config.subprocessorCount }, (_, index) => ({
            name: `subprocessor_${index}`,
            country: index % 3 === 0 ? 'DE' : 'US',
            hasDpa: index % 4 !== 0,
            countryRisk: index % 5 === 0 ? 'medium' : 'low',
          }));
        },
        checkpoint_vendor_review(reviewBundle) {
          return {
            approved: true,
            reviewer: 'compliance-oncall',
            reason: `checkpointed ${reviewBundle.requiredFrameworks.length} required frameworks`,
          };
        },
        file_vendor_review(payload) {
          return {
            reviewRecordId: `review_durable_${sizeName}_${payload.missingFrameworks.length}_${payload.riskySubprocessors.length}`,
            state: 'filed',
          };
        },
      };
    },
    assertResult(result) {
      assert.equal(result.vendorId, inputs.vendorId);
      assert.equal(result.reviewCycleId, inputs.reviewCycleId);
      assert.equal(result.state, 'filed');
      assert.ok(result.reviewRecordId.startsWith(`review_durable_${sizeName}_`));
      assert.equal(result.recommendedDecision, 'manual_review');
      assert.equal(result.approvalReviewer, 'compliance-oncall');
      assert.ok(result.missingFrameworks.length > 0);
      assert.ok(result.riskySubprocessors.length > 0);
      assert.ok(result.crossBorderFlows.length > 0);
      assert.ok(result.staleEvidence.length > 0);
    },
  };
}

function createPtcScenarios() {
  const scenarios = {};
  for (const sizeName of ['small', 'medium', 'large']) {
    const websiteScenario = createWebsiteDemoScenario(sizeName);
    scenarios[websiteScenario.metricName] = websiteScenario;

    const incidentScenario = createIncidentScenario(sizeName);
    scenarios[incidentScenario.metricName] = incidentScenario;

    const fraudScenario = createFraudScenario(sizeName);
    scenarios[fraudScenario.metricName] = fraudScenario;

    const vendorScenario = createVendorScenario(sizeName);
    scenarios[vendorScenario.metricName] = vendorScenario;
  }
  return scenarios;
}

function createDurablePtcScenarios() {
  const scenarios = {};
  for (const sizeName of ['small', 'medium', 'large']) {
    const scenario = createVendorDurableScenario(sizeName);
    scenarios[scenario.metricName] = scenario;
  }
  return scenarios;
}

module.exports = {
  PTC_WEIGHTS,
  createCapabilityTransferProbe,
  createDurablePtcScenarios,
  createPtcScenarios,
  structuredByteLength,
  summarizePtcWeightedScore,
};
