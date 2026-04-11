/*
Inputs:
  - renewalId: string

Capabilities:
  - load_renewal(renewalId)
  - fetch_discount_policy(segment, amount)
  - create_approval_request(payload)
*/

const renewal = load_renewal(renewalId);
const policy = fetch_discount_policy(renewal.segment, renewal.amount);
const request = create_approval_request({
  renewalId: renewal.id,
  amount: renewal.amount,
  approver: policy.approver,
});

const output = {};
output.renewalId = renewal.id;
output.policy = policy.rule;
output.approver = policy.approver;
output.requestId = request.requestId;
output;
