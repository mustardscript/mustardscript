/*
Inputs:
  - modelId: string
  - windowHours: number

Capabilities:
  - list_recent_deploys(modelId) -> [{ id, version, minutesAgo, initiator }]
  - fetch_model_metrics(modelId, metric, windowHours) -> { metric, current, baseline, status }
  - fetch_feature_drift(modelId) -> [{ feature, driftScore, topSegment }]
  - list_annotation_issues(modelId) -> [{ queue, severity, summary }]
  - get_rollback_playbook(modelId) -> { immediateActions: string[], rollbackTarget }
*/

async function triageModelRegression() {
  const metricNames = ["precision", "recall", "latency_ms", "fallback_rate"];
  const metricPromises = [];
  for (const metric of metricNames) {
    metricPromises.push(fetch_model_metrics(modelId, metric, windowHours));
  }

  const [deploys, metricResults, drift, annotationIssues, playbook] = await Promise.all([
    list_recent_deploys(modelId),
    Promise.allSettled(metricPromises),
    fetch_feature_drift(modelId),
    list_annotation_issues(modelId),
    get_rollback_playbook(modelId),
  ]);

  const degradedMetrics = [];
  const metricSnapshots = [];
  for (const result of metricResults) {
    if (result.status === "fulfilled") {
      metricSnapshots.push(result.value);
      if (result.value.status !== "healthy") {
        degradedMetrics.push(result.value.metric);
      }
    } else {
      degradedMetrics.push("metric_unavailable");
    }
  }

  const driftHotspots = [];
  for (const entry of drift) {
    if (entry.driftScore >= 0.35) {
      driftHotspots.push({
        feature: entry.feature,
        driftScore: entry.driftScore,
        topSegment: entry.topSegment,
      });
    }
  }

  let severity = "medium";
  if (degradedMetrics.includes("precision") || degradedMetrics.includes("recall")) {
    severity = "high";
  }
  if (
    degradedMetrics.includes("precision") &&
    degradedMetrics.includes("fallback_rate") &&
    driftHotspots.length > 0
  ) {
    severity = "critical";
  }

  const hypotheses = [];
  if (deploys.length > 0 && degradedMetrics.length > 0) {
    hypotheses.push("recent_model_or_feature_rollout");
  }
  if (driftHotspots.length > 0) {
    hypotheses.push("feature_distribution_shift");
  }
  if (annotationIssues.some((issue) => issue.severity === "high")) {
    hypotheses.push("label_quality_or_queue_backlog");
  }
  if (hypotheses.length === 0) {
    hypotheses.push("needs_manual_root_cause_analysis");
  }

  const recommendedActions = [];
  for (const action of playbook.immediateActions) {
    recommendedActions.push(action);
  }
  if (severity === "critical" && playbook.rollbackTarget) {
    recommendedActions.push("rollback_to_" + playbook.rollbackTarget);
  }

  return {
    modelId,
    windowHours,
    severity,
    degradedMetrics,
    deploys,
    driftHotspots,
    annotationIssues,
    hypotheses,
    recommendedActions,
    metricSnapshots,
  };
}

triageModelRegression();
