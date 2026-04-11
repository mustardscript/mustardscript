/*
Inputs:
  - disputeId: string
  - merchantId: string
  - network: string

Capabilities:
  - fetch_dispute_case(disputeId, merchantId, network) -> { id, reasonCode, amountUsd, cardholderClaim, orderId }
  - fetch_order_timeline(orderId) -> [{ stage, at, actor, details }]
  - fetch_customer_communications(orderId) -> [{ direction, channel, body, at }]
  - fetch_fulfillment_events(orderId) -> [{ type, carrier, status, at, signedBy }]
  - submit_evidence_packet(payload) -> { packetId, status }
*/

function compactText(text) {
  return text.replace(/\s+/g, " ").trim();
}

async function buildChargebackPacket() {
  const dispute = await fetch_dispute_case(disputeId, merchantId, network);

  const loaded = await Promise.all([
    fetch_order_timeline(dispute.orderId),
    fetch_customer_communications(dispute.orderId),
    fetch_fulfillment_events(dispute.orderId),
  ]);

  const timeline = loaded[0];
  const communications = loaded[1];
  const fulfillment = loaded[2];

  const timelineHighlights = [];
  for (const entry of timeline) {
    if (
      entry.stage === "checkout_completed" ||
      entry.stage === "fraud_screen_passed" ||
      entry.stage === "refund_declined"
    ) {
      timelineHighlights.push({
        stage: entry.stage,
        at: entry.at,
        actor: entry.actor,
      });
    }
  }

  const communicationSummary = [];
  for (const message of communications) {
    const normalized = compactText(message.body);
    if (
      normalized.includes("confirm") ||
      normalized.includes("received") ||
      normalized.includes("download")
    ) {
      communicationSummary.push({
        direction: message.direction,
        channel: message.channel,
        excerpt: normalized.slice(0, 140),
      });
    }
  }

  let signedDelivery = null;
  const fulfillmentHighlights = [];
  for (const event of fulfillment) {
    if (
      event.type === "shipment_created" ||
      event.type === "out_for_delivery" ||
      event.type === "delivered"
    ) {
      fulfillmentHighlights.push(event);
    }
    if (event.type === "delivered" && event.signedBy) {
      signedDelivery = {
        carrier: event.carrier,
        signedBy: event.signedBy,
        at: event.at,
      };
    }
  }

  const rebuttals = [];
  if (dispute.reasonCode === "fraud") {
    rebuttals.push("order passed fraud controls before fulfillment");
  }
  if (communicationSummary.length > 0) {
    rebuttals.push("customer communications show post-purchase engagement");
  }
  if (signedDelivery) {
    rebuttals.push("carrier recorded signed delivery");
  }

  const packet = {
    disputeId: dispute.id,
    merchantId,
    network,
    amountUsd: dispute.amountUsd,
    reasonCode: dispute.reasonCode,
    cardholderClaim: dispute.cardholderClaim,
    timelineHighlights,
    communicationSummary,
    fulfillmentHighlights,
    signedDelivery,
    rebuttals,
  };

  const submitted = await submit_evidence_packet(packet);

  return {
    disputeId: dispute.id,
    packetId: submitted.packetId,
    status: submitted.status,
    rebuttalCount: rebuttals.length,
    hasSignedDelivery: signedDelivery !== null,
    timelineHighlights,
    communicationSummary,
  };
}

buildChargebackPacket();
