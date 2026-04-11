/*
Inputs:
  - service: string

Capabilities:
  - list_recent_alerts(service) -> [{ severity, summary }]
  - list_recent_deploys(service) -> [{ id, version, minutesAgo }]
  - fetch_metric_window(service, metric) -> { metric, status, latest, baseline }
  - search_logs(service, query) -> string[]
  - get_runbook(service) -> { immediateActions: string[], rollbackHint: string }
*/

async function triageProductionIncident() {
  const metricNames = ["error_rate", "p95_latency", "cpu_saturation"];
  const metricRequests = [];
  for (const metric of metricNames) {
    metricRequests.push(fetch_metric_window(service, metric));
  }

  const [alerts, deploys, runbook, logLines, metricResults] = await Promise.all([
    list_recent_alerts(service),
    list_recent_deploys(service),
    get_runbook(service),
    search_logs(service, "timeout OR saturation OR rollback"),
    Promise.allSettled(metricRequests),
  ]);

  const degradedMetrics = [];
  const metricSnapshots = [];
  for (const result of metricResults) {
    if (result.status === "fulfilled") {
      metricSnapshots.push(result.value);
      if (result.value.status === "degraded") {
        degradedMetrics.push(result.value.metric);
      }
    } else {
      degradedMetrics.push("metric_unavailable");
    }
  }

  const joinedLogs = logLines.join(" ").toLowerCase();
  const matchingErrorTokens = joinedLogs.match(/timeout|saturation|rollback/g) ?? [];

  let severity = "medium";
  for (const alert of alerts) {
    if (alert.severity === "critical") {
      severity = "critical";
    } else if (severity !== "critical" && alert.severity === "high") {
      severity = "high";
    }
  }

  const suspectedCauses = [];
  if (deploys.length > 0 && degradedMetrics.includes("error_rate")) {
    suspectedCauses.push("recent_rollout_regression");
  }
  if (matchingErrorTokens.includes("timeout")) {
    suspectedCauses.push("downstream_timeouts");
  }
  if (matchingErrorTokens.includes("saturation")) {
    suspectedCauses.push("capacity_pressure");
  }
  if (suspectedCauses.length === 0) {
    suspectedCauses.push("needs_manual_triage");
  }

  const immediateActions = [];
  for (const action of runbook.immediateActions) {
    immediateActions.push(action);
  }
  if (
    suspectedCauses.includes("recent_rollout_regression") &&
    runbook.rollbackHint
  ) {
    immediateActions.push(runbook.rollbackHint);
  }

  return {
    service,
    severity,
    alertCount: alerts.length,
    degradedMetrics,
    suspectedCauses,
    immediateActions,
    recentDeploys: deploys.map((deploy) => ({
      id: deploy.id,
      version: deploy.version,
      minutesAgo: deploy.minutesAgo,
    })),
    metricSnapshots,
    matchingErrorTokens,
  };
}

triageProductionIncident();
