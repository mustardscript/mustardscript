/*
Inputs:
  - service: string

Capabilities:
  - list_pending_changes(service)
  - fetch_test_summary(service)
  - list_open_incidents(service)
*/

async function main() {
  const changes = await list_pending_changes(service);
  const tests = await fetch_test_summary(service);
  const incidents = await list_open_incidents(service);

  let decision = "ship";
  if (tests.failed > 0 || incidents.length > 0) {
    decision = "hold";
  } else if (changes.length > 5) {
    decision = "review";
  }

  const output = {};
  output.service = service;
  output.pendingChanges = changes.length;
  output.failedTests = tests.failed;
  output.openIncidents = incidents.length;
  output.decision = decision;
  return output;
}

main();
