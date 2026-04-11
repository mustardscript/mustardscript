/*
Inputs:
  - reviewId: string
  - systemSlug: string
  - ownerEmail: string
  - campaignWindowDays: number

Capabilities:
  - list_access_grants(reviewId, systemSlug) -> [{ grantId, userId, entitlement, scope, grantedDaysAgo, elevationType }]
  - fetch_identity_profiles(userIds) -> [{ userId, managerEmail, workerType, department, lastSeenDaysAgo }]
  - fetch_recent_security_findings(userIds) -> [{ userId, findingId, severity, status }]
  - fetch_exception_register(reviewId) -> [{ grantId, status, approvedBy, expiresInDays }]
  - create_recertification_tasks(payload) -> { taskBatchId, createdCount }
*/

async function runRecertification() {
  const grants = await list_access_grants(reviewId, systemSlug);

  const userIdSet = new Set();
  for (const grant of grants) {
    userIdSet.add(grant.userId);
  }

  const userIds = [];
  for (const userId of userIdSet) {
    userIds.push(userId);
  }

  const settled = await Promise.all([
    fetch_identity_profiles(userIds),
    fetch_recent_security_findings(userIds),
    fetch_exception_register(reviewId),
  ]);

  const profiles = settled[0];
  const findings = settled[1];
  const exceptions = settled[2];

  const profilesByUser = new Map();
  for (const profile of profiles) {
    profilesByUser.set(profile.userId, profile);
  }

  const findingsByUser = new Map();
  for (const finding of findings) {
    let bucket = findingsByUser.get(finding.userId);
    if (!bucket) {
      bucket = [];
      findingsByUser.set(finding.userId, bucket);
    }
    bucket.push(finding);
  }

  const exceptionsByGrant = new Map();
  for (const exception of exceptions) {
    exceptionsByGrant.set(exception.grantId, exception);
  }

  const decisions = [];
  const managerQueues = new Map();
  let revokeCount = 0;
  let reviewCount = 0;
  let keepCount = 0;

  for (const grant of grants) {
    const profile = profilesByUser.get(grant.userId);
    const grantFindings = findingsByUser.get(grant.userId) || [];
    const exception = exceptionsByGrant.get(grant.grantId);

    const reasons = [];
    if (grant.entitlement.includes("admin") || grant.scope.includes("write")) {
      reasons.push("excessive_privilege");
    }
    if (grant.grantedDaysAgo > campaignWindowDays) {
      reasons.push("stale_access");
    }
    if (profile.workerType === "contractor" && grant.elevationType === "persistent") {
      reasons.push("persistent_contractor_access");
    }
    if (profile.lastSeenDaysAgo > 45) {
      reasons.push("identity_not_recently_seen");
    }
    for (const finding of grantFindings) {
      if (finding.status !== "closed" && finding.severity !== "low") {
        reasons.push("open_security_finding");
        break;
      }
    }

    let decision = "keep";
    if (
      reasons.includes("persistent_contractor_access") ||
      reasons.includes("open_security_finding")
    ) {
      decision = "revoke";
    } else if (reasons.length > 0) {
      decision = "manager_review";
    }

    if (exception && exception.status === "approved" && decision === "revoke") {
      decision = "manager_review";
      reasons.push("temporary_exception_in_effect");
    }

    if (decision === "revoke") {
      revokeCount += 1;
    } else if (decision === "manager_review") {
      reviewCount += 1;
    } else {
      keepCount += 1;
    }

    const decisionRecord = {
      grantId: grant.grantId,
      userId: grant.userId,
      managerEmail: profile.managerEmail,
      entitlement: grant.entitlement,
      scope: grant.scope,
      workerType: profile.workerType,
      decision,
      reasons,
      exceptionStatus: exception ? exception.status : null,
    };
    decisions.push(decisionRecord);

    let queue = managerQueues.get(profile.managerEmail);
    if (!queue) {
      queue = [];
      managerQueues.set(profile.managerEmail, queue);
    }
    queue.push(decisionRecord);
  }

  const taskGroups = [];
  for (const entry of managerQueues.entries()) {
    taskGroups.push({
      managerEmail: entry[0],
      grants: entry[1],
    });
  }

  const taskBatch = await create_recertification_tasks({
    reviewId,
    systemSlug,
    ownerEmail,
    taskGroups,
  });

  return {
    reviewId,
    systemSlug,
    ownerEmail,
    taskBatchId: taskBatch.taskBatchId,
    createdTaskCount: taskBatch.createdCount,
    totalGrants: grants.length,
    revokeCount,
    reviewCount,
    keepCount,
    highRiskGrants: decisions.filter((decision) => decision.decision !== "keep"),
  };
}

runRecertification();
