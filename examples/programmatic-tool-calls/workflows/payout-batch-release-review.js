/*
Inputs:
  - payoutBatchId: string
  - analystId: string
  - region: string
  - releaseThresholdUsd: number

Capabilities:
  - fetch_payout_batch(payoutBatchId) -> { id, currency, payouts }
  - list_account_flags(accountIds) -> [{ accountId, code, severity, state }]
  - list_recent_transactions(accountIds) -> [{ accountId, type, amountUsd, destinationCountry, disputed }]
  - fetch_ruleset(region) -> { maxDisputedVolumeUsd, crossBorderReviewRegions, hardStopCodes }
  - record_release_decision(payload) -> { decisionId, state }
*/

async function reviewPayoutBatch() {
  const batch = await fetch_payout_batch(payoutBatchId);

  const accountIds = [];
  const seenAccounts = new Set();
  for (const payout of batch.payouts) {
    if (!seenAccounts.has(payout.accountId)) {
      seenAccounts.add(payout.accountId);
      accountIds.push(payout.accountId);
    }
  }

  const settled = await Promise.all([
    list_account_flags(accountIds),
    list_recent_transactions(accountIds),
    fetch_ruleset(region),
  ]);

  const flags = settled[0];
  const transactions = settled[1];
  const ruleset = settled[2];

  const flagsByAccount = new Map();
  for (const flag of flags) {
    let bucket = flagsByAccount.get(flag.accountId);
    if (!bucket) {
      bucket = [];
      flagsByAccount.set(flag.accountId, bucket);
    }
    bucket.push(flag);
  }

  const transactionsByAccount = new Map();
  for (const transaction of transactions) {
    let bucket = transactionsByAccount.get(transaction.accountId);
    if (!bucket) {
      bucket = [];
      transactionsByAccount.set(transaction.accountId, bucket);
    }
    bucket.push(transaction);
  }

  const holds = [];
  const releases = [];
  let holdAmountUsd = 0;
  let releaseAmountUsd = 0;

  for (const payout of batch.payouts) {
    const accountFlags = flagsByAccount.get(payout.accountId) || [];
    const recentTransactions = transactionsByAccount.get(payout.accountId) || [];
    const reasons = [];

    if (payout.amountUsd >= releaseThresholdUsd) {
      reasons.push("threshold_review");
    }

    for (const flag of accountFlags) {
      if (ruleset.hardStopCodes.includes(flag.code)) {
        reasons.push("hard_stop_flag");
        break;
      }
      if (/(chargeback|sanction|synthetic)/i.test(flag.code)) {
        reasons.push("risk_flag_present");
        break;
      }
    }

    let disputedVolumeUsd = 0;
    for (const transaction of recentTransactions) {
      if (transaction.disputed) {
        disputedVolumeUsd += transaction.amountUsd;
      }
      if (
        transaction.destinationCountry !== region &&
        ruleset.crossBorderReviewRegions.includes(region)
      ) {
        reasons.push("cross_border_activity");
      }
    }
    if (disputedVolumeUsd > ruleset.maxDisputedVolumeUsd) {
      reasons.push("dispute_volume_exceeded");
    }

    const record = {
      payoutId: payout.payoutId,
      accountId: payout.accountId,
      amountUsd: payout.amountUsd,
      reasons,
    };

    if (reasons.length > 0) {
      holds.push(record);
      holdAmountUsd += payout.amountUsd;
    } else {
      releases.push(record);
      releaseAmountUsd += payout.amountUsd;
    }
  }

  const decision = await record_release_decision({
    payoutBatchId,
    analystId,
    region,
    holds,
    releases,
  });

  return {
    payoutBatchId,
    currency: batch.currency,
    decisionId: decision.decisionId,
    state: decision.state,
    holdCount: holds.length,
    releaseCount: releases.length,
    holdAmountUsd,
    releaseAmountUsd,
    holds,
    releases,
  };
}

reviewPayoutBatch();
