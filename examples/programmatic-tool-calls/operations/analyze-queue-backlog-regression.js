/*
Inputs:
  - pipeline: string
  - regions: string[]

Capabilities:
  - list_queue_shards(pipeline, region) -> [{ shardId, workerPool }]
  - fetch_shard_metrics(pipeline, region, shardId) -> {
      depth, oldestAgeSec, inflowPerMin, outflowPerMin, retryRate
    }
  - fetch_worker_pool_status(workerPool) -> {
      pool, saturation, unavailableWorkers, queuedRestarts
    }
  - sample_dead_letters(pipeline, region, shardId) -> [{ code, message }]
  - get_capacity_plan(pipeline) -> {
      maxHealthyDepth, maxHealthyAgeSec, escalationActions: string[]
    }
*/

async function analyzeQueueBacklogRegression() {
  const [capacityPlan, shardBatches] = await Promise.all([
    get_capacity_plan(pipeline),
    Promise.all(regions.map((region) => list_queue_shards(pipeline, region))),
  ]);

  const shardRecords = [];

  for (let regionIndex = 0; regionIndex < regions.length; regionIndex += 1) {
    const region = regions[regionIndex];
    const shards = shardBatches[regionIndex];

    for (const shard of shards) {
      const [metrics, workerPool, deadLetters] = await Promise.all([
        fetch_shard_metrics(pipeline, region, shard.shardId),
        fetch_worker_pool_status(shard.workerPool),
        sample_dead_letters(pipeline, region, shard.shardId),
      ]);

      const joinedErrors = deadLetters
        .map((entry) => entry.code + " " + entry.message)
        .join(" ")
        .toLowerCase();
      const tokens =
        joinedErrors.match(/timeout|throttle|schema|duplicate|poison|serialize/g) ?? [];

      shardRecords.push({
        region,
        shardId: shard.shardId,
        workerPool: shard.workerPool,
        depth: metrics.depth,
        oldestAgeSec: metrics.oldestAgeSec,
        inflowPerMin: metrics.inflowPerMin,
        outflowPerMin: metrics.outflowPerMin,
        retryRate: metrics.retryRate,
        saturation: workerPool.saturation,
        unavailableWorkers: workerPool.unavailableWorkers,
        queuedRestarts: workerPool.queuedRestarts,
        tokens,
      });
    }
  }

  const hotspots = [];
  const causeCounts = new Map();

  for (const shard of shardRecords) {
    const unhealthy =
      shard.depth > capacityPlan.maxHealthyDepth ||
      shard.oldestAgeSec > capacityPlan.maxHealthyAgeSec ||
      shard.retryRate > 0.08;

    if (unhealthy) {
      hotspots.push(shard);
    }

    for (const token of shard.tokens) {
      causeCounts.set(token, (causeCounts.get(token) ?? 0) + 1);
    }
  }

  const suspectedDrivers = [];
  if ((causeCounts.get("throttle") ?? 0) > 0) {
    suspectedDrivers.push("downstream_throttling");
  }
  if ((causeCounts.get("schema") ?? 0) > 0 || (causeCounts.get("serialize") ?? 0) > 0) {
    suspectedDrivers.push("schema_or_payload_regression");
  }
  if ((causeCounts.get("duplicate") ?? 0) > 0 || (causeCounts.get("poison") ?? 0) > 0) {
    suspectedDrivers.push("retry_storm");
  }
  if ((causeCounts.get("timeout") ?? 0) > 0) {
    suspectedDrivers.push("dependency_timeouts");
  }

  const actionPlan = [];
  for (const action of capacityPlan.escalationActions) {
    actionPlan.push(action);
  }
  if (suspectedDrivers.includes("retry_storm")) {
    actionPlan.push("pause requeue for poison messages and quarantine the hottest shard");
  }
  if (suspectedDrivers.includes("downstream_throttling")) {
    actionPlan.push("reduce producer concurrency until downstream quotas recover");
  }

  return {
    pipeline,
    regions,
    shardCount: shardRecords.length,
    hotspotCount: hotspots.length,
    hotspots,
    suspectedDrivers,
    actionPlan,
  };
}

analyzeQueueBacklogRegression();
