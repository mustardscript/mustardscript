/*
Inputs:
  - warehouseId: string
  - waveId: string

Capabilities:
  - list_wave_orders(warehouseId, waveId) -> [{ orderId, priority, lines }]
  - list_inventory_exceptions(warehouseId, waveId) -> [{ sku, type, availableQty }]
  - list_pick_tickets(warehouseId, waveId) -> [{ orderId, sku, requestedQty, pickedQty }]
  - list_customer_promises(waveId) -> [{ orderId, shipBy, expedite }]
  - lookup_substitution_policy(warehouseId) -> {
      allowSubstitution, protectedSkus: string[]
    }
*/

async function coordinateWarehouseException() {
  const [orders, exceptions, tickets, promises, policy] = await Promise.all([
    list_wave_orders(warehouseId, waveId),
    list_inventory_exceptions(warehouseId, waveId),
    list_pick_tickets(warehouseId, waveId),
    list_customer_promises(waveId),
    lookup_substitution_policy(warehouseId),
  ]);

  const exceptionBySku = new Map();
  for (const exception of exceptions) {
    exceptionBySku.set(exception.sku, exception);
  }

  const picksByOrderSku = new Map();
  for (const ticket of tickets) {
    picksByOrderSku.set(ticket.orderId + "::" + ticket.sku, ticket);
  }

  const promiseByOrder = new Map();
  for (const promise of promises) {
    promiseByOrder.set(promise.orderId, promise);
  }

  const interventions = [];
  let blockedOrders = 0;

  for (const order of orders) {
    let orderBlocked = false;
    const promise = promiseByOrder.get(order.orderId);

    for (const line of order.lines) {
      const exception = exceptionBySku.get(line.sku);
      const pick = picksByOrderSku.get(order.orderId + "::" + line.sku);
      const shortPicked = pick && pick.pickedQty < pick.requestedQty;
      const protectedSku = policy.protectedSkus.includes(line.sku);

      if (exception || shortPicked) {
        orderBlocked = true;

        let action = "hold";
        if (
          policy.allowSubstitution &&
          !protectedSku &&
          exception &&
          exception.type === "out_of_stock" &&
          line.substituteSku
        ) {
          action = "substitute";
        } else if (promise && promise.expedite) {
          action = "split_and_expedite";
        } else if (exception && exception.type === "damaged") {
          action = "refund";
        }

        interventions.push({
          orderId: order.orderId,
          priority: order.priority,
          sku: line.sku,
          action,
          reason: exception ? exception.type : "short_pick",
          shipBy: promise ? promise.shipBy : null,
        });
      }
    }

    if (orderBlocked) {
      blockedOrders += 1;
    }
  }

  return {
    warehouseId,
    waveId,
    orderCount: orders.length,
    blockedOrders,
    interventionCount: interventions.length,
    interventions,
  };
}

coordinateWarehouseException();
