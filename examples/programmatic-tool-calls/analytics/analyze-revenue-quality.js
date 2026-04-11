/*
Inputs:
  - quarter: string
  - materialityThreshold: number

Capabilities:
  - list_business_units(quarter) -> [{ id, name, segment, owner }]
  - load_unit_actuals(unitId, quarter) -> { recognizedRevenue, deferredRevenue, churnedArr, dso, collectionsAtRisk }
  - load_unit_forecast(unitId, quarter) -> { committedRevenue, stretchRevenue, pipelineCoverage }
  - load_unit_deal_changes(unitId, quarter) -> [{ opportunityId, movement, amount, reason, account }]
  - list_collection_risks(quarter) -> [{ unitId, accountId, balance, reason, daysPastDue }]
*/

async function analyzeRevenueQuality() {
  const units = await list_business_units(quarter);
  const collectionRisks = await list_collection_risks(quarter);

  const unitTasks = [];
  for (const unit of units) {
    unitTasks.push(
      Promise.all([
        load_unit_actuals(unit.id, quarter),
        load_unit_forecast(unit.id, quarter),
        load_unit_deal_changes(unit.id, quarter),
      ]),
    );
  }

  const resolvedUnits = await Promise.all(unitTasks);
  const riskByUnit = new Map();
  for (const entry of collectionRisks) {
    const bucket = riskByUnit.get(entry.unitId) ?? [];
    bucket.push(entry);
    riskByUnit.set(entry.unitId, bucket);
  }

  const unitSummaries = [];
  const watchlist = [];
  let totalRecognizedRevenue = 0;
  let totalCommittedRevenue = 0;
  let totalCollectionsAtRisk = 0;

  for (let index = 0; index < units.length; index += 1) {
    const unit = units[index];
    const [actuals, forecast, dealChanges] = resolvedUnits[index];
    const riskEntries = riskByUnit.get(unit.id) ?? [];

    totalRecognizedRevenue += actuals.recognizedRevenue;
    totalCommittedRevenue += forecast.committedRevenue;
    totalCollectionsAtRisk += actuals.collectionsAtRisk;

    let slippedPipeline = 0;
    let upsidePipeline = 0;
    const notableMovements = [];
    for (const change of dealChanges) {
      if (change.movement === "slipped" || change.movement === "pushed") {
        slippedPipeline += change.amount;
      } else if (change.movement === "pulled_forward" || change.movement === "expanded") {
        upsidePipeline += change.amount;
      }
      if (Math.abs(change.amount) >= materialityThreshold) {
        notableMovements.push({
          opportunityId: change.opportunityId,
          movement: change.movement,
          amount: change.amount,
          reason: change.reason,
          account: change.account,
        });
      }
    }

    const variance = actuals.recognizedRevenue - forecast.committedRevenue;
    const riskSignals = [];
    if (variance < -materialityThreshold) {
      riskSignals.push("commit_miss");
    }
    if (actuals.dso >= 58) {
      riskSignals.push("elevated_dso");
    }
    if (forecast.pipelineCoverage < 1.15) {
      riskSignals.push("thin_pipeline");
    }
    if (slippedPipeline >= materialityThreshold) {
      riskSignals.push("slipped_pipeline");
    }
    if (riskEntries.length > 0) {
      riskSignals.push("collections_pressure");
    }

    const summary = {
      unitId: unit.id,
      unitName: unit.name,
      segment: unit.segment,
      owner: unit.owner,
      recognizedRevenue: actuals.recognizedRevenue,
      committedRevenue: forecast.committedRevenue,
      stretchRevenue: forecast.stretchRevenue,
      variance,
      deferredRevenue: actuals.deferredRevenue,
      churnedArr: actuals.churnedArr,
      dso: actuals.dso,
      collectionsAtRisk: actuals.collectionsAtRisk,
      pipelineCoverage: forecast.pipelineCoverage,
      slippedPipeline,
      upsidePipeline,
      riskSignals,
      notableMovements,
    };

    unitSummaries.push(summary);

    if (riskSignals.length >= 2) {
      watchlist.push({
        unitId: unit.id,
        unitName: unit.name,
        riskSignals,
        collectionsAccounts: riskEntries.map((entry) => ({
          accountId: entry.accountId,
          balance: entry.balance,
          reason: entry.reason,
          daysPastDue: entry.daysPastDue,
        })),
      });
    }
  }

  let worstUnit = null;
  for (const summary of unitSummaries) {
    if (!worstUnit || summary.variance < worstUnit.variance) {
      worstUnit = summary;
    }
  }

  return {
    quarter,
    unitCount: unitSummaries.length,
    totalRecognizedRevenue,
    totalCommittedRevenue,
    revenueVariance: totalRecognizedRevenue - totalCommittedRevenue,
    totalCollectionsAtRisk,
    boardNarrative: {
      headline:
        totalRecognizedRevenue >= totalCommittedRevenue
          ? "Revenue landed at or above commit, but quality remains mixed."
          : "Revenue landed below commit and quality signals deteriorated.",
      worstUnit:
        worstUnit === null
          ? null
          : {
              unitId: worstUnit.unitId,
              unitName: worstUnit.unitName,
              variance: worstUnit.variance,
              riskSignals: worstUnit.riskSignals,
            },
      watchlistCount: watchlist.length,
    },
    watchlist,
    unitSummaries,
  };
}

analyzeRevenueQuality();
