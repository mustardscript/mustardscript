/*
Inputs:
  - ticketId: string

Capabilities:
  - fetch_ticket(ticketId)
  - fetch_customer_context(customerId)
  - fetch_recent_incidents(service)
*/

async function main() {
  const ticket = await fetch_ticket(ticketId);
  const customer = await fetch_customer_context(ticket.customerId);
  const incidents = await fetch_recent_incidents(ticket.service);

  const output = {};
  output.ticketId = ticketId;
  output.priority = customer.tier === "enterprise" ? "high" : "normal";
  output.incidentCount = incidents.length;
  output.summary = [
    "Customer: " + customer.name,
    "Service: " + ticket.service,
    "Open incidents: " + incidents.length,
  ];
  return output;
}

main();
