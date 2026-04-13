'use strict';

const approvalExceptionRoutingInputs = {
  requestId: 'pr_9001',
  actorEmail: 'alex.rivera@acme.test',
  region: 'US',
  spendLimitUsd: 25000,
};

const securityAccessRecertificationInputs = {
  reviewId: 'recert_q2_2026',
  systemSlug: 'prod-admin-console',
  ownerEmail: 'security-ops@acme.test',
  campaignWindowDays: 90,
};

const vipSupportEscalationInputs = {
  ticketId: 'vip_44012',
  customerId: 'acct_zephyr',
  nowIso: '2026-04-11T19:20:00Z',
  targetSlaMinutes: 60,
};

const payoutBatchReleaseReviewInputs = {
  payoutBatchId: 'payout_batch_2207',
  analystId: 'fraud-analyst-17',
  region: 'US',
  releaseThresholdUsd: 50000,
};

const enterpriseRenewalSavePlanInputs = {
  accountId: 'acct_northwind',
  renewalWindowDays: 37,
  targetExpansionSeats: 25,
};

const vendorComplianceRenewalInputs = {
  vendorId: 'vendor_polaris',
  reviewCycleId: 'vendor_review_2026_q2',
  requiredFrameworks: ['SOC2', 'ISO27001', 'DPA'],
};

const privacyErasureOrchestrationInputs = {
  requestId: 'dsr_440',
  customerId: 'cust_991',
  deadlineIso: '2026-05-01T00:00:00Z',
  jurisdictions: ['GDPR', 'CCPA'],
};

function approvalRoutingStartPlan() {
  return {
    capabilities: {
      fetch_purchase_request() {},
      fetch_vendor_profile() {},
      fetch_policy_matrix() {},
      create_approval_case() {},
      post_timeline_event() {},
      notify_approvers() {},
    },
    resumes: [
      {
        capability: 'fetch_purchase_request',
        value: {
          id: 'pr_9001',
          vendorId: 'vendor_77',
          category: 'security-software',
          amountUsd: 85000,
          purpose: 'Expand endpoint telemetry coverage before the summer launch.',
          dataClasses: ['employee_pii', 'security_logs'],
          destinationCountries: ['US', 'DE'],
          costCenter: 'ENG-PLAT',
          requestedBy: 'alex.rivera@acme.test',
          businessOwner: 'vp-platform@acme.test',
        },
      },
      {
        capability: 'fetch_vendor_profile',
        value: {
          id: 'vendor_77',
          name: 'ZeroNorth Analytics',
          tier: 'new',
          onboardingState: 'in_review',
          securityReviewStatus: 'pending',
          hasDpa: false,
          allowedRegions: ['US'],
          accountOwner: 'procurement@acme.test',
        },
      },
      {
        capability: 'fetch_policy_matrix',
        value: {
          baseApprovers: [
            'finance-director@acme.test',
            'budget-owner@acme.test',
          ],
          escalationApprovers: [
            {
              when: 'over_spend_limit',
              approver: 'cfo@acme.test',
            },
            {
              when: 'vendor_not_fully_onboarded',
              approver: 'procurement-lead@acme.test',
            },
            {
              when: 'security_review_required',
              approver: 'security-review@acme.test',
            },
            {
              when: 'cross_border_data_transfer',
              approver: 'privacy-counsel@acme.test',
            },
            {
              when: 'missing_dpa',
              approver: 'privacy-counsel@acme.test',
            },
          ],
          blockedCountries: ['RU', 'KP'],
        },
      },
      {
        capability: 'create_approval_case',
        value: {
          caseId: 'appr_7781',
          queue: 'procurement-high-risk',
          slaHours: 12,
        },
      },
      {
        capability: 'post_timeline_event',
        value: {
          eventId: 'evt_2001',
        },
      },
      {
        capability: 'notify_approvers',
        value: {
          notificationIds: ['msg_1', 'msg_2', 'msg_3', 'msg_4', 'msg_5'],
        },
      },
    ],
  };
}

function makeSecurityAccessRecertificationOptions() {
  return {
    inputs: securityAccessRecertificationInputs,
    capabilities: {
      async list_access_grants() {
        return [
          {
            grantId: 'grant_1',
            userId: 'u_ada',
            entitlement: 'billing_admin',
            scope: 'write:payments',
            grantedDaysAgo: 122,
            elevationType: 'persistent',
          },
          {
            grantId: 'grant_2',
            userId: 'u_jo',
            entitlement: 'support_readonly',
            scope: 'read:cases',
            grantedDaysAgo: 24,
            elevationType: 'temporary',
          },
          {
            grantId: 'grant_3',
            userId: 'u_lee',
            entitlement: 'incident_admin',
            scope: 'write:production',
            grantedDaysAgo: 64,
            elevationType: 'persistent',
          },
        ];
      },
      async fetch_identity_profiles() {
        return [
          {
            userId: 'u_ada',
            managerEmail: 'mgr-finops@acme.test',
            workerType: 'contractor',
            department: 'finance-ops',
            lastSeenDaysAgo: 8,
          },
          {
            userId: 'u_jo',
            managerEmail: 'mgr-support@acme.test',
            workerType: 'employee',
            department: 'support',
            lastSeenDaysAgo: 2,
          },
          {
            userId: 'u_lee',
            managerEmail: 'mgr-sre@acme.test',
            workerType: 'employee',
            department: 'sre',
            lastSeenDaysAgo: 61,
          },
        ];
      },
      async fetch_recent_security_findings() {
        return [
          {
            userId: 'u_ada',
            findingId: 'f_1',
            severity: 'medium',
            status: 'open',
          },
          {
            userId: 'u_lee',
            findingId: 'f_2',
            severity: 'high',
            status: 'open',
          },
        ];
      },
      async fetch_exception_register() {
        return [
          {
            grantId: 'grant_3',
            status: 'approved',
            approvedBy: 'security-ops@acme.test',
            expiresInDays: 12,
          },
        ];
      },
      async create_recertification_tasks(payload) {
        let createdCount = 0;
        for (const group of payload.taskGroups) {
          createdCount += group.grants.length;
        }
        return {
          taskBatchId: 'recert_batch_18',
          createdCount,
        };
      },
    },
  };
}

function makeVipSupportEscalationOptions() {
  return {
    inputs: vipSupportEscalationInputs,
    capabilities: {
      async fetch_ticket_thread() {
        return {
          id: 'vip_44012',
          priority: 'urgent',
          severity: 'high',
          productArea: 'login',
          summary: 'Enterprise finance users cannot log in after invoice update.',
          lastCustomerReplyAt: '2026-04-11T18:37:00Z',
          thread: [
            {
              sender: 'customer',
              body: 'We are blocked during month-end close.',
            },
          ],
        };
      },
      async fetch_customer_360() {
        return {
          id: 'acct_zephyr',
          name: 'Zephyr Robotics',
          plan: 'enterprise',
          arrUsd: 540000,
          renewalDateIso: '2026-05-14',
          successOwner: 'csm@acme.test',
          executiveSponsor: 'vp-revenue@acme.test',
          openEscalations: 1,
        };
      },
      async list_open_incidents() {
        return [
          {
            id: 'inc_901',
            severity: 'critical',
            startedAt: '2026-04-11T18:40:00Z',
            title: 'Login failures after invoice-address migration',
            status: 'investigating',
          },
          {
            id: 'inc_890',
            severity: 'medium',
            startedAt: '2026-04-11T10:20:00Z',
            title: 'Minor delay in invoice PDF generation',
            status: 'monitoring',
          },
        ];
      },
      async search_internal_notes() {
        return [
          {
            noteId: 'note_11',
            title: 'Renewal risk from previous auth outage',
            body: 'Customer flagged renewal risk and asked for exec visibility.',
            createdAt: '2026-03-18T09:00:00Z',
          },
          {
            noteId: 'note_12',
            title: 'Workaround for SSO fallback',
            body: 'Offer temporary admin-assisted reset if invoice sync blocks login.',
            createdAt: '2026-04-02T14:00:00Z',
          },
        ];
      },
      async post_support_brief(payload) {
        return {
          briefId: 'brief_991',
          postedChannel: payload.channel,
        };
      },
    },
  };
}

function makePayoutBatchReleaseReviewOptions() {
  return {
    inputs: payoutBatchReleaseReviewInputs,
    capabilities: {
      async fetch_payout_batch() {
        return {
          id: 'payout_batch_2207',
          currency: 'USD',
          payouts: [
            {
              payoutId: 'po_1',
              accountId: 'acct_alpha',
              amountUsd: 120000,
            },
            {
              payoutId: 'po_2',
              accountId: 'acct_beta',
              amountUsd: 18000,
            },
            {
              payoutId: 'po_3',
              accountId: 'acct_gamma',
              amountUsd: 36000,
            },
          ],
        };
      },
      async list_account_flags() {
        return [
          {
            accountId: 'acct_alpha',
            code: 'SANCTION_SCREENING_REVIEW',
            severity: 'critical',
            state: 'open',
          },
          {
            accountId: 'acct_gamma',
            code: 'recent_chargeback_cluster',
            severity: 'high',
            state: 'open',
          },
        ];
      },
      async list_recent_transactions() {
        return [
          {
            accountId: 'acct_alpha',
            type: 'sale',
            amountUsd: 51000,
            destinationCountry: 'US',
            disputed: false,
          },
          {
            accountId: 'acct_beta',
            type: 'sale',
            amountUsd: 9000,
            destinationCountry: 'US',
            disputed: false,
          },
          {
            accountId: 'acct_gamma',
            type: 'sale',
            amountUsd: 12000,
            destinationCountry: 'CA',
            disputed: true,
          },
          {
            accountId: 'acct_gamma',
            type: 'sale',
            amountUsd: 19000,
            destinationCountry: 'GB',
            disputed: true,
          },
        ];
      },
      async fetch_ruleset() {
        return {
          maxDisputedVolumeUsd: 20000,
          crossBorderReviewRegions: ['US'],
          hardStopCodes: ['SANCTION_SCREENING_REVIEW'],
        };
      },
      async record_release_decision(payload) {
        return {
          decisionId: 'decision_2207',
          state: payload.holds.length > 0 ? 'partial_hold' : 'released',
        };
      },
    },
  };
}

function makeEnterpriseRenewalSavePlanOptions() {
  return {
    inputs: enterpriseRenewalSavePlanInputs,
    capabilities: {
      async fetch_account_summary() {
        return {
          id: 'acct_northwind',
          name: 'Northwind Health',
          currentPlan: 'business',
          currentSeats: 180,
          arrUsd: 310000,
          renewalDateIso: '2026-05-18',
          csmEmail: 'ivy@acme.test',
        };
      },
      async fetch_product_usage() {
        return {
          activeUsers: 154,
          seatUtilization: 0.86,
          adoption: [
            {
              area: 'workflow_automation',
              status: 'healthy',
            },
            {
              area: 'analytics_dashboards',
              status: 'watch',
            },
            {
              area: 'api_integrations',
              status: 'degraded',
            },
          ],
        };
      },
      async list_open_support_cases() {
        return [
          {
            caseId: 'case_1',
            severity: 'high',
            theme: 'sso',
            ageDays: 17,
            status: 'open',
          },
          {
            caseId: 'case_2',
            severity: 'medium',
            theme: 'reporting',
            ageDays: 5,
            status: 'open',
          },
        ];
      },
      async fetch_billing_history() {
        return [
          {
            invoiceId: 'inv_1',
            status: 'paid',
            amountUsd: 24000,
            daysLate: 0,
          },
          {
            invoiceId: 'inv_2',
            status: 'open',
            amountUsd: 26000,
            daysLate: 9,
          },
        ];
      },
      async create_success_plan(payload) {
        return {
          planId: 'plan_44',
          owner: payload.csmEmail,
        };
      },
      async log_account_note() {
        return {
          noteId: 'note_plan_44',
        };
      },
    },
  };
}

function makeVendorComplianceRenewalOptions() {
  return {
    inputs: vendorComplianceRenewalInputs,
    capabilities: {
      async fetch_vendor_master() {
        return {
          id: 'vendor_polaris',
          name: 'Polaris AI',
          serviceTier: 'critical',
          hostsCustomerData: true,
          primaryCountry: 'US',
        };
      },
      async fetch_control_evidence() {
        return [
          {
            framework: 'SOC2',
            type: 'report',
            status: 'current',
            ageDays: 120,
          },
          {
            framework: 'ISO27001',
            type: 'certificate',
            status: 'expired',
            ageDays: 410,
          },
          {
            framework: 'DPA',
            type: 'contract',
            status: 'current',
            ageDays: 40,
          },
        ];
      },
      async fetch_data_flow_inventory() {
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
        ];
      },
      async fetch_subprocessor_list() {
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
        ];
      },
      async file_vendor_review(payload) {
        return {
          reviewRecordId: 'vendor_record_812',
          state: payload.recommendedDecision,
        };
      },
    },
  };
}

function privacyErasureStartPlan() {
  return {
    capabilities: {
      fetch_privacy_request() {},
      list_systems_of_record() {},
      fetch_retention_exceptions() {},
      queue_erasure_job() {},
      record_case_event() {},
      finalize_request() {},
    },
    resumes: [
      {
        capability: 'fetch_privacy_request',
        value: {
          id: 'dsr_440',
          type: 'erasure',
          subjectEmail: 'patient-admin@northwind.test',
          requestedDataClasses: ['customer_pii', 'usage_metadata'],
        },
      },
      {
        capability: 'list_systems_of_record',
        value: [
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
        ],
      },
      {
        capability: 'fetch_retention_exceptions',
        value: [
          {
            system: 'billing',
            recordType: 'invoice_record',
            reason: 'tax_retention_hold',
            expiresInDays: 187,
          },
        ],
      },
      {
        capability: 'queue_erasure_job',
        value: {
          jobId: 'erase_1',
          system: 'accounts',
          state: 'queued',
        },
      },
      {
        capability: 'record_case_event',
        value: {
          eventId: 'evt_erase_1',
        },
      },
      {
        capability: 'queue_erasure_job',
        value: {
          jobId: 'erase_2',
          system: 'warehouse',
          state: 'queued',
        },
      },
      {
        capability: 'record_case_event',
        value: {
          eventId: 'evt_erase_2',
        },
      },
      {
        capability: 'queue_erasure_job',
        value: {
          jobId: 'erase_3',
          system: 'analytics',
          state: 'queued',
        },
      },
      {
        capability: 'record_case_event',
        value: {
          eventId: 'evt_erase_3',
        },
      },
      {
        capability: 'finalize_request',
        value: {
          caseId: 'privacy_case_44',
          finalState: 'in_progress',
        },
      },
    ],
  };
}

function makeChargebackEvidenceAssemblyOptions() {
  return {
    inputs: {
      disputeId: 'disp_882',
      merchantId: 'mrc_77',
      network: 'visa',
    },
    capabilities: {
      async fetch_dispute_case() {
        return {
          id: 'disp_882',
          reasonCode: 'fraud',
          amountUsd: 389.42,
          cardholderClaim: 'I did not authorize this purchase.',
          orderId: 'ord_9912',
        };
      },
      async fetch_order_timeline() {
        return [
          {
            stage: 'checkout_completed',
            at: '2026-04-01T14:10:00Z',
            actor: 'customer',
            details: 'Order completed from saved device.',
          },
          {
            stage: 'fraud_screen_passed',
            at: '2026-04-01T14:10:03Z',
            actor: 'risk-engine',
            details: 'Low-risk score based on historical pattern.',
          },
          {
            stage: 'refund_declined',
            at: '2026-04-05T11:00:00Z',
            actor: 'support',
            details: 'Refund declined because goods were already delivered.',
          },
        ];
      },
      async fetch_customer_communications() {
        return [
          {
            direction: 'outbound',
            channel: 'email',
            body: 'Please confirm the delivery window for your order.',
            at: '2026-04-01T16:00:00Z',
          },
          {
            direction: 'inbound',
            channel: 'email',
            body: 'Confirmed. I will be available to receive it.',
            at: '2026-04-01T16:12:00Z',
          },
        ];
      },
      async fetch_fulfillment_events() {
        return [
          {
            type: 'shipment_created',
            carrier: 'ups',
            status: 'label_created',
            at: '2026-04-01T18:00:00Z',
            signedBy: null,
          },
          {
            type: 'out_for_delivery',
            carrier: 'ups',
            status: 'out_for_delivery',
            at: '2026-04-03T08:05:00Z',
            signedBy: null,
          },
          {
            type: 'delivered',
            carrier: 'ups',
            status: 'delivered',
            at: '2026-04-03T13:44:00Z',
            signedBy: 'J. Patel',
          },
        ];
      },
      async submit_evidence_packet() {
        return {
          packetId: 'packet_882',
          status: 'submitted',
        };
      },
    },
  };
}

module.exports = [
  {
    id: 'approval-exception-routing',
    name: 'Approval Exception Routing',
    file: 'approval-exception-routing.js',
    description:
      'Routes a high-risk vendor purchase through procurement, finance, security, and privacy approvers.',
    inputs: approvalExceptionRoutingInputs,
    startPlan: approvalRoutingStartPlan(),
  },
  {
    id: 'security-access-recertification',
    name: 'Security Access Recertification',
    file: 'security-access-recertification.js',
    description:
      'Builds manager review queues for stale or high-risk privileged access grants.',
    inputs: securityAccessRecertificationInputs,
    options: makeSecurityAccessRecertificationOptions(),
  },
  {
    id: 'vip-support-escalation',
    name: 'VIP Support Escalation',
    file: 'vip-support-escalation.js',
    description:
      'Creates an escalation brief for a renewal-risk enterprise support incident.',
    inputs: vipSupportEscalationInputs,
    options: makeVipSupportEscalationOptions(),
  },
  {
    id: 'payout-batch-release-review',
    name: 'Payout Batch Release Review',
    file: 'payout-batch-release-review.js',
    description:
      'Screens a payout batch for holds and release decisions using fraud and compliance signals.',
    inputs: payoutBatchReleaseReviewInputs,
    options: makePayoutBatchReleaseReviewOptions(),
  },
  {
    id: 'enterprise-renewal-save-plan',
    name: 'Enterprise Renewal Save Plan',
    file: 'enterprise-renewal-save-plan.js',
    description:
      'Aggregates product, support, and billing signals into a customer-success save plan.',
    inputs: enterpriseRenewalSavePlanInputs,
    options: makeEnterpriseRenewalSavePlanOptions(),
  },
  {
    id: 'vendor-compliance-renewal',
    name: 'Vendor Compliance Renewal',
    file: 'vendor-compliance-renewal.js',
    description:
      'Reviews vendor evidence, data flows, and subprocessors before a compliance renewal decision.',
    inputs: vendorComplianceRenewalInputs,
    options: makeVendorComplianceRenewalOptions(),
  },
  {
    id: 'privacy-erasure-orchestration',
    name: 'Privacy Erasure Orchestration',
    file: 'privacy-erasure-orchestration.js',
    description:
      'Coordinates resumable erasure jobs across systems of record while honoring retention holds.',
    inputs: privacyErasureOrchestrationInputs,
    startPlan: privacyErasureStartPlan(),
  },
  {
    id: 'chargeback-evidence-assembly',
    name: 'Chargeback Evidence Assembly',
    file: 'chargeback-evidence-assembly.js',
    description:
      'Builds a network-ready evidence packet from dispute, communication, and fulfillment systems.',
    inputs: {
      disputeId: 'disp_882',
      merchantId: 'mrc_77',
      network: 'visa',
    },
    options: makeChargebackEvidenceAssemblyOptions(),
  },
];
