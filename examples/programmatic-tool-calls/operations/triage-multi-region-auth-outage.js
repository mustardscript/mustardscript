/*
Inputs:
  - incidentId: string
  - service: string
  - regions: string[]

Capabilities:
  - load_incident_timeline(incidentId) -> [{ ts, kind, note }]
  - list_regional_alerts(service, region) -> [{ severity, signal, fingerprint, summary }]
  - fetch_service_slo(service, region) -> { region, availability, latencyP95, errorRate }
  - search_error_samples(service, incidentId, region) -> string[]
  - get_mitigation_runbook(service) -> {
      owners: string[],
      immediateActions: string[],
      rollbackTriggers: string[]
    }
*/

async function triageMultiRegionAuthOutage() {
  const alertRequests = [];
  const sloRequests = [];
  const sampleRequests = [];

  for (const region of regions) {
    alertRequests.push(list_regional_alerts(service, region));
    sloRequests.push(fetch_service_slo(service, region));
    sampleRequests.push(search_error_samples(service, incidentId, region));
  }

  const [timeline, runbook, alertBatches, sloWindows, sampleBatches] =
    await Promise.all([
      load_incident_timeline(incidentId),
      get_mitigation_runbook(service),
      Promise.all(alertRequests),
      Promise.all(sloRequests),
      Promise.all(sampleRequests),
    ]);

  const impactedRegions = [];
  const regionalFindings = [];
  const seenFingerprints = new Set();
  const dedupedAlerts = [];
  const tokenCounts = new Map();

  for (let index = 0; index < regions.length; index += 1) {
    const region = regions[index];
    const slo = sloWindows[index];
    const alerts = alertBatches[index];
    const samples = sampleBatches[index];
    const regionSignals = [];

    for (const alert of alerts) {
      if (!seenFingerprints.has(alert.fingerprint)) {
        seenFingerprints.add(alert.fingerprint);
        dedupedAlerts.push(alert);
      }
      if (alert.severity === "critical" || alert.severity === "high") {
        regionSignals.push(alert.signal);
      }
    }

    const joinedSamples = samples.join(" ").toLowerCase();
    const matchedTokens =
      joinedSamples.match(/jwks|timeout|token|rate limit|dns|certificate/g) ?? [];
    for (const token of matchedTokens) {
      tokenCounts.set(token, (tokenCounts.get(token) ?? 0) + 1);
    }

    const isImpacted =
      slo.availability < 99.5 ||
      slo.errorRate > 0.03 ||
      slo.latencyP95 > 900 ||
      alerts.length > 0;

    if (isImpacted) {
      impactedRegions.push(region);
    }

    regionalFindings.push({
      region,
      availability: slo.availability,
      latencyP95: slo.latencyP95,
      errorRate: slo.errorRate,
      alertCount: alerts.length,
      dominantSignals: regionSignals,
      errorTokens: matchedTokens,
    });
  }

  let severity = "medium";
  if (impactedRegions.length >= 2) {
    severity = "high";
  }
  for (const alert of dedupedAlerts) {
    if (alert.severity === "critical") {
      severity = "critical";
    }
  }

  const suspectedCauses = [];
  if ((tokenCounts.get("jwks") ?? 0) > 0 || (tokenCounts.get("certificate") ?? 0) > 0) {
    suspectedCauses.push("identity_key_distribution");
  }
  if ((tokenCounts.get("dns") ?? 0) > 0) {
    suspectedCauses.push("regional_dns_path");
  }
  if ((tokenCounts.get("timeout") ?? 0) > 1) {
    suspectedCauses.push("downstream_dependency_timeout");
  }
  if ((tokenCounts.get("rate limit") ?? 0) > 0) {
    suspectedCauses.push("provider_rate_limit");
  }

  const timelineNarrative = [];
  for (const event of timeline) {
    const compact = event.note.replaceAll("\n", " ").trim();
    if (
      compact.includes("deploy") ||
      compact.includes("rollback") ||
      compact.includes("certificate") ||
      compact.includes("dns")
    ) {
      timelineNarrative.push(compact);
    }
  }

  const immediateActions = [];
  for (const action of runbook.immediateActions) {
    immediateActions.push(action);
  }
  if (suspectedCauses.includes("identity_key_distribution")) {
    immediateActions.push("verify the newest signing keys and cert chain in every region");
  }
  if (suspectedCauses.includes("regional_dns_path")) {
    immediateActions.push("shift authentication traffic away from the worst region");
  }
  if (timelineNarrative.join(" ").includes("deploy")) {
    immediateActions.push("check whether the newest auth deployment must be rolled back");
  }

  return {
    incidentId,
    service,
    severity,
    impactedRegions,
    regionalFindings,
    alertCount: dedupedAlerts.length,
    suspectedCauses,
    timelineNarrative,
    runbookOwners: runbook.owners,
    immediateActions,
  };
}

triageMultiRegionAuthOutage();
