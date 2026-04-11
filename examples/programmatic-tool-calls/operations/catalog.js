'use strict';

const triageMultiRegionAuthOutageInputs = {
  incidentId: 'inc_9041',
  service: 'auth-gateway',
  regions: ['us-east-1', 'us-west-2', 'eu-west-1'],
};

const guardPaymentsRolloutInputs = {
  changeId: 'chg_6021',
  service: 'payments-api',
  canaryRegions: ['iad', 'dub', 'syd'],
};

const reconcileMarketplacePayoutsInputs = {
  payoutBatchId: 'batch_2026_04_11',
  sellerIds: ['seller_101', 'seller_202', 'seller_303'],
};

const stabilizeOncallHandoffInputs = {
  team: 'checkout-platform',
  shiftStart: '2026-04-11T00:00:00Z',
  shiftEnd: '2026-04-11T08:00:00Z',
};

const analyzeQueueBacklogRegressionInputs = {
  pipeline: 'fulfillment-events',
  regions: ['us-central1', 'europe-west1'],
};

const planDatabaseFailoverInputs = {
  cluster: 'orders-primary',
  incidentId: 'inc_9055',
};

const coordinateWarehouseExceptionInputs = {
  warehouseId: 'dfw-3',
  waveId: 'wave_771',
};

const assessGlobalDeploymentFreezeInputs = {
  since: '2026-04-10T20:00:00Z',
  services: ['checkout-api', 'payments-api', 'identity-api', 'pricing-api'],
};

module.exports = [
  {
    id: 'triage-multi-region-auth-outage',
    name: 'Triage Multi-Region Auth Outage',
    file: 'triage-multi-region-auth-outage.js',
    description:
      'Correlates regional auth alerts, SLO windows, error samples, and runbook guidance during a live incident.',
    inputs: triageMultiRegionAuthOutageInputs,
    options: {
      inputs: triageMultiRegionAuthOutageInputs,
      capabilities: {
        async load_incident_timeline() {
          return [
            {
              ts: '2026-04-11T07:08:00Z',
              kind: 'deploy',
              note: 'auth-gateway deploy chg_6012 reached 100 percent in us-west-2',
            },
            {
              ts: '2026-04-11T07:12:00Z',
              kind: 'symptom',
              note: 'certificate refresh warnings started after the deploy',
            },
            {
              ts: '2026-04-11T07:18:00Z',
              kind: 'mitigation',
              note: 'rollback considered but not yet executed',
            },
          ];
        },
        async list_regional_alerts(_service, region) {
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
            ],
            'eu-west-1': [
              {
                severity: 'high',
                signal: 'dns_lookup_errors',
                fingerprint: 'auth-dns-7',
                summary: 'Resolver errors between edge and identity provider',
              },
            ],
          };
          return byRegion[region] || [];
        },
        async fetch_service_slo(_service, region) {
          const byRegion = {
            'us-east-1': {
              region,
              availability: 99.21,
              latencyP95: 830,
              errorRate: 0.028,
            },
            'us-west-2': {
              region,
              availability: 97.04,
              latencyP95: 1410,
              errorRate: 0.119,
            },
            'eu-west-1': {
              region,
              availability: 98.74,
              latencyP95: 990,
              errorRate: 0.051,
            },
          };
          return byRegion[region];
        },
        async search_error_samples(_service, _incidentId, region) {
          const byRegion = {
            'us-east-1': [
              'token refresh timeout while calling identity upstream',
              'rate limit encountered while minting replacement token',
            ],
            'us-west-2': [
              'jwks cache miss caused certificate lookup timeout',
              'token validation failed after certificate rotation',
            ],
            'eu-west-1': [
              'dns timeout talking to signer endpoint',
            ],
          };
          return byRegion[region] || [];
        },
        async get_mitigation_runbook() {
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
      },
    },
  },
  {
    id: 'guard-payments-rollout',
    name: 'Guard Payments Rollout',
    file: 'guard-payments-rollout.js',
    description:
      'Evaluates a risky payments rollout against canary metrics, active incidents, and flag overrides before promotion.',
    inputs: guardPaymentsRolloutInputs,
    options: {
      inputs: guardPaymentsRolloutInputs,
      capabilities: {
        async load_change_request(changeId) {
          return {
            id: changeId,
            service: 'payments-api',
            version: '2026.04.11.6',
            risk: 'high',
            createdAt: '2026-04-11T08:10:00Z',
            summary: 'Card auth pipeline refactor',
            expectedBlastRadius: 'checkout and subscription renewals',
          };
        },
        async fetch_canary_metric(_service, region, metric) {
          const matrix = {
            iad: {
              error_rate: { metric, latest: 0.014, baseline: 0.006, status: 'degraded' },
              p95_latency: { metric, latest: 660, baseline: 430, status: 'degraded' },
              success_rate: { metric, latest: 98.8, baseline: 99.6, status: 'degraded' },
            },
            dub: {
              error_rate: { metric, latest: 0.005, baseline: 0.005, status: 'healthy' },
              p95_latency: { metric, latest: 410, baseline: 405, status: 'healthy' },
              success_rate: { metric, latest: 99.7, baseline: 99.7, status: 'healthy' },
            },
            syd: {
              error_rate: { metric, latest: 0.019, baseline: 0.007, status: 'degraded' },
              p95_latency: { metric, latest: 880, baseline: 470, status: 'degraded' },
              success_rate: { metric, latest: 97.9, baseline: 99.5, status: 'degraded' },
            },
          };
          return matrix[region][metric];
        },
        async list_related_incidents() {
          return [
            {
              id: 'inc_9048',
              severity: 'sev2',
              minutesAgo: 35,
              summary: 'Increased payment retries in canary',
            },
          ];
        },
        async get_release_guardrails() {
          return {
            maxErrorRate: 0.01,
            maxLatencyP95: 550,
            minSuccessRate: 99.1,
            rollbackOnIncidentCount: 1,
          };
        },
        async inspect_feature_flags() {
          return [
            { name: 'auth_fallback_bypass', state: 'bypass', owner: 'payments' },
            { name: 'ledger_shadow_write', state: 'enabled', owner: 'ledger' },
          ];
        },
      },
    },
  },
  {
    id: 'reconcile-marketplace-payouts',
    name: 'Reconcile Marketplace Payouts',
    file: 'reconcile-marketplace-payouts.js',
    description:
      'Rebuilds seller payout expectations from orders, refunds, payout transfers, and ledger adjustments.',
    inputs: reconcileMarketplacePayoutsInputs,
    options: {
      inputs: reconcileMarketplacePayoutsInputs,
      capabilities: {
        async list_batch_orders() {
          return [
            { orderId: 'ord_1', sellerId: 'seller_101', grossAmount: 24000, status: 'captured' },
            { orderId: 'ord_2', sellerId: 'seller_101', grossAmount: 9000, status: 'captured' },
            { orderId: 'ord_3', sellerId: 'seller_202', grossAmount: 15000, status: 'captured' },
            { orderId: 'ord_4', sellerId: 'seller_303', grossAmount: 8200, status: 'captured' },
          ];
        },
        async list_batch_refunds() {
          return [
            { orderId: 'ord_2', sellerId: 'seller_101', amount: 2000, reason: 'partial_refund' },
            { orderId: 'ord_4', sellerId: 'seller_303', amount: 8200, reason: 'full_refund' },
          ];
        },
        async list_payout_lines() {
          return [
            {
              sellerId: 'seller_101',
              amount: 28510,
              status: 'released',
              transferId: 'tr_101',
            },
            {
              sellerId: 'seller_202',
              amount: 14100,
              status: 'pending_review',
              transferId: 'tr_202',
            },
          ];
        },
        async list_ledger_adjustments() {
          return [
            { sellerId: 'seller_101', amount: 300, type: 'shipping_credit' },
            { sellerId: 'seller_202', amount: 500, type: 'manual_hold_release' },
            { sellerId: 'seller_303', amount: -200, type: 'chargeback_reserve' },
          ];
        },
        async load_seller_contract(sellerId) {
          const contracts = {
            seller_101: {
              sellerId,
              feeBps: 650,
              reservePct: 5,
              settlementMode: 'weekly',
            },
            seller_202: {
              sellerId,
              feeBps: 600,
              reservePct: 0,
              settlementMode: 'daily',
            },
            seller_303: {
              sellerId,
              feeBps: 700,
              reservePct: 10,
              settlementMode: 'weekly',
            },
          };
          return contracts[sellerId];
        },
      },
    },
  },
  {
    id: 'stabilize-oncall-handoff',
    name: 'Stabilize On-Call Handoff',
    file: 'stabilize-oncall-handoff.js',
    description:
      'Builds a dense handoff brief from incidents, repeated pages, muted alerts, and risky scheduled changes.',
    inputs: stabilizeOncallHandoffInputs,
    options: {
      inputs: stabilizeOncallHandoffInputs,
      capabilities: {
        async list_open_incidents() {
          return [
            {
              id: 'inc_9044',
              severity: 'critical',
              service: 'checkout',
              status: 'mitigated',
              summary: 'Checkout retries spiked during cache warmup',
            },
            {
              id: 'inc_9046',
              severity: 'high',
              service: 'webhooks',
              status: 'investigating',
              summary: 'Webhook backlog still above SLO',
            },
          ];
        },
        async list_recent_pages() {
          return [
            { service: 'checkout', summary: 'latency', count: 4 },
            { service: 'webhooks', summary: 'backlog', count: 2 },
            { service: 'fraud', summary: 'cpu saturation', count: 1 },
          ];
        },
        async list_muted_alerts() {
          return [
            {
              service: 'checkout',
              signal: 'p95_latency',
              reason: 'rollback in progress',
              expiresInMinutes: 45,
            },
            {
              service: 'webhooks',
              signal: 'queue_depth',
              reason: 'known backlog cleanup',
              expiresInMinutes: 220,
            },
          ];
        },
        async list_pending_followups() {
          return [
            { owner: 'alice', item: 'remove emergency canary pin', dueInMinutes: 90 },
            { owner: 'bob', item: 'verify cache refill jobs', dueInMinutes: 300 },
            { owner: 'cory', item: 'close incident note gaps', dueInMinutes: 120 },
          ];
        },
        async search_runbook_notes() {
          return [
            'Rollback required extra shard drain because cache refill stayed flaky',
            'Saturation cleared after checkout worker pool drain',
          ];
        },
        async list_scheduled_changes() {
          return [
            {
              changeId: 'chg_6022',
              service: 'checkout',
              startsInMinutes: 30,
              risk: 'high',
            },
            {
              changeId: 'chg_6024',
              service: 'fraud',
              startsInMinutes: 200,
              risk: 'medium',
            },
          ];
        },
      },
    },
  },
  {
    id: 'analyze-queue-backlog-regression',
    name: 'Analyze Queue Backlog Regression',
    file: 'analyze-queue-backlog-regression.js',
    description:
      'Combines per-shard depth, worker saturation, and dead-letter samples to explain a growing backlog.',
    inputs: analyzeQueueBacklogRegressionInputs,
    options: {
      inputs: analyzeQueueBacklogRegressionInputs,
      capabilities: {
        async list_queue_shards(_pipeline, region) {
          const byRegion = {
            'us-central1': [
              { shardId: 'a', workerPool: 'workers-a' },
              { shardId: 'b', workerPool: 'workers-b' },
            ],
            'europe-west1': [
              { shardId: 'c', workerPool: 'workers-c' },
            ],
          };
          return byRegion[region] || [];
        },
        async fetch_shard_metrics(_pipeline, region, shardId) {
          const key = region + ':' + shardId;
          const table = {
            'us-central1:a': {
              depth: 7800,
              oldestAgeSec: 1640,
              inflowPerMin: 440,
              outflowPerMin: 180,
              retryRate: 0.14,
            },
            'us-central1:b': {
              depth: 2100,
              oldestAgeSec: 420,
              inflowPerMin: 260,
              outflowPerMin: 250,
              retryRate: 0.03,
            },
            'europe-west1:c': {
              depth: 5200,
              oldestAgeSec: 980,
              inflowPerMin: 310,
              outflowPerMin: 160,
              retryRate: 0.09,
            },
          };
          return table[key];
        },
        async fetch_worker_pool_status(workerPool) {
          const status = {
            'workers-a': {
              pool: workerPool,
              saturation: 0.96,
              unavailableWorkers: 3,
              queuedRestarts: 2,
            },
            'workers-b': {
              pool: workerPool,
              saturation: 0.61,
              unavailableWorkers: 0,
              queuedRestarts: 0,
            },
            'workers-c': {
              pool: workerPool,
              saturation: 0.91,
              unavailableWorkers: 1,
              queuedRestarts: 1,
            },
          };
          return status[workerPool];
        },
        async sample_dead_letters(_pipeline, region, shardId) {
          const key = region + ':' + shardId;
          const samples = {
            'us-central1:a': [
              { code: 'THROTTLE', message: 'downstream throttle while reserving stock' },
              { code: 'POISON', message: 'poison message repeated after duplicate replay' },
            ],
            'us-central1:b': [
              { code: 'TIMEOUT', message: 'sporadic timeout on audit sink' },
            ],
            'europe-west1:c': [
              { code: 'SCHEMA', message: 'schema mismatch in serialization path' },
              { code: 'TIMEOUT', message: 'timeout while writing invoice event' },
            ],
          };
          return samples[key] || [];
        },
        async get_capacity_plan() {
          return {
            maxHealthyDepth: 3000,
            maxHealthyAgeSec: 600,
            escalationActions: [
              'page the owning worker pool',
              'pause the noisiest producer if backlog continues to climb',
            ],
          };
        },
      },
    },
  },
  {
    id: 'plan-database-failover',
    name: 'Plan Database Failover',
    file: 'plan-database-failover.js',
    description:
      'Resumable synchronous workflow that evaluates replica health, reserves a window, requests approval, and records a failover decision.',
    inputs: planDatabaseFailoverInputs,
    startPlan: {
      capabilities: {
        load_cluster_topology() {},
        load_replication_health() {},
        reserve_change_window() {},
        request_operator_approval() {},
        record_failover_decision() {},
      },
      resumes: [
        {
          primaryRegion: 'us-east-1',
          replicas: [
            { region: 'us-west-2', role: 'replica', priority: 1 },
            { region: 'eu-west-1', role: 'replica', priority: 2 },
          ],
          writeTrafficRps: 8400,
        },
        {
          replicas: [
            { region: 'us-west-2', lagMs: 120, replaying: true },
            { region: 'eu-west-1', lagMs: 340, replaying: true },
          ],
          stalledReplication: [],
        },
        {
          windowId: 'mw_118',
          startsInMinutes: 7,
        },
        {
          approved: true,
          approver: 'db-oncall',
          ticketId: 'ops-991',
        },
        {
          decisionId: 'failover_771',
        },
      ],
    },
  },
  {
    id: 'coordinate-warehouse-exception',
    name: 'Coordinate Warehouse Exception',
    file: 'coordinate-warehouse-exception.js',
    description:
      'Coordinates substitutions, split shipments, holds, and refunds when a fulfillment wave runs into inventory exceptions.',
    inputs: coordinateWarehouseExceptionInputs,
    options: {
      inputs: coordinateWarehouseExceptionInputs,
      capabilities: {
        async list_wave_orders() {
          return [
            {
              orderId: 'ord_710',
              priority: 'vip',
              lines: [
                { sku: 'sku_red_shoe', substituteSku: 'sku_blue_shoe' },
                { sku: 'sku_sock', substituteSku: null },
              ],
            },
            {
              orderId: 'ord_711',
              priority: 'standard',
              lines: [
                { sku: 'sku_jacket', substituteSku: null },
              ],
            },
            {
              orderId: 'ord_712',
              priority: 'expedite',
              lines: [
                { sku: 'sku_hat', substituteSku: 'sku_hat_alt' },
              ],
            },
          ];
        },
        async list_inventory_exceptions() {
          return [
            { sku: 'sku_red_shoe', type: 'out_of_stock', availableQty: 0 },
            { sku: 'sku_jacket', type: 'damaged', availableQty: 1 },
          ];
        },
        async list_pick_tickets() {
          return [
            { orderId: 'ord_710', sku: 'sku_red_shoe', requestedQty: 1, pickedQty: 0 },
            { orderId: 'ord_710', sku: 'sku_sock', requestedQty: 2, pickedQty: 2 },
            { orderId: 'ord_711', sku: 'sku_jacket', requestedQty: 1, pickedQty: 0 },
            { orderId: 'ord_712', sku: 'sku_hat', requestedQty: 1, pickedQty: 0 },
          ];
        },
        async list_customer_promises() {
          return [
            { orderId: 'ord_710', shipBy: '2026-04-11', expedite: true },
            { orderId: 'ord_711', shipBy: '2026-04-12', expedite: false },
            { orderId: 'ord_712', shipBy: '2026-04-11', expedite: true },
          ];
        },
        async lookup_substitution_policy() {
          return {
            allowSubstitution: true,
            protectedSkus: ['sku_jacket'],
          };
        },
      },
    },
  },
  {
    id: 'assess-global-deployment-freeze',
    name: 'Assess Global Deployment Freeze',
    file: 'assess-global-deployment-freeze.js',
    description:
      'Ranks service risk, staffing pressure, and recent incidents to decide whether a global deployment freeze is warranted.',
    inputs: assessGlobalDeploymentFreezeInputs,
    options: {
      inputs: assessGlobalDeploymentFreezeInputs,
      capabilities: {
        async list_recent_incidents() {
          return [
            {
              id: 'inc_9050',
              severity: 'sev1',
              service: 'identity-api',
              startedAt: '2026-04-11T09:05:00Z',
              summary: 'Global login latency regression',
            },
            {
              id: 'inc_9049',
              severity: 'sev2',
              service: 'payments-api',
              startedAt: '2026-04-11T05:20:00Z',
              summary: 'Canary retry pressure in payments',
            },
          ];
        },
        async list_active_changes() {
          return [
            {
              changeId: 'chg_6021',
              service: 'payments-api',
              stage: 'canary',
              startedAt: '2026-04-11T08:10:00Z',
              owner: 'payments',
            },
            {
              changeId: 'chg_6030',
              service: 'checkout-api',
              stage: 'scheduled',
              startedAt: '2026-04-11T11:00:00Z',
              owner: 'checkout',
            },
          ];
        },
        async list_staffing_exceptions() {
          return [
            {
              team: 'release-managers',
              kind: 'release_manager_gap',
              endsAt: '2026-04-11T18:00:00Z',
            },
          ];
        },
        async fetch_service_tier(service) {
          const tiers = {
            'checkout-api': { service, tier: 0, ownerTeam: 'checkout' },
            'payments-api': { service, tier: 0, ownerTeam: 'payments' },
            'identity-api': { service, tier: 0, ownerTeam: 'identity' },
            'pricing-api': { service, tier: 2, ownerTeam: 'pricing' },
          };
          return tiers[service];
        },
        async list_customer_commitments(service) {
          const commitments = {
            'checkout-api': [
              { account: 'enterprise_44', eventAt: '2026-04-11T15:00:00Z', revenueBand: 'high' },
            ],
            'payments-api': [
              { account: 'marketplace_9', eventAt: '2026-04-11T16:00:00Z', revenueBand: 'high' },
            ],
            'identity-api': [],
            'pricing-api': [],
          };
          return commitments[service] || [];
        },
      },
    },
  },
];
