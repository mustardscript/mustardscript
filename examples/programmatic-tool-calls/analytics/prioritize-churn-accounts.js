/*
Inputs:
  - segment: string

Capabilities:
  - list_accounts(segment)
  - fetch_usage_snapshot(accountId)
  - list_support_threads(accountId)
*/

async function main() {
  const accounts = await list_accounts(segment);
  const risks = [];

  for (const account of accounts) {
    const usage = await fetch_usage_snapshot(account.id);
    const threads = await list_support_threads(account.id);
    const score =
      usage.activeSeats < usage.purchasedSeats ? 15 : 0 +
      (threads.length > 2 ? 10 : 0) +
      (account.renewalDays < 21 ? 20 : 0);

    risks.push({
      accountId: account.id,
      owner: account.owner,
      score,
      renewalDays: account.renewalDays,
    });
  }

  risks.sort((left, right) => right.score - left.score);
  risks.slice(0, 5);
}

main();
