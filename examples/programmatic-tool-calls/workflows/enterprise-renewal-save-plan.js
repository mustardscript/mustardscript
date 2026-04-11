/*
Inputs:
  - accountId: string
  - renewalWindowDays: number
  - targetExpansionSeats: number

Capabilities:
  - fetch_account_summary(accountId) -> { id, name, currentPlan, currentSeats, arrUsd, renewalDateIso, csmEmail }
  - fetch_product_usage(accountId) -> { activeUsers, seatUtilization, adoption }
  - list_open_support_cases(accountId) -> [{ caseId, severity, theme, ageDays, status }]
  - fetch_billing_history(accountId) -> [{ invoiceId, status, amountUsd, daysLate }]
  - create_success_plan(payload) -> { planId, owner }
  - log_account_note(payload) -> { noteId }
*/

async function buildRenewalSavePlan() {
  const account = await fetch_account_summary(accountId);

  const outcomes = await Promise.allSettled([
    fetch_product_usage(accountId),
    list_open_support_cases(accountId),
    fetch_billing_history(accountId),
  ]);

  const usage = outcomes[0].status === "fulfilled" ? outcomes[0].value : null;
  const supportCases = outcomes[1].status === "fulfilled" ? outcomes[1].value : [];
  const invoices = outcomes[2].status === "fulfilled" ? outcomes[2].value : [];

  const riskSignals = [];
  const productGaps = [];
  const supportThemes = new Map();

  if (usage) {
    if (usage.seatUtilization < 0.7) {
      riskSignals.push("underutilized_seats");
    }
    for (const metric of usage.adoption) {
      if (metric.status !== "healthy") {
        productGaps.push(metric.area);
      }
    }
  } else {
    riskSignals.push("usage_data_unavailable");
  }

  for (const supportCase of supportCases) {
    if (supportCase.status !== "resolved" && supportCase.ageDays > 14) {
      riskSignals.push("aged_support_case");
    }
    const current = supportThemes.get(supportCase.theme) || 0;
    supportThemes.set(supportCase.theme, current + 1);
  }

  let lateInvoiceCount = 0;
  let outstandingUsd = 0;
  for (const invoice of invoices) {
    if (invoice.status !== "paid") {
      outstandingUsd += invoice.amountUsd;
    }
    if (invoice.daysLate > 0) {
      lateInvoiceCount += 1;
    }
  }
  if (lateInvoiceCount > 0) {
    riskSignals.push("billing_friction");
  }

  const recommendedActions = [];
  if (productGaps.length > 0) {
    recommendedActions.push("schedule targeted adoption workshop");
  }
  if (lateInvoiceCount > 0) {
    recommendedActions.push("coordinate renewal with finance and procurement");
  }
  if (supportCases.length > 0) {
    recommendedActions.push("review open support themes with engineering");
  }
  if ((usage && usage.activeUsers >= account.currentSeats * 0.85) || targetExpansionSeats > 0) {
    recommendedActions.push("position seat expansion with proof of usage");
  }

  const supportThemeSummary = [];
  for (const entry of supportThemes.entries()) {
    supportThemeSummary.push({
      theme: entry[0],
      count: entry[1],
    });
  }

  const successPlan = await create_success_plan({
    accountId,
    csmEmail: account.csmEmail,
    riskSignals,
    productGaps,
    supportThemeSummary,
    outstandingUsd,
    targetExpansionSeats,
    recommendedActions,
  });

  const note = await log_account_note({
    accountId,
    planId: successPlan.planId,
    note:
      "Generated renewal save plan with " +
      riskSignals.length +
      " risk signals and " +
      recommendedActions.length +
      " recommended actions.",
  });

  return {
    accountId,
    accountName: account.name,
    currentPlan: account.currentPlan,
    currentSeats: account.currentSeats,
    arrUsd: account.arrUsd,
    renewalWindowDays,
    successPlanId: successPlan.planId,
    noteId: note.noteId,
    riskSignals,
    productGaps,
    outstandingUsd,
    supportThemes: supportThemeSummary,
    recommendedActions,
  };
}

buildRenewalSavePlan();
