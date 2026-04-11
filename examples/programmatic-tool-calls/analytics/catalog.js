'use strict';

module.exports = [
  {
    id: 'analytics_revenue_quality',
    name: 'Analyze Revenue Quality',
    file: 'analyze-revenue-quality.js',
    description:
      'Builds a board-ready revenue quality view across business units, forecast variance, collections pressure, and deal movement.',
    inputs: {
      quarter: '2026-Q2',
      materialityThreshold: 250000,
    },
    options: {
      capabilities: {
        async list_business_units() {
          return [
            { id: 'bu_enterprise', name: 'Enterprise', segment: 'enterprise', owner: 'vp-enterprise' },
            { id: 'bu_midmarket', name: 'Mid-Market', segment: 'mid_market', owner: 'vp-midmarket' },
            { id: 'bu_selfserve', name: 'Self-Serve', segment: 'self_serve', owner: 'gm-selfserve' },
          ];
        },
        async load_unit_actuals(unitId) {
          const actuals = {
            bu_enterprise: {
              recognizedRevenue: 6200000,
              deferredRevenue: 1500000,
              churnedArr: 410000,
              dso: 64,
              collectionsAtRisk: 580000,
            },
            bu_midmarket: {
              recognizedRevenue: 4300000,
              deferredRevenue: 920000,
              churnedArr: 260000,
              dso: 49,
              collectionsAtRisk: 140000,
            },
            bu_selfserve: {
              recognizedRevenue: 2100000,
              deferredRevenue: 260000,
              churnedArr: 180000,
              dso: 28,
              collectionsAtRisk: 45000,
            },
          };
          return actuals[unitId];
        },
        async load_unit_forecast(unitId) {
          const forecasts = {
            bu_enterprise: {
              committedRevenue: 6600000,
              stretchRevenue: 7050000,
              pipelineCoverage: 1.08,
            },
            bu_midmarket: {
              committedRevenue: 4150000,
              stretchRevenue: 4400000,
              pipelineCoverage: 1.21,
            },
            bu_selfserve: {
              committedRevenue: 2000000,
              stretchRevenue: 2180000,
              pipelineCoverage: 1.33,
            },
          };
          return forecasts[unitId];
        },
        async load_unit_deal_changes(unitId) {
          const changes = {
            bu_enterprise: [
              {
                opportunityId: 'opp_101',
                movement: 'slipped',
                amount: 420000,
                reason: 'procurement_delay',
                account: 'Northstar Bank',
              },
              {
                opportunityId: 'opp_114',
                movement: 'expanded',
                amount: 310000,
                reason: 'security_module_attach',
                account: 'Stellar Health',
              },
            ],
            bu_midmarket: [
              {
                opportunityId: 'opp_221',
                movement: 'pushed',
                amount: 120000,
                reason: 'budget_reapproval',
                account: 'Canvas Retail',
              },
              {
                opportunityId: 'opp_224',
                movement: 'pulled_forward',
                amount: 270000,
                reason: 'multi_year_close',
                account: 'Orbit Labs',
              },
            ],
            bu_selfserve: [
              {
                opportunityId: 'opp_301',
                movement: 'expanded',
                amount: 90000,
                reason: 'annual_prepaid',
                account: 'Long-tail cohort',
              },
            ],
          };
          return changes[unitId];
        },
        async list_collection_risks() {
          return [
            {
              unitId: 'bu_enterprise',
              accountId: 'acct_900',
              balance: 240000,
              reason: 'invoice_dispute',
              daysPastDue: 43,
            },
            {
              unitId: 'bu_enterprise',
              accountId: 'acct_901',
              balance: 180000,
              reason: 'budget_hold',
              daysPastDue: 32,
            },
            {
              unitId: 'bu_midmarket',
              accountId: 'acct_912',
              balance: 80000,
              reason: 'renewal_pending',
              daysPastDue: 18,
            },
          ];
        },
      },
    },
  },
  {
    id: 'analytics_fraud_ring',
    name: 'Investigate Fraud Ring',
    file: 'investigate-fraud-ring.js',
    description:
      'Correlates payments, device overlap, identity signals, chargebacks, and prior case notes to decide whether to escalate a fraud cluster.',
    inputs: {
      caseId: 'fraud_case_441',
      lookbackDays: 21,
    },
    options: {
      capabilities: {
        async load_alert_case(caseId) {
          return {
            id: caseId,
            queue: 'card_fraud',
            primaryReason: 'velocity_spike',
            flaggedAccountIds: ['acct_1', 'acct_2', 'acct_3'],
          };
        },
        async list_related_transactions() {
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
              entityId: 'ent_c',
              cardFingerprint: 'card_z',
              amount: 760,
              outcome: 'approved',
              ipAddress: '203.0.113.7',
              email: 'normal.user@example.test',
              deviceId: 'dev_3',
              timestamp: '2026-04-09T10:20:00Z',
            },
          ];
        },
        async fetch_device_clusters() {
          return [
            {
              clusterId: 'cluster_77',
              accounts: ['acct_1', 'acct_2', 'acct_3'],
              devices: ['dev_1', 'dev_2', 'dev_3'],
              riskLabel: 'dense_cross_account_overlap',
            },
          ];
        },
        async fetch_chargeback_history() {
          return [
            {
              cardFingerprint: 'card_x',
              chargebackRate: 0.26,
              disputedAmount: 8100,
              count: 5,
            },
            {
              cardFingerprint: 'card_y',
              chargebackRate: 0.04,
              disputedAmount: 600,
              count: 1,
            },
          ];
        },
        async lookup_identity_signals() {
          return [
            {
              entityId: 'ent_a',
              syntheticRisk: 0.82,
              watchlistHits: 0,
              documentMismatch: true,
            },
            {
              entityId: 'ent_b',
              syntheticRisk: 0.41,
              watchlistHits: 1,
              documentMismatch: false,
            },
            {
              entityId: 'ent_c',
              syntheticRisk: 0.85,
              watchlistHits: 0,
              documentMismatch: true,
            },
          ];
        },
        async search_internal_notes() {
          return [
            {
              source: 'case_111',
              body: 'Possible refund mule activity tied to a synthetic identity ring.',
            },
            {
              source: 'case_201',
              body: 'Analyst noted collusion signals across reused devices and shared IP space.',
            },
          ];
        },
      },
    },
  },
  {
    id: 'analytics_supplier_disruption',
    name: 'Assess Supplier Disruption',
    file: 'assess-supplier-disruption.js',
    description:
      'Maps a regional supplier shock into SKU constraints, alternate-source options, and revenue at risk.',
    inputs: {
      region: 'apac',
      scenario: 'port_closure',
    },
    options: {
      capabilities: {
        async list_suppliers() {
          return [
            {
              supplierId: 'sup_1',
              name: 'Shenzen Motion Systems',
              tier: 1,
              region: 'apac',
              riskEvent: 'port_closure',
              skuIds: ['sku_a', 'sku_b'],
              recoveryDays: 18,
            },
            {
              supplierId: 'sup_2',
              name: 'Pacific Board Fabrication',
              tier: 2,
              region: 'apac',
              riskEvent: 'flooding',
              skuIds: ['sku_c'],
              recoveryDays: 11,
            },
          ];
        },
        async fetch_inventory_positions() {
          return [
            { sku: 'sku_a', onHandUnits: 420, daysOfCover: 12, inboundUnits: 300 },
            { sku: 'sku_b', onHandUnits: 180, daysOfCover: 7, inboundUnits: 0 },
            { sku: 'sku_c', onHandUnits: 900, daysOfCover: 28, inboundUnits: 200 },
          ];
        },
        async fetch_open_shipments() {
          return [
            {
              supplierId: 'sup_1',
              shipmentId: 'ship_101',
              status: 'delayed',
              delayedDays: 6,
            },
            {
              supplierId: 'sup_2',
              shipmentId: 'ship_202',
              status: 'in_transit',
              delayedDays: 0,
            },
          ];
        },
        async lookup_alternate_sources() {
          return [
            {
              sku: 'sku_a',
              alternateSupplier: 'Monterrey Motion',
              qualified: true,
              leadTimeDays: 14,
              unitCostDelta: 0.08,
            },
            {
              sku: 'sku_b',
              alternateSupplier: 'Seoul Precision',
              qualified: false,
              leadTimeDays: 20,
              unitCostDelta: 0.11,
            },
          ];
        },
        async map_sku_revenue() {
          return [
            { sku: 'sku_a', weeklyRevenue: 410000, criticality: 'high' },
            { sku: 'sku_b', weeklyRevenue: 290000, criticality: 'high' },
            { sku: 'sku_c', weeklyRevenue: 160000, criticality: 'medium' },
          ];
        },
      },
    },
  },
  {
    id: 'analytics_market_event_brief',
    name: 'Prepare Market Event Brief',
    file: 'prepare-market-event-brief.js',
    description:
      'Builds an event brief from consensus revisions, options positioning, transcripts, and street research. Intentionally uses realistic transcript token extraction.',
    inputs: {
      symbol: 'NVMM',
      eventType: 'earnings',
      noteWindowDays: 21,
    },
    options: {
      capabilities: {
        async fetch_event_calendar() {
          return {
            nextEventDate: '2026-04-29',
            blackoutStart: '2026-04-15',
            timezone: 'America/New_York',
          };
        },
        async fetch_consensus_estimates() {
          return {
            revenue: 1820000000,
            eps: 2.14,
            revisions: [
              { broker: 'North Pier', direction: 'up', deltaPct: 0.03 },
              { broker: 'Westlake', direction: 'down', deltaPct: -0.02 },
              { broker: 'Juniper', direction: 'down', deltaPct: -0.01 },
            ],
          };
        },
        async fetch_transcript_history() {
          return [
            {
              quarter: '2025-Q2',
              excerpts: [
                'Pricing stayed disciplined, but demand in Europe remained soft.',
                'Inventory normalization should complete next quarter if channel sell-through holds.',
              ],
            },
            {
              quarter: '2025-Q3',
              excerpts: [
                'Margin recovery depended on pricing and better freight contracts.',
                'Guide assumes capacity remains available for the accelerator line.',
              ],
            },
          ];
        },
        async fetch_options_positioning() {
          return {
            impliedMovePct: 0.092,
            putCallRatio: 1.4,
            skew: 'puts_bid',
            largestStrikes: [
              { strike: 120, side: 'put', openInterest: 18200 },
              { strike: 145, side: 'call', openInterest: 12300 },
            ],
          };
        },
        async search_research_notes() {
          return [
            {
              source: 'Alder',
              publishedAt: '2026-04-05',
              excerpt: 'We see margin pressure persisting, though demand stabilization is improving in enterprise.',
            },
            {
              source: 'Beacon',
              publishedAt: '2026-04-08',
              excerpt: 'Inventory digestion is ongoing and could keep near-term guide conservative.',
            },
          ];
        },
      },
    },
  },
  {
    id: 'analytics_model_regression',
    name: 'Triage Model Regression',
    file: 'triage-model-regression.js',
    description:
      'Combines deploy history, feature drift, quality metrics, and labeling signals to triage a production ML regression.',
    inputs: {
      modelId: 'fraud_score_v4',
      windowHours: 24,
    },
    options: {
      capabilities: {
        async list_recent_deploys() {
          return [
            {
              id: 'dep_model_19',
              version: 'fraud_score_v4.19',
              minutesAgo: 73,
              initiator: 'ml-release-bot',
            },
          ];
        },
        async fetch_model_metrics(_modelId, metric) {
          const metrics = {
            precision: {
              metric: 'precision',
              current: 0.77,
              baseline: 0.91,
              status: 'degraded',
            },
            recall: {
              metric: 'recall',
              current: 0.69,
              baseline: 0.72,
              status: 'healthy',
            },
            latency_ms: {
              metric: 'latency_ms',
              current: 164,
              baseline: 118,
              status: 'degraded',
            },
            fallback_rate: {
              metric: 'fallback_rate',
              current: 0.14,
              baseline: 0.03,
              status: 'degraded',
            },
          };
          return metrics[metric];
        },
        async fetch_feature_drift() {
          return [
            { feature: 'merchant_country', driftScore: 0.41, topSegment: 'cross_border' },
            { feature: 'device_age_days', driftScore: 0.18, topSegment: 'new_devices' },
            { feature: 'charge_amount_bucket', driftScore: 0.37, topSegment: 'high_value' },
          ];
        },
        async list_annotation_issues() {
          return [
            {
              queue: 'manual_review_backlog',
              severity: 'high',
              summary: 'labels delayed by 18 hours for high-risk cohort',
            },
          ];
        },
        async get_rollback_playbook() {
          return {
            immediateActions: [
              'compare feature feed freshness across regions',
              'disable recent fallback threshold experiment',
              'sample false positives from highest-drift cohorts',
            ],
            rollbackTarget: 'fraud_score_v4.18',
          };
        },
      },
    },
  },
  {
    id: 'analytics_enterprise_renewal',
    name: 'Underwrite Enterprise Renewal',
    file: 'underwrite-enterprise-renewal.js',
    description:
      'Sequentially gathers account health signals and underwriting benchmarks before producing a renewal posture.',
    inputs: {
      accountId: 'acct_enterprise_77',
      renewalTermMonths: 24,
    },
    startPlan: {
      capabilities: {
        load_account_snapshot() {},
        load_product_usage() {},
        load_support_escalations() {},
        load_payment_history() {},
        lookup_peer_benchmark() {},
      },
      resumes: [
        {
          id: 'acct_enterprise_77',
          name: 'Atlas Manufacturing',
          segment: 'upper_mid_market',
          plan: 'business',
          annualRecurringRevenue: 480000,
          owner: 'csm@atlas.test',
        },
        {
          seatUtilization: 0.58,
          activeWeeks: 7,
          moduleAdoption: [
            { module: 'workflow_automation', adopted: true },
            { module: 'governance', adopted: false },
            { module: 'analytics', adopted: true },
          ],
        },
        [
          { severity: 'high', openDays: 11, theme: 'sso_reliability' },
          { severity: 'medium', openDays: 4, theme: 'reporting_latency' },
        ],
        {
          lateInvoices: 2,
          averageDaysLate: 24,
          collectionsFlag: false,
        },
        {
          grossRetentionFloor: 0.9,
          expansionMedian: 0.17,
          discountCeiling: 0.12,
        },
      ],
    },
  },
  {
    id: 'analytics_market_abuse_review',
    name: 'Escalate Market Abuse Review',
    file: 'escalate-market-abuse-review.js',
    description:
      'Uses sequential surveillance, order-book, profile, communications, and watchlist lookups to decide whether to escalate a trading case.',
    inputs: {
      investigationId: 'mkt_case_902',
    },
    startPlan: {
      capabilities: {
        load_surveillance_alert() {},
        load_order_timeline() {},
        load_client_profile() {},
        load_comms_hits() {},
        load_watchlist_hits() {},
      },
      resumes: [
        {
          alertId: 'alert_902',
          clientId: 'client_44',
          strategy: 'small_cap_momentum',
          trigger: 'spoofing_signature',
          windowStart: '2026-04-10T14:00:00Z',
          windowEnd: '2026-04-10T14:30:00Z',
        },
        [
          {
            orderId: 'ord_1',
            side: 'buy',
            symbol: 'ALPX',
            notional: 280000,
            event: 'layer_added',
          },
          {
            orderId: 'ord_2',
            side: 'buy',
            symbol: 'ALPX',
            notional: 310000,
            event: 'cancelled_near_touch',
          },
          {
            orderId: 'ord_3',
            side: 'buy',
            symbol: 'ALPX',
            notional: 260000,
            event: 'layer_added',
          },
          {
            orderId: 'ord_4',
            side: 'sell',
            symbol: 'ALPX',
            notional: 350000,
            event: 'cancelled_near_touch',
          },
        ],
        {
          clientId: 'client_44',
          desk: 'event_driven',
          jurisdiction: 'GB',
          priorWarnings: 1,
        },
        [
          {
            channel: 'chat',
            excerpt: 'Keep it under the radar and close it before the print.',
          },
          {
            channel: 'voice',
            excerpt: 'If we can paint the tape early the rest should follow.',
          },
        ],
        [
          { list: 'internal_heightened_supervision', reason: 'prior spoofing alert' },
        ],
      ],
    },
  },
  {
    id: 'analytics_capital_allocation',
    name: 'Build Capital Allocation Brief',
    file: 'build-capital-allocation-brief.js',
    description:
      'Ranks downside loss contributors, liquidity constraints, and hedge options for a portfolio rebalance. Intentionally keeps realistic position ranking logic.',
    inputs: {
      portfolioId: 'growth_long_only',
      downsideScenario: 'rates_up_growth_selloff',
    },
    options: {
      capabilities: {
        async list_portfolio_positions() {
          return [
            {
              ticker: 'CLDY',
              sector: 'software',
              marketValue: 4200000,
              beta: 1.35,
              thesis: 'cloud optimization leader',
              hedgeable: true,
            },
            {
              ticker: 'NOVA',
              sector: 'semis',
              marketValue: 3600000,
              beta: 1.2,
              thesis: 'ai accelerator demand',
              hedgeable: true,
            },
            {
              ticker: 'RXLT',
              sector: 'biotech',
              marketValue: 1800000,
              beta: 1.55,
              thesis: 'phase-three catalyst',
              hedgeable: false,
            },
          ];
        },
        async fetch_factor_shocks() {
          return [
            { sector: 'software', drawdownPct: 0.14, betaMultiplier: 1.1 },
            { sector: 'semis', drawdownPct: 0.18, betaMultiplier: 1.05 },
            { sector: 'biotech', drawdownPct: 0.11, betaMultiplier: 1.3 },
          ];
        },
        async fetch_liquidity_profile() {
          return [
            { ticker: 'CLDY', daysToExit: 3, averageDailyVolumePct: 0.19 },
            { ticker: 'NOVA', daysToExit: 2, averageDailyVolumePct: 0.26 },
            { ticker: 'RXLT', daysToExit: 6, averageDailyVolumePct: 0.11 },
          ];
        },
        async fetch_hedge_candidates() {
          return [
            {
              ticker: 'CLDY',
              instrument: 'put_spread',
              expectedProtectionPct: 0.42,
              carryCostBps: 88,
            },
            {
              ticker: 'NOVA',
              instrument: 'collar',
              expectedProtectionPct: 0.35,
              carryCostBps: 64,
            },
          ];
        },
        async fetch_risk_limits() {
          return {
            maxSingleNameLoss: 620000,
            maxPortfolioDrawdown: 1400000,
            concentrationLimitPct: 0.18,
          };
        },
      },
    },
  },
];
