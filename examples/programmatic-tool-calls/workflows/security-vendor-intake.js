/*
Inputs:
  - intakeId: string

Capabilities:
  - list_vendor_findings(intakeId)
  - fetch_business_owner(intakeId)
*/

async function main() {
  const findings = await list_vendor_findings(intakeId);
  const owner = await fetch_business_owner(intakeId);
  const scored = [];

  for (const finding of findings) {
    scored.push({
      findingId: finding.id,
      severity: finding.severity,
      owner: owner.email,
    });
  }

  scored.sort((left, right) => right.severity - left.severity);
  scored;
}

main();
