/*
Inputs:
  - requestId: string
  - actorEmail: string
  - region: string
  - spendLimitUsd: number

Capabilities:
  - fetch_purchase_request(requestId) -> { id, vendorId, category, amountUsd, purpose, dataClasses, destinationCountries, costCenter, requestedBy, businessOwner }
  - fetch_vendor_profile(vendorId) -> { id, name, tier, onboardingState, securityReviewStatus, hasDpa, allowedRegions, accountOwner }
  - fetch_policy_matrix(region, category, amountUsd) -> { baseApprovers, escalationApprovers, blockedCountries }
  - create_approval_case(payload) -> { caseId, queue, slaHours }
  - post_timeline_event(payload) -> { eventId }
  - notify_approvers(payload) -> { notificationIds }
*/

function hasRegulatedData(dataClasses) {
  for (const dataClass of dataClasses) {
    if (
      dataClass.includes("pii") ||
      dataClass.includes("phi") ||
      dataClass.includes("cardholder")
    ) {
      return true;
    }
  }
  return false;
}

function toList(setLike) {
  const values = [];
  for (const value of setLike) {
    values.push(value);
  }
  return values;
}

const request = fetch_purchase_request(requestId);
const vendor = fetch_vendor_profile(request.vendorId);
const policy = fetch_policy_matrix(region, request.category, request.amountUsd);

const riskSignals = [];
const blockedDestinations = [];
const approvers = new Set();

for (const approver of policy.baseApprovers) {
  approvers.add(approver);
}

for (const country of request.destinationCountries) {
  if (policy.blockedCountries.includes(country)) {
    blockedDestinations.push(country);
  }
}

if (request.amountUsd > spendLimitUsd) {
  riskSignals.push("over_spend_limit");
}
if (vendor.onboardingState !== "approved") {
  riskSignals.push("vendor_not_fully_onboarded");
}
if (hasRegulatedData(request.dataClasses) && vendor.securityReviewStatus !== "approved") {
  riskSignals.push("security_review_required");
}
if (hasRegulatedData(request.dataClasses) && !vendor.hasDpa) {
  riskSignals.push("missing_dpa");
}
for (const country of request.destinationCountries) {
  if (!vendor.allowedRegions.includes(country)) {
    riskSignals.push("cross_border_data_transfer");
    break;
  }
}
if (blockedDestinations.length > 0) {
  riskSignals.push("blocked_destination");
}

for (const escalation of policy.escalationApprovers) {
  if (riskSignals.includes(escalation.when)) {
    approvers.add(escalation.approver);
  }
}

const approverChain = toList(approvers);

let recommendedDecision = "pre_approved";
let queue = "standard-procurement";
if (blockedDestinations.length > 0) {
  recommendedDecision = "deny";
  queue = "procurement-denials";
} else if (riskSignals.length > 0) {
  recommendedDecision = "manual_review";
  queue = "procurement-high-risk";
}

const approvalCase = create_approval_case({
  requestId: request.id,
  actorEmail,
  businessOwner: request.businessOwner,
  vendorId: vendor.id,
  vendorName: vendor.name,
  amountUsd: request.amountUsd,
  costCenter: request.costCenter,
  riskSignals,
  approverChain,
  recommendedDecision,
  requestedBy: request.requestedBy,
});

const timeline = post_timeline_event({
  requestId: request.id,
  actorEmail,
  caseId: approvalCase.caseId,
  message:
    "Created approval case " +
    approvalCase.caseId +
    " for vendor " +
    vendor.name +
    " with decision " +
    recommendedDecision,
});

const notifications = notify_approvers({
  caseId: approvalCase.caseId,
  queue: approvalCase.queue,
  approvers: approverChain,
  summary: {
    requestId: request.id,
    vendorName: vendor.name,
    amountUsd: request.amountUsd,
    riskSignals,
  },
});

const output = {};
output.requestId = request.id;
output.caseId = approvalCase.caseId;
output.timelineEventId = timeline.eventId;
output.notificationIds = notifications.notificationIds;
output.vendor = {
  id: vendor.id,
  name: vendor.name,
  onboardingState: vendor.onboardingState,
  securityReviewStatus: vendor.securityReviewStatus,
};
output.summary = {
  category: request.category,
  purpose: request.purpose,
  amountUsd: request.amountUsd,
  recommendedDecision,
  queue: approvalCase.queue,
  approverChain,
  riskSignals,
  blockedDestinations,
};

output;
