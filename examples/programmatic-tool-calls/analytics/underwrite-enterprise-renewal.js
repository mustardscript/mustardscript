/*
Inputs:
  - accountId: string
  - renewalTermMonths: number

Capabilities:
  - load_account_snapshot(accountId) -> { id, name, segment, plan, annualRecurringRevenue, owner }
  - load_product_usage(accountId) -> { seatUtilization, activeWeeks, moduleAdoption: [{ module, adopted }] }
  - load_support_escalations(accountId) -> [{ severity, openDays, theme }]
  - load_payment_history(accountId) -> { lateInvoices, averageDaysLate, collectionsFlag }
  - lookup_peer_benchmark(segment, termMonths) -> { grossRetentionFloor, expansionMedian, discountCeiling }
*/

const account = load_account_snapshot(accountId);
const usage = load_product_usage(accountId);
const escalations = load_support_escalations(accountId);
const payments = load_payment_history(accountId);
const benchmark = lookup_peer_benchmark(account.segment, renewalTermMonths);

let adoptedModuleCount = 0;
for (const module of usage.moduleAdoption) {
  if (module.adopted) {
    adoptedModuleCount += 1;
  }
}

let openHighSeverity = 0;
const escalationThemes = [];
for (const escalation of escalations) {
  if (escalation.severity === "high" || escalation.severity === "critical") {
    openHighSeverity += 1;
  }
  if (!escalationThemes.includes(escalation.theme)) {
    escalationThemes.push(escalation.theme);
  }
}

const riskSignals = [];
if (usage.seatUtilization < 0.62) {
  riskSignals.push("low_seat_utilization");
}
if (usage.activeWeeks < 8) {
  riskSignals.push("inconsistent_recent_usage");
}
if (openHighSeverity > 0) {
  riskSignals.push("open_high_severity_support");
}
if (payments.collectionsFlag || payments.averageDaysLate > 20) {
  riskSignals.push("payment_discipline");
}

let posture = "renew_on_standard_terms";
if (riskSignals.length >= 3) {
  posture = "executive_review_required";
} else if (riskSignals.length >= 1) {
  posture = "renew_with_success_plan";
}

const result = {};
result.accountId = account.id;
result.accountName = account.name;
result.segment = account.segment;
result.plan = account.plan;
result.annualRecurringRevenue = account.annualRecurringRevenue;
result.owner = account.owner;
result.renewalTermMonths = renewalTermMonths;
result.seatUtilization = usage.seatUtilization;
result.activeWeeks = usage.activeWeeks;
result.adoptedModuleCount = adoptedModuleCount;
result.openHighSeverity = openHighSeverity;
result.escalationThemes = escalationThemes;
result.payments = payments;
result.peerBenchmark = benchmark;
result.riskSignals = riskSignals;
result.posture = posture;
result.underwritingMemo = [
  "Retention floor for segment: " + benchmark.grossRetentionFloor,
  "Expansion median for segment: " + benchmark.expansionMedian,
  "Discount ceiling for segment: " + benchmark.discountCeiling,
  "Recommended posture: " + posture,
];

result;
