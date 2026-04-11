/*
Inputs:
  - accountId: string

Capabilities:
  - list_orders(accountId) -> [{ id, total, status }]
  - list_payments(accountId) -> [{ orderId, amount, status }]
  - list_shipments(accountId) -> [{ orderId, carrier, status }]
*/

async function reconcileFulfillmentState() {
  const [orders, payments, shipments] = await Promise.all([
    list_orders(accountId),
    list_payments(accountId),
    list_shipments(accountId),
  ]);

  const paymentByOrder = new Map();
  for (const payment of payments) {
    paymentByOrder.set(payment.orderId, payment);
  }

  const shipmentByOrder = new Map();
  for (const shipment of shipments) {
    shipmentByOrder.set(shipment.orderId, shipment);
  }

  const actionItems = [];
  let grossRevenue = 0;
  let capturedRevenue = 0;
  let shippedOrders = 0;

  for (const order of orders) {
    grossRevenue += order.total;
    const payment = paymentByOrder.get(order.id);
    const shipment = shipmentByOrder.get(order.id);
    const issues = [];
    let paymentStatus = "missing";
    let shipmentStatus = "missing";

    if (!payment) {
      issues.push("missing_payment");
    } else {
      paymentStatus = payment.status;
      if (payment.status === "captured") {
        capturedRevenue += payment.amount;
      }
      if (payment.amount !== order.total) {
        issues.push("payment_amount_mismatch");
      }
      if (payment.status !== "captured") {
        if (order.status !== "cancelled") {
          issues.push("payment_not_captured");
        }
      }
    }

    if (shipment) {
      shippedOrders += 1;
      shipmentStatus = shipment.status;
      if (order.status === "cancelled") {
        issues.push("shipment_exists_for_cancelled_order");
      }
      if (shipment.status !== "delivered") {
        if (order.status === "delivered") {
          issues.push("shipment_status_lags_order_status");
        }
      }
    }

    if (!shipment) {
      if (order.status === "ready_to_ship") {
        issues.push("missing_shipment");
      }
      if (order.status === "delivered") {
        issues.push("missing_shipment");
      }
    }

    if (issues.length > 0) {
      const item = {};
      item.orderId = order.id;
      item.orderStatus = order.status;
      item.paymentStatus = paymentStatus;
      item.shipmentStatus = shipmentStatus;
      item.issues = issues;
      actionItems.push(item);
    }
  }

  const result = {};
  result.accountId = accountId;
  result.orderCount = orders.length;
  result.grossRevenue = grossRevenue;
  result.capturedRevenue = capturedRevenue;
  result.shippedOrders = shippedOrders;
  result.openActionCount = actionItems.length;
  result.actionItems = actionItems;
  return result;
}

reconcileFulfillmentState();
