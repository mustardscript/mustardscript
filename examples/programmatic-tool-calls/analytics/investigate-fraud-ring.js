/*
Inputs:
  - caseId: string
  - lookbackDays: number

Capabilities:
  - load_alert_case(caseId) -> { id, queue, primaryReason, flaggedAccountIds: string[] }
  - list_related_transactions(caseId, lookbackDays) -> [{ id, accountId, entityId, cardFingerprint, amount, outcome, ipAddress, email, deviceId, timestamp }]
  - fetch_device_clusters(accountIds) -> [{ clusterId, accounts: string[], devices: string[], riskLabel }]
  - fetch_chargeback_history(cardFingerprints) -> [{ cardFingerprint, chargebackRate, disputedAmount, count }]
  - lookup_identity_signals(entityIds) -> [{ entityId, syntheticRisk, watchlistHits, documentMismatch }]
  - search_internal_notes(caseId, query) -> [{ source, body }]
*/

async function investigateFraudRing() {
  const alertCase = await load_alert_case(caseId);
  const transactions = await list_related_transactions(caseId, lookbackDays);

  const accountIds = new Set();
  const entityIds = new Set();
  const cardFingerprints = new Set();

  for (const transaction of transactions) {
    accountIds.add(transaction.accountId);
    entityIds.add(transaction.entityId);
    cardFingerprints.add(transaction.cardFingerprint);
  }

  const [clusters, chargebacks, identitySignals, internalNotes] = await Promise.all([
    fetch_device_clusters(Array.from(accountIds)),
    fetch_chargeback_history(Array.from(cardFingerprints)),
    lookup_identity_signals(Array.from(entityIds)),
    search_internal_notes(caseId, "refund mule synthetic identity collusion"),
  ]);

  const chargebackByCard = new Map();
  for (const item of chargebacks) {
    chargebackByCard.set(item.cardFingerprint, item);
  }

  const identityByEntity = new Map();
  for (const signal of identitySignals) {
    identityByEntity.set(signal.entityId, signal);
  }

  const clusterByAccount = new Map();
  for (const cluster of clusters) {
    for (const accountId of cluster.accounts) {
      clusterByAccount.set(accountId, cluster);
    }
  }

  let approvedAmount = 0;
  let declinedAmount = 0;
  let rapidReuseCount = 0;
  const suspiciousTransactions = [];
  const ipCounts = new Map();

  for (const transaction of transactions) {
    if (transaction.outcome === "approved") {
      approvedAmount += transaction.amount;
    } else {
      declinedAmount += transaction.amount;
    }

    ipCounts.set(transaction.ipAddress, (ipCounts.get(transaction.ipAddress) ?? 0) + 1);

    const chargeback = chargebackByCard.get(transaction.cardFingerprint);
    const identity = identityByEntity.get(transaction.entityId);
    const cluster = clusterByAccount.get(transaction.accountId);
    const reasons = [];

    if (chargeback && chargeback.chargebackRate >= 0.18) {
      reasons.push("high_chargeback_card");
    }
    if (identity && identity.syntheticRisk >= 0.8) {
      reasons.push("synthetic_identity_signal");
    }
    if (identity && identity.watchlistHits > 0) {
      reasons.push("watchlist_overlap");
    }
    if (cluster && cluster.accounts.length >= 3) {
      reasons.push("shared_device_cluster");
    }
    if ((ipCounts.get(transaction.ipAddress) ?? 0) >= 3) {
      reasons.push("reused_ip");
    }
    if (transaction.email.includes("+") || transaction.email.includes("test")) {
      reasons.push("throwaway_email_pattern");
    }

    if (reasons.length > 0) {
      suspiciousTransactions.push({
        id: transaction.id,
        accountId: transaction.accountId,
        amount: transaction.amount,
        outcome: transaction.outcome,
        reasons,
      });
    }
  }

  for (const count of ipCounts.values()) {
    if (count >= 3) {
      rapidReuseCount += 1;
    }
  }

  const narrativeSignals = [];
  for (const note of internalNotes) {
    const compact = note.body
      .toLowerCase()
      .replaceAll(/\s+/g, " ")
      .replaceAll(/[^a-z0-9 ]+/g, " ");
    if (compact.includes("refund mule")) {
      narrativeSignals.push("refund_mule_language");
    }
    if (compact.includes("synthetic identity")) {
      narrativeSignals.push("synthetic_identity_language");
    }
    if (compact.includes("collusion")) {
      narrativeSignals.push("collusion_language");
    }
  }

  const escalationReasons = [];
  if (suspiciousTransactions.length >= 4) {
    escalationReasons.push("multi_transaction_pattern");
  }
  if (rapidReuseCount > 0) {
    escalationReasons.push("shared_infrastructure");
  }
  if (narrativeSignals.length >= 2) {
    escalationReasons.push("historical_case_overlap");
  }

  return {
    caseId: alertCase.id,
    queue: alertCase.queue,
    primaryReason: alertCase.primaryReason,
    accountCount: accountIds.size,
    transactionCount: transactions.length,
    approvedAmount,
    declinedAmount,
    suspiciousTransactionCount: suspiciousTransactions.length,
    rapidReuseCount,
    narrativeSignals,
    escalationReasons,
    suspiciousTransactions,
    recommendedDisposition:
      escalationReasons.length >= 2 ? "escalate_to_fraud_ops" : "continue_monitoring",
  };
}

investigateFraudRing();
