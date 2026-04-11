/*
Inputs:
  - team: string

Capabilities:
  - list_open_alerts(team)
  - list_pending_followups(team)
  - fetch_shift_notes(team)
*/

async function main() {
  const alerts = await list_open_alerts(team);
  const followups = await list_pending_followups(team);
  const notes = await fetch_shift_notes(team);

  const summary = [];
  summary.push("Open alerts: " + alerts.length);
  summary.push("Pending followups: " + followups.length);
  summary.push("Top note: " + notes[0]);

  const output = {};
  output.team = team;
  output.summary = summary;
  return output;
}

main();
