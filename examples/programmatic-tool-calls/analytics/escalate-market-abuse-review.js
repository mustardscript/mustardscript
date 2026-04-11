/*
Inputs:
  - investigationId: string

Capabilities:
  - load_surveillance_alert(investigationId) -> { alertId, clientId, strategy, trigger, windowStart, windowEnd }
  - load_order_timeline(alertId) -> [{ orderId, side, symbol, notional, event }]
  - load_client_profile(clientId) -> { clientId, desk, jurisdiction, priorWarnings }
  - load_comms_hits(clientId, windowStart, windowEnd) -> [{ channel, excerpt }]
  - load_watchlist_hits(clientId) -> [{ list, reason }]
*/

const alert = load_surveillance_alert(investigationId);
const orders = load_order_timeline(alert.alertId);
const client = load_client_profile(alert.clientId);
const commsHits = load_comms_hits(client.clientId, alert.windowStart, alert.windowEnd);
const watchlistHits = load_watchlist_hits(client.clientId);

let totalNotional = 0;
let layeringEvents = 0;
let cancelEvents = 0;
for (const order of orders) {
  totalNotional += order.notional;
  if (order.event === "layer_added") {
    layeringEvents += 1;
  }
  if (order.event === "cancelled_near_touch") {
    cancelEvents += 1;
  }
}

const communicationSignals = [];
for (const hit of commsHits) {
  const compact = hit.excerpt.toLowerCase().replaceAll(/\s+/g, " ");
  if (compact.includes("paint the tape")) {
    communicationSignals.push("manipulative_language");
  }
  if (compact.includes("keep it under the radar")) {
    communicationSignals.push("concealment_language");
  }
  if (compact.includes("close it before the print")) {
    communicationSignals.push("timing_language");
  }
}

const reasons = [];
if (layeringEvents >= 2 && cancelEvents >= 2) {
  reasons.push("layering_pattern");
}
if (communicationSignals.length > 0) {
  reasons.push("supporting_comms");
}
if (client.priorWarnings > 0) {
  reasons.push("prior_supervisory_history");
}
if (watchlistHits.length > 0) {
  reasons.push("watchlist_overlap");
}

const disposition =
  reasons.length >= 3 ? "escalate_to_compliance_committee" : "continue_supervisory_review";

({
  investigationId,
  alertId: alert.alertId,
  clientId: client.clientId,
  desk: client.desk,
  strategy: alert.strategy,
  trigger: alert.trigger,
  totalNotional,
  layeringEvents,
  cancelEvents,
  communicationSignals,
  watchlistHits,
  reasons,
  disposition,
});
