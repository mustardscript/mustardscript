/*
Inputs:
  - accountId: string
  - requestedSeats: number

Capabilities:
  - load_account(accountId) -> { id, name, plan, seats, owner }
  - lookup_plan_policy(plan, requestedSeats) -> { targetPlan, requiresApproval, monthlyDelta }
  - create_quote(payload) -> { quoteId, expiresInDays }
  - write_audit_log(payload) -> { auditId }
*/

const account = load_account(accountId);
const policy = lookup_plan_policy(account.plan, requestedSeats);
const quote = create_quote({
  accountId: account.id,
  currentPlan: account.plan,
  targetPlan: policy.targetPlan,
  requestedSeats,
  monthlyDelta: policy.monthlyDelta,
  owner: account.owner,
});
const audit = write_audit_log({
  accountId: account.id,
  quoteId: quote.quoteId,
  requiresApproval: policy.requiresApproval,
  actor: "guest_workflow",
});

let approvalMode = "manual";
if (!policy.requiresApproval) {
  approvalMode = "automatic";
}

const summary = [];
summary[summary.length] =
  "Account " + account.name + " requested " + requestedSeats + " seats";
summary[summary.length] = "Current plan: " + account.plan;
summary[summary.length] = "Target plan: " + policy.targetPlan;
summary[summary.length] = "Approval mode: " + approvalMode;

const result = {};
result.accountId = account.id;
result.accountName = account.name;
result.currentPlan = account.plan;
result.currentSeats = account.seats;
result.requestedSeats = requestedSeats;
result.targetPlan = policy.targetPlan;
result.approvalMode = approvalMode;
result.monthlyDelta = policy.monthlyDelta;
result.quoteId = quote.quoteId;
result.quoteExpiresInDays = quote.expiresInDays;
result.auditId = audit.auditId;
result.summary = summary;

result;
