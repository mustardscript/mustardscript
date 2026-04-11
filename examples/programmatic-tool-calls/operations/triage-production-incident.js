/*
Inputs:
  - service: string

Capabilities:
  - list_recent_alerts(service)
  - list_recent_deploys(service)
  - search_logs(service, query)
  - fetch_metric_window(service, metric)
*/

async function main() {
  const alerts = await list_recent_alerts(service);
  const deploys = await list_recent_deploys(service);
  const logs = await search_logs(service, "timeout OR saturation OR rollback");

  const metrics = [];
  metrics.push(await fetch_metric_window(service, "error_rate"));
  metrics.push(await fetch_metric_window(service, "p95_latency"));
  metrics.push(await fetch_metric_window(service, "cpu_saturation"));

  const degraded = [];
  for (const metric of metrics) {
    if (metric.status === "degraded") {
      degraded.push(metric.metric);
    }
  }

  const output = {};
  output.service = service;
  output.alertCount = alerts.length;
  output.recentDeploys = deploys.length;
  output.degradedMetrics = degraded;
  output.timeoutSignals = logs.join(" ").match(/timeout|rollback|saturation/g) ?? [];
  return output;
}

main();
