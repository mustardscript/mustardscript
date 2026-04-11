/*
Inputs:
  - portfolioId: string
  - downsideScenario: string

Capabilities:
  - list_portfolio_positions(portfolioId) -> [{ ticker, sector, marketValue, beta, thesis, hedgeable }]
  - fetch_factor_shocks(downsideScenario) -> [{ sector, drawdownPct, betaMultiplier }]
  - fetch_liquidity_profile(tickers) -> [{ ticker, daysToExit, averageDailyVolumePct }]
  - fetch_hedge_candidates(portfolioId, downsideScenario) -> [{ ticker, instrument, expectedProtectionPct, carryCostBps }]
  - fetch_risk_limits(portfolioId) -> { maxSingleNameLoss, maxPortfolioDrawdown, concentrationLimitPct }
*/

async function buildCapitalAllocationBrief() {
  const positions = await list_portfolio_positions(portfolioId);
  const tickers = [];
  for (const position of positions) {
    tickers.push(position.ticker);
  }

  const [factorShocks, liquidity, hedges, limits] = await Promise.all([
    fetch_factor_shocks(downsideScenario),
    fetch_liquidity_profile(tickers),
    fetch_hedge_candidates(portfolioId, downsideScenario),
    fetch_risk_limits(portfolioId),
  ]);

  const shockBySector = new Map();
  for (const shock of factorShocks) {
    shockBySector.set(shock.sector, shock);
  }

  const liquidityByTicker = new Map();
  for (const item of liquidity) {
    liquidityByTicker.set(item.ticker, item);
  }

  const hedgeByTicker = new Map();
  for (const hedge of hedges) {
    hedgeByTicker.set(hedge.ticker, hedge);
  }

  const positionImpacts = [];
  let projectedDrawdown = 0;
  for (const position of positions) {
    const shock = shockBySector.get(position.sector);
    const liquidityProfile = liquidityByTicker.get(position.ticker);
    const hedge = hedgeByTicker.get(position.ticker);
    const drawdownPct =
      (shock?.drawdownPct ?? 0.08) * (shock?.betaMultiplier ?? 1) * position.beta;
    const grossLoss = position.marketValue * drawdownPct;
    const hedgeProtection = hedge ? grossLoss * hedge.expectedProtectionPct : 0;
    const netLoss = grossLoss - hedgeProtection;

    projectedDrawdown += netLoss;
    positionImpacts.push({
      ticker: position.ticker,
      sector: position.sector,
      thesis: position.thesis,
      marketValue: position.marketValue,
      drawdownPct,
      grossLoss,
      hedgeProtection,
      netLoss,
      daysToExit: liquidityProfile?.daysToExit ?? null,
      carryCostBps: hedge?.carryCostBps ?? null,
      hedgeable: position.hedgeable,
    });
  }

  const rankedLosses = positionImpacts.sort((left, right) => right.netLoss - left.netLoss);
  const priorityNames = [];
  for (const impact of rankedLosses) {
    if (priorityNames.length < 3) {
      priorityNames.push(impact.ticker);
    }
  }

  const actions = [];
  if (projectedDrawdown > limits.maxPortfolioDrawdown) {
    actions.push("reduce_gross_exposure");
  }
  if (rankedLosses.some((impact) => impact.netLoss > limits.maxSingleNameLoss)) {
    actions.push("trim_single_name_risk");
  }
  if (rankedLosses.some((impact) => impact.daysToExit !== null && impact.daysToExit > 4)) {
    actions.push("prehedge_illiquid_positions");
  }

  return {
    portfolioId,
    downsideScenario,
    projectedDrawdown,
    maxPortfolioDrawdown: limits.maxPortfolioDrawdown,
    priorityNames,
    actions,
    rankedLosses,
  };
}

buildCapitalAllocationBrief();
