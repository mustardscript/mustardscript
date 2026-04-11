/*
Inputs:
  - payoutBatchId: string
  - sellerIds: string[]

Capabilities:
  - list_batch_orders(payoutBatchId) -> [{ orderId, sellerId, grossAmount, status }]
  - list_batch_refunds(payoutBatchId) -> [{ orderId, sellerId, amount, reason }]
  - list_payout_lines(payoutBatchId) -> [{ sellerId, amount, status, transferId }]
  - list_ledger_adjustments(payoutBatchId) -> [{ sellerId, amount, type }]
  - load_seller_contract(sellerId) -> { sellerId, feeBps, reservePct, settlementMode }
*/

async function reconcileMarketplacePayouts() {
  const contractRequests = [];
  for (const sellerId of sellerIds) {
    contractRequests.push(load_seller_contract(sellerId));
  }

  const [orders, refunds, payoutLines, adjustments, contracts] = await Promise.all([
    list_batch_orders(payoutBatchId),
    list_batch_refunds(payoutBatchId),
    list_payout_lines(payoutBatchId),
    list_ledger_adjustments(payoutBatchId),
    Promise.all(contractRequests),
  ]);

  const contractBySeller = new Map();
  for (const contract of contracts) {
    contractBySeller.set(contract.sellerId, contract);
  }

  const refundsByOrder = new Map();
  for (const refund of refunds) {
    const next = (refundsByOrder.get(refund.orderId) ?? 0) + refund.amount;
    refundsByOrder.set(refund.orderId, next);
  }

  const adjustmentsBySeller = new Map();
  for (const adjustment of adjustments) {
    const next =
      (adjustmentsBySeller.get(adjustment.sellerId) ?? 0) + adjustment.amount;
    adjustmentsBySeller.set(adjustment.sellerId, next);
  }

  const payoutBySeller = new Map();
  for (const payoutLine of payoutLines) {
    payoutBySeller.set(payoutLine.sellerId, payoutLine);
  }

  const sellerSummaries = [];
  const mismatches = [];

  for (const sellerId of sellerIds) {
    const contract = contractBySeller.get(sellerId);
    const sellerOrders = [];
    for (const order of orders) {
      if (order.sellerId === sellerId) {
        sellerOrders.push(order);
      }
    }

    let grossAmount = 0;
    let refundedAmount = 0;
    let processingFees = 0;

    for (const order of sellerOrders) {
      grossAmount += order.grossAmount;
      refundedAmount += refundsByOrder.get(order.orderId) ?? 0;
      processingFees += Math.trunc((order.grossAmount * contract.feeBps) / 10000);
    }

    const reserveAmount = Math.trunc((grossAmount * contract.reservePct) / 100);
    const adjustmentAmount = adjustmentsBySeller.get(sellerId) ?? 0;
    const expectedNet =
      grossAmount - refundedAmount - processingFees - reserveAmount + adjustmentAmount;

    const payoutLine = payoutBySeller.get(sellerId);
    const actualNet = payoutLine ? payoutLine.amount : 0;
    const delta = actualNet - expectedNet;

    const summary = {
      sellerId,
      settlementMode: contract.settlementMode,
      orderCount: sellerOrders.length,
      grossAmount,
      refundedAmount,
      processingFees,
      reserveAmount,
      adjustmentAmount,
      expectedNet,
      actualNet,
      delta,
    };
    sellerSummaries.push(summary);

    if (!payoutLine || delta !== 0 || payoutLine.status !== "released") {
      const reasons = [];
      if (!payoutLine) {
        reasons.push("missing_payout_line");
      } else {
        if (delta !== 0) {
          reasons.push("amount_mismatch");
        }
        if (payoutLine.status !== "released") {
          reasons.push("payout_not_released");
        }
      }

      mismatches.push({
        sellerId,
        reasons,
        expectedNet,
        actualNet,
        transferId: payoutLine ? payoutLine.transferId : null,
      });
    }
  }

  return {
    payoutBatchId,
    sellerCount: sellerIds.length,
    mismatchCount: mismatches.length,
    sellerSummaries,
    mismatches,
  };
}

reconcileMarketplacePayouts();
