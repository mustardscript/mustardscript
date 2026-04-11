/*
Inputs:
  - reviewId: string

Capabilities:
  - list_review_items(reviewId)
  - fetch_manager_attestation(userId)
*/

async function main() {
  const items = await list_review_items(reviewId);
  const decisions = [];

  for (const item of items) {
    const attestation = await fetch_manager_attestation(item.userId);
    const row = {};
    row.userId = item.userId;
    row.resource = item.resource;
    row.decision = attestation.approved ? "keep" : "remove";
    decisions.push(row);
  }

  const output = {};
  output.reviewId = reviewId;
  output.decisions = decisions;
  return output;
}

main();
