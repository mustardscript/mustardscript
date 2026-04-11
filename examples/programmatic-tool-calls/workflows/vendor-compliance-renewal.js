/*
Inputs:
  - vendorId: string
  - reviewCycleId: string
  - requiredFrameworks: string[]

Capabilities:
  - fetch_vendor_master(vendorId) -> { id, name, serviceTier, hostsCustomerData, primaryCountry }
  - fetch_control_evidence(vendorId) -> [{ framework, type, status, ageDays }]
  - fetch_data_flow_inventory(vendorId) -> [{ system, originCountry, destinationCountry, dataClasses }]
  - fetch_subprocessor_list(vendorId) -> [{ name, country, hasDpa, countryRisk }]
  - file_vendor_review(payload) -> { reviewRecordId, state }
*/

async function runVendorComplianceRenewal() {
  const loaded = await Promise.all([
    fetch_vendor_master(vendorId),
    fetch_control_evidence(vendorId),
    fetch_data_flow_inventory(vendorId),
    fetch_subprocessor_list(vendorId),
  ]);

  const vendor = loaded[0];
  const evidence = loaded[1];
  const dataFlows = loaded[2];
  const subprocessors = loaded[3];

  const evidenceByFramework = Object.fromEntries(
    requiredFrameworks.map((framework) => {
      return [
        framework,
        evidence.filter((item) => item.framework === framework),
      ];
    }),
  );

  const missingFrameworks = [];
  const staleEvidence = [];

  for (const framework of requiredFrameworks) {
    const frameworkEvidence = evidenceByFramework[framework];
    if (!frameworkEvidence || frameworkEvidence.length === 0) {
      missingFrameworks.push(framework);
      continue;
    }

    let hasCurrentAttestation = false;
    for (const item of frameworkEvidence) {
      if (item.status === "current" && item.ageDays <= 365) {
        hasCurrentAttestation = true;
      }
      if (item.status !== "current" || item.ageDays > 365) {
        staleEvidence.push({
          framework,
          type: item.type,
          status: item.status,
          ageDays: item.ageDays,
        });
      }
    }

    if (!hasCurrentAttestation) {
      missingFrameworks.push(framework);
    }
  }

  const crossBorderFlows = [];
  for (const flow of dataFlows) {
    if (
      flow.originCountry !== flow.destinationCountry &&
      flow.dataClasses.some((dataClass) => {
        return dataClass.includes("pii") || dataClass.includes("customer");
      })
    ) {
      crossBorderFlows.push(flow);
    }
  }

  const riskySubprocessors = subprocessors.filter((subprocessor) => {
    return subprocessor.countryRisk !== "low" || !subprocessor.hasDpa;
  });

  const recommendedDecision =
    missingFrameworks.length === 0 &&
    riskySubprocessors.length === 0 &&
    crossBorderFlows.length <= 1
      ? "approve"
      : "manual_review";

  const filed = await file_vendor_review({
    vendorId,
    reviewCycleId,
    vendorName: vendor.name,
    serviceTier: vendor.serviceTier,
    hostsCustomerData: vendor.hostsCustomerData,
    missingFrameworks,
    staleEvidence,
    riskySubprocessors,
    crossBorderFlows,
    recommendedDecision,
  });

  return {
    vendorId,
    vendorName: vendor.name,
    reviewCycleId,
    reviewRecordId: filed.reviewRecordId,
    state: filed.state,
    recommendedDecision,
    missingFrameworks,
    staleEvidence,
    riskySubprocessors,
    crossBorderFlows,
  };
}

runVendorComplianceRenewal();
