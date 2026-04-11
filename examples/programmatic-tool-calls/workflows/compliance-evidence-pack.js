/*
Inputs:
  - auditId: string

Capabilities:
  - list_control_evidence(auditId)
*/

async function main() {
  const evidence = await list_control_evidence(auditId);
  const pairs = [];
  for (const row of evidence) {
    pairs.push([row.controlId, row.link]);
  }
  Object.fromEntries(pairs);
}

main();
