/*
Inputs:
  - ledger: string

Capabilities:
  - list_cost_centers(ledger)
  - fetch_spend_series(costCenterId)
*/

async function main() {
  const today = Date.now();
  const centers = await list_cost_centers(ledger);
  const anomalies = [];

  for (const center of centers) {
    const series = await fetch_spend_series(center.id);
    const latest = series[series.length - 1];
    if (latest.amount > latest.baseline * 1.4) {
      anomalies.push({
        costCenterId: center.id,
        checkedAt: today,
        latestAmount: latest.amount,
        baseline: latest.baseline,
      });
    }
  }

  anomalies;
}

main();
