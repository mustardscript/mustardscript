/*
Inputs:
  - team: string
  - shiftStart: string
  - shiftEnd: string

Capabilities:
  - list_open_incidents(team) -> [{ id, severity, service, status, summary }]
  - list_recent_pages(team, shiftStart, shiftEnd) -> [{ service, summary, count }]
  - list_muted_alerts(team) -> [{ service, signal, reason, expiresInMinutes }]
  - list_pending_followups(team) -> [{ owner, item, dueInMinutes }]
  - search_runbook_notes(team, query) -> string[]
  - list_scheduled_changes(team) -> [{ changeId, service, startsInMinutes, risk }]
*/

async function stabilizeOncallHandoff() {
  const [incidents, pages, mutedAlerts, followups, notes, changes] =
    await Promise.all([
      list_open_incidents(team),
      list_recent_pages(team, shiftStart, shiftEnd),
      list_muted_alerts(team),
      list_pending_followups(team),
      search_runbook_notes(team, "rollback OR saturation OR flaky"),
      list_scheduled_changes(team),
    ]);

  const repeatedPagesByService = new Map();
  for (const page of pages) {
    repeatedPagesByService.set(
      page.service,
      (repeatedPagesByService.get(page.service) ?? 0) + page.count,
    );
  }

  const noteSignals = [];
  for (const note of notes) {
    const matched = note.toLowerCase().match(/rollback|saturation|flaky|drain/g) ?? [];
    for (const token of matched) {
      noteSignals.push(token);
    }
  }

  const urgentItems = [];
  for (const incident of incidents) {
    if (incident.severity === "critical" || incident.status !== "mitigated") {
      urgentItems.push({
        kind: "incident",
        service: incident.service,
        summary: incident.summary,
      });
    }
  }

  for (const muted of mutedAlerts) {
    if (muted.expiresInMinutes <= 180) {
      urgentItems.push({
        kind: "muted_alert",
        service: muted.service,
        summary: muted.signal + " muted for " + muted.reason,
      });
    }
  }

  for (const change of changes) {
    if (change.risk === "high" || change.startsInMinutes <= 60) {
      urgentItems.push({
        kind: "scheduled_change",
        service: change.service,
        summary: "Change " + change.changeId + " starts in " + change.startsInMinutes + "m",
      });
    }
  }

  const followupBacklog = [];
  for (const followup of followups) {
    if (followup.dueInMinutes <= 240) {
      followupBacklog.push(followup);
    }
  }

  const briefingLines = [];
  briefingLines.push("Team " + team + " handled " + pages.length + " page groups this shift");
  briefingLines.push("Open incidents: " + incidents.length);
  if ((repeatedPagesByService.get("checkout") ?? 0) >= 3) {
    briefingLines.push("Checkout paged repeatedly and needs a fresh owner at handoff");
  }
  if (noteSignals.includes("rollback")) {
    briefingLines.push("Runbook notes mention a recent rollback and possible follow-up cleanup");
  }

  return {
    team,
    shiftStart,
    shiftEnd,
    openIncidentCount: incidents.length,
    urgentItemCount: urgentItems.length,
    mutedAlertCount: mutedAlerts.length,
    followupBacklogCount: followupBacklog.length,
    noteSignals,
    urgentItems,
    followupBacklog,
    briefingLines,
  };
}

stabilizeOncallHandoff();
