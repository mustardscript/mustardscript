/*
Inputs:
  - queueName: string

Capabilities:
  - list_backlog_jobs(queueName)
  - fetch_customer_tier(customerId)
*/

async function main() {
  const jobs = await list_backlog_jobs(queueName);
  const prioritized = [];

  for (const job of jobs) {
    const tier = await fetch_customer_tier(job.customerId);
    prioritized.push({
      jobId: job.id,
      customerId: job.customerId,
      priority: tier === "enterprise" ? 100 : 10,
      attempts: job.attempts,
    });
  }

  prioritized.sort((left, right) => right.priority - left.priority);
  prioritized;
}

main();
