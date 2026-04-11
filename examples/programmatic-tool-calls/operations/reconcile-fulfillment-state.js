/*
Inputs:
  - accountId: string

Capabilities:
  - list_orders(accountId)
  - list_payments(accountId)
  - list_shipments(accountId)
*/

async function main() {
  const orders = await list_orders(accountId);
  const payments = await list_payments(accountId);
  const shipments = await list_shipments(accountId);
  const issues = [];

  for (const order of orders) {
    let payment = null;
    let shipment = null;

    for (const candidate of payments) {
      if (candidate.orderId === order.id) {
        payment = candidate;
      }
    }
    for (const candidate of shipments) {
      if (candidate.orderId === order.id) {
        shipment = candidate;
      }
    }

    if (!payment) {
      issues.push("missing_payment:" + order.id);
    }
    if (!shipment && (order.status === "ready_to_ship" || order.status === "delivered")) {
      issues.push("missing_shipment:" + order.id);
    }
  }

  const output = {};
  output.accountId = accountId;
  output.issueCount = issues.length;
  output.issues = issues;
  return output;
}

main();
