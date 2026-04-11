/*
Inputs:
  - portfolio: string

Capabilities:
  - list_vendors(portfolio)
  - fetch_audit_findings(vendorId)
  - fetch_security_questionnaire(vendorId)
*/

async function main() {
  const vendors = await list_vendors(portfolio);
  const flagged = [];

  for (const vendor of vendors) {
    const findings = await fetch_audit_findings(vendor.id);
    const questionnaire = await fetch_security_questionnaire(vendor.id);
    let severity = "low";
    if (findings.openCritical > 0 || questionnaire.dataResidency === "unknown") {
      severity = "high";
    } else if (findings.openHigh > 0 || questionnaire.sso !== true) {
      severity = "medium";
    }

    if (severity !== "low") {
      const row = {};
      row.vendorId = vendor.id;
      row.vendorName = vendor.name;
      row.severity = severity;
      row.openCritical = findings.openCritical;
      row.openHigh = findings.openHigh;
      row.dataResidency = questionnaire.dataResidency;
      flagged.push(row);
    }
  }

  const output = {};
  output.portfolio = portfolio;
  output.flagged = flagged;
  output.reviewCount = flagged.length;
  return output;
}

main();
