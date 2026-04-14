'use strict';

const fs = require('node:fs');
const path = require('node:path');

const {
  HEADLINE_USE_CASE_IDS,
  USE_CASE_METADATA,
  metricNameForUseCase,
} = require('./ptc-portfolio.ts');

const EXAMPLES_ROOT = path.join(__dirname, '..', 'examples', 'programmatic-tool-calls');

function loadExampleSource(relativePath) {
  return fs.readFileSync(path.join(EXAMPLES_ROOT, relativePath), 'utf8');
}

const HEADLINE_SEED_CONFIGS = Object.freeze({
  analytics_revenue_quality: {
    sourceFile: 'analytics/analyze-revenue-quality.js',
    inputs: {
      quarter: '2026-Q2-skew',
      materialityThreshold: 250000,
    },
    skewPatterns: [
      'hotspot_cardinality_skew',
      'duplicate_heavy_collections',
      'longer_deal_reason_strings',
    ],
    createCapabilities() {
      return {
        list_business_units() {
          return [
            {
              id: 'bu_enterprise',
              name: 'Enterprise',
              segment: 'enterprise',
              owner: 'vp-enterprise',
            },
            {
              id: 'bu_midmarket',
              name: 'Mid-Market',
              segment: 'mid_market',
              owner: 'vp-midmarket',
            },
            {
              id: 'bu_selfserve',
              name: 'Self-Serve',
              segment: 'self_serve',
              owner: 'gm-selfserve',
            },
            {
              id: 'bu_strategic',
              name: 'Strategic Accounts',
              segment: 'enterprise',
              owner: 'vp-strategic',
            },
          ];
        },
        load_unit_actuals(unitId) {
          return {
            bu_enterprise: {
              recognizedRevenue: 7100000,
              deferredRevenue: 1840000,
              churnedArr: 520000,
              dso: 71,
              collectionsAtRisk: 930000,
            },
            bu_midmarket: {
              recognizedRevenue: 4020000,
              deferredRevenue: 980000,
              churnedArr: 280000,
              dso: 52,
              collectionsAtRisk: 220000,
            },
            bu_selfserve: {
              recognizedRevenue: 2260000,
              deferredRevenue: 310000,
              churnedArr: 160000,
              dso: 27,
              collectionsAtRisk: 62000,
            },
            bu_strategic: {
              recognizedRevenue: 1950000,
              deferredRevenue: 470000,
              churnedArr: 210000,
              dso: 63,
              collectionsAtRisk: 390000,
            },
          }[unitId];
        },
        load_unit_forecast(unitId) {
          return {
            bu_enterprise: {
              committedRevenue: 7800000,
              stretchRevenue: 8400000,
              pipelineCoverage: 1.03,
            },
            bu_midmarket: {
              committedRevenue: 4300000,
              stretchRevenue: 4520000,
              pipelineCoverage: 1.17,
            },
            bu_selfserve: {
              committedRevenue: 2190000,
              stretchRevenue: 2360000,
              pipelineCoverage: 1.32,
            },
            bu_strategic: {
              committedRevenue: 2440000,
              stretchRevenue: 2710000,
              pipelineCoverage: 1.04,
            },
          }[unitId];
        },
        load_unit_deal_changes(unitId) {
          return {
            bu_enterprise: [
              {
                opportunityId: 'opp_701',
                movement: 'slipped',
                amount: 540000,
                reason: 'global_security_review_and_counterparty_risk_committee_delay',
                account: 'Northstar Global Bank',
              },
              {
                opportunityId: 'opp_702',
                movement: 'slipped',
                amount: 410000,
                reason: 'multiregion_data_processing_addendum_redline_cycle',
                account: 'Helios Capital Partners',
              },
              {
                opportunityId: 'opp_703',
                movement: 'expanded',
                amount: 290000,
                reason: 'late_stage_enterprise_identity_bundle_attach',
                account: 'Stellar Health',
              },
            ],
            bu_midmarket: [
              {
                opportunityId: 'opp_721',
                movement: 'pushed',
                amount: 180000,
                reason: 'budget_reapproval',
                account: 'Canvas Retail',
              },
              {
                opportunityId: 'opp_722',
                movement: 'pulled_forward',
                amount: 210000,
                reason: 'annual_prepay',
                account: 'Beacon Analytics',
              },
            ],
            bu_selfserve: [
              {
                opportunityId: 'opp_731',
                movement: 'expanded',
                amount: 92000,
                reason: 'upsell',
                account: 'Long-tail cohort',
              },
            ],
            bu_strategic: [
              {
                opportunityId: 'opp_741',
                movement: 'slipped',
                amount: 330000,
                reason: 'coordinated_procurement_security_and_legal_approval_rework',
                account: 'Atlas Manufacturing Group',
              },
              {
                opportunityId: 'opp_742',
                movement: 'expanded',
                amount: 260000,
                reason: 'multi_year_ai_governance_attach',
                account: 'Atlas Manufacturing Group',
              },
            ],
          }[unitId];
        },
        list_collection_risks() {
          return [
            {
              unitId: 'bu_enterprise',
              accountId: 'acct_990',
              balance: 260000,
              reason: 'invoice_dispute_enterprise_security_workstream',
              daysPastDue: 48,
            },
            {
              unitId: 'bu_enterprise',
              accountId: 'acct_991',
              balance: 210000,
              reason: 'budget_hold_enterprise_expansion',
              daysPastDue: 33,
            },
            {
              unitId: 'bu_enterprise',
              accountId: 'acct_992',
              balance: 180000,
              reason: 'invoice_dispute_enterprise_security_workstream',
              daysPastDue: 35,
            },
            {
              unitId: 'bu_strategic',
              accountId: 'acct_993',
              balance: 310000,
              reason: 'renewal_committee_delay',
              daysPastDue: 41,
            },
            {
              unitId: 'bu_midmarket',
              accountId: 'acct_994',
              balance: 90000,
              reason: 'purchase_order_lag',
              daysPastDue: 16,
            },
          ];
        },
      };
    },
  },
  analytics_fraud_ring: {
    sourceFile: 'analytics/investigate-fraud-ring.js',
    inputs: {
      caseId: 'fraud_case_441_skew',
      lookbackDays: 30,
    },
    skewPatterns: [
      'hotspot_cardinality_skew',
      'duplicate_heavy_joins',
      'noisier_identity_and_note_strings',
    ],
    createCapabilities() {
      return {
        load_alert_case(caseId) {
          return {
            id: caseId,
            queue: 'card_fraud',
            primaryReason: 'velocity_spike',
            flaggedAccountIds: ['acct_1', 'acct_2', 'acct_3', 'acct_4', 'acct_5'],
          };
        },
        list_related_transactions() {
          return [
            {
              id: 'txn_1',
              accountId: 'acct_1',
              entityId: 'ent_a',
              cardFingerprint: 'card_x',
              amount: 1400,
              outcome: 'approved',
              ipAddress: '198.51.100.10',
              email: 'maria+promo@fastmail.test',
              deviceId: 'dev_1',
              timestamp: '2026-04-09T10:11:00Z',
            },
            {
              id: 'txn_2',
              accountId: 'acct_2',
              entityId: 'ent_b',
              cardFingerprint: 'card_y',
              amount: 990,
              outcome: 'approved',
              ipAddress: '198.51.100.10',
              email: 'buyer-test@relaymail.test',
              deviceId: 'dev_2',
              timestamp: '2026-04-09T10:14:00Z',
            },
            {
              id: 'txn_3',
              accountId: 'acct_3',
              entityId: 'ent_c',
              cardFingerprint: 'card_x',
              amount: 1180,
              outcome: 'declined',
              ipAddress: '198.51.100.10',
              email: 'ops-signal@relaymail.test',
              deviceId: 'dev_3',
              timestamp: '2026-04-09T10:16:00Z',
            },
            {
              id: 'txn_4',
              accountId: 'acct_4',
              entityId: 'ent_d',
              cardFingerprint: 'card_x',
              amount: 1640,
              outcome: 'approved',
              ipAddress: '198.51.100.10',
              email: 'northwind+gift@relaymail.test',
              deviceId: 'dev_4',
              timestamp: '2026-04-09T10:18:00Z',
            },
            {
              id: 'txn_5',
              accountId: 'acct_5',
              entityId: 'ent_e',
              cardFingerprint: 'card_z',
              amount: 780,
              outcome: 'approved',
              ipAddress: '203.0.113.7',
              email: 'steady.customer@example.test',
              deviceId: 'dev_3',
              timestamp: '2026-04-09T10:20:00Z',
            },
            {
              id: 'txn_6',
              accountId: 'acct_1',
              entityId: 'ent_a',
              cardFingerprint: 'card_x',
              amount: 920,
              outcome: 'approved',
              ipAddress: '198.51.100.10',
              email: 'maria+retry@fastmail.test',
              deviceId: 'dev_1',
              timestamp: '2026-04-09T10:21:00Z',
            },
            {
              id: 'txn_7',
              accountId: 'acct_6',
              entityId: 'ent_f',
              cardFingerprint: 'card_y',
              amount: 1110,
              outcome: 'declined',
              ipAddress: '198.51.100.10',
              email: 'shopper-test-7@relaymail.test',
              deviceId: 'dev_2',
              timestamp: '2026-04-09T10:24:00Z',
            },
            {
              id: 'txn_8',
              accountId: 'acct_7',
              entityId: 'ent_g',
              cardFingerprint: 'card_hot',
              amount: 630,
              outcome: 'approved',
              ipAddress: '198.51.100.88',
              email: 'legit.customer@example.test',
              deviceId: 'dev_8',
              timestamp: '2026-04-09T10:30:00Z',
            },
          ];
        },
        fetch_device_clusters() {
          return [
            {
              clusterId: 'cluster_77',
              accounts: ['acct_1', 'acct_2', 'acct_3', 'acct_4', 'acct_6'],
              devices: ['dev_1', 'dev_2', 'dev_3', 'dev_4'],
              riskLabel: 'dense_cross_account_overlap',
            },
            {
              clusterId: 'cluster_88',
              accounts: ['acct_5', 'acct_7'],
              devices: ['dev_3', 'dev_8'],
              riskLabel: 'mixed_household_overlap',
            },
          ];
        },
        fetch_chargeback_history() {
          return [
            {
              cardFingerprint: 'card_x',
              chargebackRate: 0.31,
              disputedAmount: 9400,
              count: 6,
            },
            {
              cardFingerprint: 'card_y',
              chargebackRate: 0.21,
              disputedAmount: 4200,
              count: 3,
            },
            {
              cardFingerprint: 'card_hot',
              chargebackRate: 0.02,
              disputedAmount: 160,
              count: 1,
            },
          ];
        },
        lookup_identity_signals() {
          return [
            { entityId: 'ent_a', syntheticRisk: 0.91, watchlistHits: 1, documentMismatch: true },
            { entityId: 'ent_b', syntheticRisk: 0.63, watchlistHits: 1, documentMismatch: false },
            { entityId: 'ent_c', syntheticRisk: 0.88, watchlistHits: 0, documentMismatch: true },
            { entityId: 'ent_d', syntheticRisk: 0.84, watchlistHits: 0, documentMismatch: true },
            { entityId: 'ent_e', syntheticRisk: 0.22, watchlistHits: 0, documentMismatch: false },
            { entityId: 'ent_f', syntheticRisk: 0.79, watchlistHits: 2, documentMismatch: true },
            { entityId: 'ent_g', syntheticRisk: 0.08, watchlistHits: 0, documentMismatch: false },
          ];
        },
        search_internal_notes() {
          return [
            {
              source: 'case_120',
              body: 'Older fraud case noted REFUND MULE coordination plus synthetic identity reuse across promo cohorts.',
            },
            {
              source: 'case_121',
              body: 'Analyst write-up mentioned cross-account collusion, device sharing, and synthetic identity layering.',
            },
          ];
        },
      };
    },
  },
  'triage-multi-region-auth-outage': {
    sourceFile: 'operations/triage-multi-region-auth-outage.js',
    inputs: {
      incidentId: 'inc_9041_skew',
      service: 'auth-gateway',
      regions: ['us-east-1', 'us-west-2', 'eu-west-1', 'ap-southeast-1', 'sa-east-1'],
    },
    skewPatterns: [
      'duplicate_heavy_alert_dedupe',
      'regional_hotspot_skew',
      'longer_error_sample_strings',
    ],
    createCapabilities() {
      return {
        load_incident_timeline() {
          return [
            {
              ts: '2026-04-11T07:06:00Z',
              kind: 'deploy',
              note: 'auth-gateway deploy chg_6012 reached 100 percent in us-west-2',
            },
            {
              ts: '2026-04-11T07:09:00Z',
              kind: 'symptom',
              note: 'certificate refresh lag started after the deploy and spread into the signer lookup path',
            },
            {
              ts: '2026-04-11T07:17:00Z',
              kind: 'symptom',
              note: 'regional dns retries climbed after the certificate rollout',
            },
            {
              ts: '2026-04-11T07:19:00Z',
              kind: 'mitigation',
              note: 'rollback remains under consideration for the newest auth deployment',
            },
          ];
        },
        list_regional_alerts(_service, region) {
          const byRegion = {
            'us-east-1': [
              {
                severity: 'high',
                signal: 'elevated_401s',
                fingerprint: 'auth-401-1',
                summary: '401 spike from web sessions',
              },
            ],
            'us-west-2': [
              {
                severity: 'critical',
                signal: 'token_validation_failure',
                fingerprint: 'auth-jwks-1',
                summary: 'Token validation failed against newest signing key',
              },
              {
                severity: 'critical',
                signal: 'token_validation_failure',
                fingerprint: 'auth-jwks-1',
                summary: 'Duplicate page for the same failure family',
              },
              {
                severity: 'high',
                signal: 'certificate_chain_error',
                fingerprint: 'auth-cert-3',
                summary: 'Certificate chain validation retried repeatedly',
              },
            ],
            'eu-west-1': [
              {
                severity: 'high',
                signal: 'dns_lookup_errors',
                fingerprint: 'auth-dns-7',
                summary: 'Resolver errors between edge and identity provider',
              },
            ],
            'ap-southeast-1': [
              {
                severity: 'high',
                signal: 'token_validation_failure',
                fingerprint: 'auth-jwks-1',
                summary: 'Shared token validation failure family echoed in APAC',
              },
              {
                severity: 'high',
                signal: 'dns_lookup_errors',
                fingerprint: 'auth-dns-19',
                summary: 'DNS timeout to signer endpoint after repeated retries',
              },
            ],
            'sa-east-1': [
              {
                severity: 'medium',
                signal: 'latency_drift',
                fingerprint: 'auth-latency-2',
                summary: 'Latency climbed but stayed under major thresholds',
              },
            ],
          };
          return byRegion[region] || [];
        },
        fetch_service_slo(_service, region) {
          return {
            'us-east-1': { region, availability: 99.18, latencyP95: 860, errorRate: 0.026 },
            'us-west-2': { region, availability: 96.92, latencyP95: 1490, errorRate: 0.122 },
            'eu-west-1': { region, availability: 98.71, latencyP95: 1020, errorRate: 0.054 },
            'ap-southeast-1': { region, availability: 97.84, latencyP95: 1170, errorRate: 0.071 },
            'sa-east-1': { region, availability: 99.66, latencyP95: 720, errorRate: 0.012 },
          }[region];
        },
        search_error_samples(_service, _incidentId, region) {
          const byRegion = {
            'us-east-1': [
              'token refresh timeout while calling identity upstream after certificate propagation lag',
              'rate limit encountered while minting replacement token during replay',
            ],
            'us-west-2': [
              'jwks cache miss caused certificate lookup timeout after deploy',
              'token validation failed after certificate rotation and signer mismatch',
            ],
            'eu-west-1': [
              'dns timeout talking to signer endpoint through the regional resolver path',
            ],
            'ap-southeast-1': [
              'dns timeout and certificate mismatch while contacting signer endpoint in apac',
              'token validation retry loop after delayed certificate bundle refresh',
            ],
            'sa-east-1': [
              'latency regression without hard error signals',
            ],
          };
          return byRegion[region] || [];
        },
        get_mitigation_runbook() {
          return {
            owners: ['identity-oncall', 'traffic-manager'],
            immediateActions: [
              'confirm blast radius by region',
              'pause rollout progression',
              'prepare rollback and traffic shift',
            ],
            rollbackTriggers: [
              'token validation failures continue after key sync',
            ],
          };
        },
      };
    },
  },
  'analyze-queue-backlog-regression': {
    sourceFile: 'operations/analyze-queue-backlog-regression.js',
    inputs: {
      pipeline: 'fulfillment-events',
      regions: ['us-central1', 'europe-west1', 'asia-south1'],
    },
    skewPatterns: [
      'hotspot_cardinality_skew',
      'larger_intermediate_payloads',
      'noisier_dead_letter_strings',
    ],
    createCapabilities() {
      return {
        list_queue_shards(_pipeline, region) {
          return {
            'us-central1': [
              { shardId: 'a', workerPool: 'workers-a' },
              { shardId: 'b', workerPool: 'workers-a' },
              { shardId: 'c', workerPool: 'workers-b' },
            ],
            'europe-west1': [
              { shardId: 'd', workerPool: 'workers-c' },
              { shardId: 'e', workerPool: 'workers-c' },
            ],
            'asia-south1': [
              { shardId: 'f', workerPool: 'workers-a' },
              { shardId: 'g', workerPool: 'workers-b' },
            ],
          }[region] || [];
        },
        fetch_shard_metrics(_pipeline, region, shardId) {
          return {
            'us-central1:a': {
              depth: 6100,
              oldestAgeSec: 1420,
              inflowPerMin: 980,
              outflowPerMin: 430,
              retryRate: 0.16,
            },
            'us-central1:b': {
              depth: 4200,
              oldestAgeSec: 970,
              inflowPerMin: 760,
              outflowPerMin: 410,
              retryRate: 0.11,
            },
            'us-central1:c': {
              depth: 1600,
              oldestAgeSec: 260,
              inflowPerMin: 420,
              outflowPerMin: 405,
              retryRate: 0.03,
            },
            'europe-west1:d': {
              depth: 3000,
              oldestAgeSec: 760,
              inflowPerMin: 610,
              outflowPerMin: 430,
              retryRate: 0.09,
            },
            'europe-west1:e': {
              depth: 2100,
              oldestAgeSec: 520,
              inflowPerMin: 540,
              outflowPerMin: 500,
              retryRate: 0.05,
            },
            'asia-south1:f': {
              depth: 4700,
              oldestAgeSec: 1210,
              inflowPerMin: 790,
              outflowPerMin: 390,
              retryRate: 0.13,
            },
            'asia-south1:g': {
              depth: 1900,
              oldestAgeSec: 340,
              inflowPerMin: 360,
              outflowPerMin: 355,
              retryRate: 0.04,
            },
          }[`${region}:${shardId}`];
        },
        fetch_worker_pool_status(workerPool) {
          return {
            'workers-a': {
              pool: workerPool,
              saturation: 0.97,
              unavailableWorkers: 4,
              queuedRestarts: 3,
            },
            'workers-b': {
              pool: workerPool,
              saturation: 0.66,
              unavailableWorkers: 1,
              queuedRestarts: 0,
            },
            'workers-c': {
              pool: workerPool,
              saturation: 0.92,
              unavailableWorkers: 2,
              queuedRestarts: 2,
            },
          }[workerPool];
        },
        sample_dead_letters(_pipeline, region, shardId) {
          return {
            'us-central1:a': [
              {
                code: 'THROTTLE',
                message: 'downstream throttle while reserving stock for the fulfillment fanout batch',
              },
              {
                code: 'POISON',
                message: 'poison message repeated after duplicate replay and schema drift',
              },
            ],
            'us-central1:b': [
              {
                code: 'TIMEOUT',
                message: 'timeout while writing invoice event into the audit sink',
              },
              {
                code: 'SERIALIZE',
                message: 'serialize mismatch after payload version drift',
              },
            ],
            'europe-west1:d': [
              {
                code: 'SCHEMA',
                message: 'schema mismatch in serialization path during recovery',
              },
            ],
            'asia-south1:f': [
              {
                code: 'DUPLICATE',
                message: 'duplicate replay loop after consumer timeout and throttle backoff',
              },
              {
                code: 'TIMEOUT',
                message: 'timeout while draining the dependency queue in regional failover',
              },
            ],
          }[`${region}:${shardId}`] || [];
        },
        get_capacity_plan() {
          return {
            maxHealthyDepth: 3000,
            maxHealthyAgeSec: 600,
            escalationActions: [
              'page the owning worker pool',
              'pause the noisiest producer if backlog continues to climb',
            ],
          };
        },
      };
    },
  },
  'vendor-compliance-renewal': {
    sourceFile: 'workflows/vendor-compliance-renewal.js',
    inputs: {
      vendorId: 'vendor_polaris_skew',
      reviewCycleId: 'vendor_review_2026_q2_skew',
      requiredFrameworks: ['SOC2', 'ISO27001', 'DPA', 'PCI', 'GDPR'],
    },
    skewPatterns: [
      'larger_intermediate_payloads',
      'duplicate_heavy_evidence_sets',
      'lower_signal_to_noise',
    ],
    createCapabilities() {
      return {
        fetch_vendor_master(vendorId) {
          return {
            id: vendorId,
            name: 'Polaris Data Fabric International',
            serviceTier: 'critical',
            hostsCustomerData: true,
            primaryCountry: 'US',
          };
        },
        fetch_control_evidence() {
          return [
            { framework: 'SOC2', type: 'attestation', status: 'current', ageDays: 130 },
            { framework: 'SOC2', type: 'penetration_test', status: 'current', ageDays: 95 },
            { framework: 'ISO27001', type: 'attestation', status: 'expired', ageDays: 490 },
            { framework: 'ISO27001', type: 'penetration_test', status: 'current', ageDays: 112 },
            { framework: 'DPA', type: 'contract', status: 'current', ageDays: 41 },
            { framework: 'PCI', type: 'attestation', status: 'expired', ageDays: 510 },
            { framework: 'PCI', type: 'evidence_note', status: 'current', ageDays: 17 },
            { framework: 'GDPR', type: 'attestation', status: 'current', ageDays: 280 },
            { framework: 'GDPR', type: 'processor_register', status: 'current', ageDays: 45 },
          ];
        },
        fetch_data_flow_inventory() {
          return [
            {
              system: 'model-trainer',
              originCountry: 'US',
              destinationCountry: 'DE',
              dataClasses: ['customer_pii', 'usage_metadata'],
            },
            {
              system: 'support-index',
              originCountry: 'US',
              destinationCountry: 'US',
              dataClasses: ['support_ticket'],
            },
            {
              system: 'reporting-cache',
              originCountry: 'US',
              destinationCountry: 'BR',
              dataClasses: ['customer_pii', 'billing_history'],
            },
            {
              system: 'event-lake',
              originCountry: 'US',
              destinationCountry: 'DE',
              dataClasses: ['usage_metadata'],
            },
          ];
        },
        fetch_subprocessor_list() {
          return [
            {
              name: 'EuroCompute GmbH',
              country: 'DE',
              hasDpa: true,
              countryRisk: 'low',
            },
            {
              name: 'FastLabel Ops',
              country: 'BR',
              hasDpa: false,
              countryRisk: 'medium',
            },
            {
              name: 'Archive Transit Partners',
              country: 'US',
              hasDpa: true,
              countryRisk: 'low',
            },
            {
              name: 'Realtime Insights APAC',
              country: 'SG',
              hasDpa: false,
              countryRisk: 'medium',
            },
          ];
        },
        file_vendor_review(payload) {
          return {
            reviewRecordId: `vendor_record_skew_${payload.missingFrameworks.length}_${payload.riskySubprocessors.length}`,
            state: payload.recommendedDecision,
          };
        },
      };
    },
  },
  'privacy-erasure-orchestration': {
    sourceFile: 'workflows/privacy-erasure-orchestration.js',
    inputs: {
      requestId: 'dsr_440_skew',
      customerId: 'cust_991',
      deadlineIso: '2026-05-03T00:00:00Z',
      jurisdictions: ['GDPR', 'CCPA', 'LGPD'],
    },
    skewPatterns: [
      'writeback_fanout_skew',
      'retention_hold_hotspot',
      'larger_intermediate_payloads',
    ],
    createCapabilities() {
      let queuedCount = 0;
      let eventCount = 0;
      return {
        fetch_privacy_request(requestId) {
          return {
            id: requestId,
            type: 'erasure',
            subjectEmail: 'patient-admin+privacy@northwind.test',
            requestedDataClasses: ['customer_pii', 'usage_metadata', 'billing_history'],
          };
        },
        list_systems_of_record() {
          return [
            {
              system: 'accounts',
              recordType: 'identity_profile',
              region: 'US',
              dataClasses: ['customer_pii'],
            },
            {
              system: 'billing',
              recordType: 'invoice_record',
              region: 'US',
              dataClasses: ['billing_history'],
            },
            {
              system: 'warehouse',
              recordType: 'event_archive',
              region: 'EU',
              dataClasses: ['usage_metadata'],
            },
            {
              system: 'analytics',
              recordType: 'behavioral_profile',
              region: 'US',
              dataClasses: ['customer_pii', 'usage_metadata'],
            },
            {
              system: 'support',
              recordType: 'ticket_transcript',
              region: 'US',
              dataClasses: ['customer_pii'],
            },
            {
              system: 'crm',
              recordType: 'contact_profile',
              region: 'US',
              dataClasses: ['customer_pii', 'billing_history'],
            },
          ];
        },
        fetch_retention_exceptions() {
          return [
            {
              system: 'billing',
              recordType: 'invoice_record',
              reason: 'tax_retention_hold',
              expiresInDays: 187,
            },
            {
              system: 'support',
              recordType: 'ticket_transcript',
              reason: 'legal_hold',
              expiresInDays: 45,
            },
          ];
        },
        queue_erasure_job(payload) {
          queuedCount += 1;
          return {
            jobId: `erase_skew_${queuedCount}`,
            system: payload.system,
            state: 'queued',
          };
        },
        record_case_event() {
          eventCount += 1;
          return {
            eventId: `evt_erase_skew_${eventCount}`,
          };
        },
        finalize_request(payload) {
          return {
            caseId: `privacy_case_skew_${payload.queuedJobCount}_${payload.blockedSystemCount}`,
            finalState: 'in_progress',
          };
        },
      };
    },
  },
});

function createHeadlineSeedScenarioDefinitions() {
  return Object.fromEntries(
    Object.entries(HEADLINE_SEED_CONFIGS).map(([useCaseId, config]) => {
      const metadata = USE_CASE_METADATA[useCaseId];
      if (!metadata) {
        throw new Error(`Missing phase-2 metadata for headline seed ${useCaseId}`);
      }
      const metricName = metricNameForUseCase(useCaseId, 'medium', 'skewed');
      return [metricName, {
        metricName,
        laneId: useCaseId,
        category: metadata.category,
        sizeName: 'medium',
        seedName: 'skewed',
        nominalMetricName: metricNameForUseCase(useCaseId),
        sourceFile: config.sourceFile,
        source: loadExampleSource(config.sourceFile),
        inputs: structuredClone(config.inputs),
        skewPatterns: [...config.skewPatterns],
        shape: {
          sourceRef: `examples/programmatic-tool-calls/${config.sourceFile}`,
          toolFamilyCount: Object.keys(config.createCapabilities()).length,
          logicalPeakFanout: metadata.logicalPeakFanout,
          compactionExpectation: metadata.compactionExpectation,
          ...metadata.shapes,
        },
        createCapabilities() {
          return config.createCapabilities();
        },
      }];
    }),
  );
}

function verifyHeadlineSeedIntegrity() {
  const ids = Object.keys(HEADLINE_SEED_CONFIGS);
  if (ids.length !== HEADLINE_USE_CASE_IDS.length) {
    throw new Error('Headline seed fixtures must cover every headline use-case id exactly once');
  }
  for (const useCaseId of HEADLINE_USE_CASE_IDS) {
    if (!HEADLINE_SEED_CONFIGS[useCaseId]) {
      throw new Error(`Missing skewed seed fixture for ${useCaseId}`);
    }
  }
}

verifyHeadlineSeedIntegrity();

module.exports = {
  HEADLINE_SEED_CONFIGS,
  createHeadlineSeedScenarioDefinitions,
};
