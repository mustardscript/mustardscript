/*
Inputs:
  - accountId: string
  - requestedSeats: number

Capabilities:
  - load_account(accountId)
  - lookup_plan_policy(plan, requestedSeats)
  - create_quote(payload)
  - write_audit_log(payload)
*/

const account = load_account(accountId);
const policy = lookup_plan_policy(account.plan, requestedSeats);
const quote = create_quote({
  accountId: account.id,
  currentPlan: account.plan,
  requestedSeats: requestedSeats,
  targetPlan: policy.targetPlan,
});
const audit = write_audit_log({
  accountId: account.id,
  quoteId: quote.quoteId,
  requiresApproval: policy.requiresApproval,
});

const output = {};
output.accountId = account.id;
output.targetPlan = policy.targetPlan;
output.quoteId = quote.quoteId;
output.auditId = audit.auditId;
output.approvalMode = policy.requiresApproval ? "manual" : "automatic";
output;
