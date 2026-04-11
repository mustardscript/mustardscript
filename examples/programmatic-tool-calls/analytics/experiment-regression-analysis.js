/*
Inputs:
  - experimentKey: string

Capabilities:
  - list_experiment_variants(experimentKey)
  - fetch_metric_window(experimentKey, variant, metric)
*/

async function main() {
  const variants = await list_experiment_variants(experimentKey);
  const comparisons = [];

  for (const variant of variants) {
    const conversion = await fetch_metric_window(experimentKey, variant.key, "conversion_rate");
    const latency = await fetch_metric_window(experimentKey, variant.key, "p95_latency");
    const row = {};
    row.variant = variant.key;
    row.conversion = conversion.latest;
    row.latency = latency.latest;
    row.regression = latency.latest > latency.baseline && conversion.latest < conversion.baseline;
    comparisons.push(row);
  }

  const output = {};
  output.experimentKey = experimentKey;
  output.comparisons = comparisons;
  return output;
}

main();
