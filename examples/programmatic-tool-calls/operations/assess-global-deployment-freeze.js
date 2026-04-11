/*
Inputs:
  - since: string
  - services: string[]

Capabilities:
  - list_recent_incidents(since) -> [{ id, severity, service, startedAt, summary }]
  - list_active_changes() -> [{ changeId, service, stage, startedAt, owner }]
  - list_staffing_exceptions() -> [{ team, kind, endsAt }]
  - fetch_service_tier(service) -> { service, tier, ownerTeam }
  - list_customer_commitments(service) -> [{ account, eventAt, revenueBand }]
*/

async function assessGlobalDeploymentFreeze() {
  const [incidents, activeChanges, staffing] = await Promise.all([
    list_recent_incidents(since),
    list_active_changes(),
    list_staffing_exceptions(),
  ]);

  const serviceFacts = await Promise.all(
    services.map(async (service) => {
      const [tier, commitments] = await Promise.all([
        fetch_service_tier(service),
        list_customer_commitments(service),
      ]);
      return {
        service,
        tier,
        commitments,
      };
    }),
  );

  const commitmentsByService = Object.fromEntries(
    serviceFacts.map((entry) => [entry.service, entry.commitments]),
  );

  const latestSevereIncidentAt = incidents
    .filter((incident) => incident.severity === "sev0" || incident.severity === "sev1")
    .map((incident) => new Date(incident.startedAt).getTime())
    .sort((left, right) => right - left)[0];

  const hasThinStaffing = staffing.some(
    (entry) => entry.kind === "primary_oncall_gap" || entry.kind === "release_manager_gap",
  );

  const rankedServices = serviceFacts
    .map((entry) => {
      const changeCount = activeChanges.filter(
        (change) => change.service === entry.service,
      ).length;
      const commitmentCount = (commitmentsByService[entry.service] ?? []).length;
      let riskScore = 0;

      if (entry.tier.tier === 0) {
        riskScore += 5;
      } else if (entry.tier.tier === 1) {
        riskScore += 3;
      }
      if (changeCount > 0) {
        riskScore += 2;
      }
      if (commitmentCount > 0) {
        riskScore += 2;
      }

      return {
        service: entry.service,
        ownerTeam: entry.tier.ownerTeam,
        changeCount,
        commitmentCount,
        riskScore,
      };
    })
    .sort((left, right) => right.riskScore - left.riskScore);

  let freezeDecision = "no_freeze";
  if (latestSevereIncidentAt && Date.now() - latestSevereIncidentAt < 6 * 60 * 60 * 1000) {
    freezeDecision = "freeze_all_non_emergency_changes";
  } else if (hasThinStaffing && activeChanges.length > 0) {
    freezeDecision = "freeze_high_risk_changes";
  }

  const blockedServices = rankedServices.filter(
    (entry) => freezeDecision !== "no_freeze" && entry.riskScore >= 4,
  );
  const waiverCandidates = rankedServices.filter(
    (entry) => entry.riskScore <= 2 && entry.changeCount === 0,
  );

  return {
    since,
    freezeDecision,
    activeChangeCount: activeChanges.length,
    recentIncidentCount: incidents.length,
    staffingExceptions: staffing,
    blockedServices,
    waiverCandidates,
  };
}

assessGlobalDeploymentFreeze();
