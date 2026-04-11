/*
Inputs:
  - changeId: string
  - service: string
  - canaryRegions: string[]

Capabilities:
  - load_change_request(changeId) -> {
      id, service, version, risk, createdAt, summary, expectedBlastRadius
    }
  - fetch_canary_metric(service, region, metric) -> {
      metric, latest, baseline, status
    }
  - list_related_incidents(service) -> [{ id, severity, minutesAgo, summary }]
  - get_release_guardrails(service) -> {
      maxErrorRate, maxLatencyP95, minSuccessRate, rollbackOnIncidentCount
    }
  - inspect_feature_flags(changeId) -> [{ name, state, owner }]
*/

async function guardPaymentsRollout() {
  const metricNames = ["error_rate", "p95_latency", "success_rate"];
  const metricRequests = [];

  for (const region of canaryRegions) {
    for (const metric of metricNames) {
      metricRequests.push(fetch_canary_metric(service, region, metric));
    }
  }

  const [change, incidents, guardrails, flags, metricResults] = await Promise.all([
    load_change_request(changeId),
    list_related_incidents(service),
    get_release_guardrails(service),
    inspect_feature_flags(changeId),
    Promise.all(metricRequests),
  ]);

  const scorecards = [];
  let cursor = 0;

  for (const region of canaryRegions) {
    const metrics = {};
    for (const metric of metricNames) {
      metrics[metric] = metricResults[cursor];
      cursor += 1;
    }

    const blockers = [];
    let riskScore = 0;

    if (metrics.error_rate.latest > guardrails.maxErrorRate) {
      blockers.push("error_rate_regression");
      riskScore += 4;
    }
    if (metrics.p95_latency.latest > guardrails.maxLatencyP95) {
      blockers.push("latency_regression");
      riskScore += 3;
    }
    if (metrics.success_rate.latest < guardrails.minSuccessRate) {
      blockers.push("success_rate_drop");
      riskScore += 4;
    }

    scorecards.push({
      region,
      riskScore,
      blockers,
      metrics,
    });
  }

  for (const flag of flags) {
    if (flag.state === "bypass") {
      for (const scorecard of scorecards) {
        scorecard.blockers.push("unsafe_flag_" + flag.name);
        scorecard.riskScore += 2;
      }
    }
  }

  if (incidents.length >= guardrails.rollbackOnIncidentCount) {
    for (const scorecard of scorecards) {
      scorecard.blockers.push("active_incident_pressure");
      scorecard.riskScore += 2;
    }
  }

  scorecards.sort((left, right) => right.riskScore - left.riskScore);

  let decision = "promote";
  if (scorecards.length > 0 && scorecards[0].riskScore >= 7) {
    decision = "rollback";
  } else if (scorecards.length > 0 && scorecards[0].riskScore >= 3) {
    decision = "hold";
  }

  const recommendedActions = [];
  if (decision === "rollback") {
    recommendedActions.push("rollback " + change.version + " before expanding the canary");
  }
  if (decision === "hold") {
    recommendedActions.push("freeze further rollout and collect another metric window");
  }
  if (flags.length > 0) {
    recommendedActions.push("review flag overrides attached to the change request");
  }
  if (incidents.length > 0) {
    recommendedActions.push("check whether the rollout is amplifying an already-open incident");
  }

  return {
    changeId: change.id,
    service,
    version: change.version,
    decision,
    changeRisk: change.risk,
    expectedBlastRadius: change.expectedBlastRadius,
    scorecards,
    activeIncidentCount: incidents.length,
    recommendedActions,
  };
}

guardPaymentsRollout();
