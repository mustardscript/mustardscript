/*
Inputs:
  - ticketId: string
  - customerId: string
  - nowIso: string
  - targetSlaMinutes: number

Capabilities:
  - fetch_ticket_thread(ticketId) -> { id, priority, severity, productArea, summary, lastCustomerReplyAt, thread }
  - fetch_customer_360(customerId) -> { id, name, plan, arrUsd, renewalDateIso, successOwner, executiveSponsor, openEscalations }
  - list_open_incidents(productArea) -> [{ id, severity, startedAt, title, status }]
  - search_internal_notes(customerId, query) -> [{ noteId, title, body, createdAt }]
  - post_support_brief(payload) -> { briefId, postedChannel }
*/

async function runVipSupportEscalation() {
  const loaded = await Promise.all([
    fetch_ticket_thread(ticketId),
    fetch_customer_360(customerId),
    list_open_incidents("billing-and-identity"),
    search_internal_notes(customerId, "renewal risk or exec escalation"),
  ]);

  const ticket = loaded[0];
  const customer = loaded[1];
  const incidents = loaded[2];
  const notes = loaded[3];

  const nowMs = new Date(nowIso).getTime();
  const lastReplyMs = new Date(ticket.lastCustomerReplyAt).getTime();
  const minutesSinceLastCustomerReply = Math.trunc((nowMs - lastReplyMs) / 60000);
  const minutesToBreach = targetSlaMinutes - minutesSinceLastCustomerReply;

  const severityRank = {
    critical: 4,
    high: 3,
    medium: 2,
    low: 1,
  };

  const matchingIncidents = incidents
    .filter((incident) => {
      return (
        incident.status !== "resolved" &&
        (incident.title.toLowerCase().includes(ticket.productArea) ||
          ticket.summary.toLowerCase().includes("login") ||
          ticket.summary.toLowerCase().includes("invoice"))
      );
    })
    .sort((left, right) => severityRank[right.severity] - severityRank[left.severity]);

  const noteHits = [];
  for (const note of notes) {
    const normalized = note.body.toLowerCase();
    if (
      normalized.includes("renewal risk") ||
      normalized.includes("exec") ||
      normalized.includes("legal escalation")
    ) {
      noteHits.push({
        noteId: note.noteId,
        title: note.title,
      });
    }
  }

  const actionQueue = [
    {
      label: "page owning engineer",
      priority: matchingIncidents.length > 0 ? 100 : 75,
    },
    {
      label: "notify customer success owner",
      priority: customer.arrUsd > 250000 ? 95 : 60,
    },
    {
      label: "stage executive update",
      priority: noteHits.length > 0 || customer.openEscalations > 0 ? 90 : 40,
    },
    {
      label: "offer workaround from prior notes",
      priority: noteHits.length > 0 ? 80 : 20,
    },
  ];
  actionQueue.sort((left, right) => right.priority - left.priority);

  const brief = {
    ticketId: ticket.id,
    customerId: customer.id,
    customerName: customer.name,
    supportPriority: ticket.priority,
    plan: customer.plan,
    arrUsd: customer.arrUsd,
    summary: ticket.summary,
    minutesSinceLastCustomerReply,
    minutesToBreach,
    relatedIncidents: matchingIncidents,
    noteHits,
    recommendedActions: actionQueue.map((action) => action.label),
  };

  const posted = await post_support_brief({
    channel: minutesToBreach <= 15 ? "support-war-room" : "vip-support",
    brief,
  });

  brief.briefId = posted.briefId;
  brief.postedChannel = posted.postedChannel;
  return brief;
}

runVipSupportEscalation();
