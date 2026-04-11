/*
Inputs:
  - requestId: string
  - customerId: string
  - deadlineIso: string
  - jurisdictions: string[]

Capabilities:
  - fetch_privacy_request(requestId) -> { id, type, subjectEmail, requestedDataClasses }
  - list_systems_of_record(customerId) -> [{ system, recordType, region, dataClasses }]
  - fetch_retention_exceptions(requestId) -> [{ system, recordType, reason, expiresInDays }]
  - queue_erasure_job(payload) -> { jobId, system, state }
  - record_case_event(payload) -> { eventId }
  - finalize_request(payload) -> { caseId, finalState }
*/

function findRetentionException(system, exceptions) {
  for (const exception of exceptions) {
    if (
      exception.system === system.system ||
      exception.recordType === system.recordType
    ) {
      return exception;
    }
  }
  return null;
}

const request = fetch_privacy_request(requestId);
const systems = list_systems_of_record(customerId);
const exceptions = fetch_retention_exceptions(requestId);

const queuedJobs = [];
const blockedSystems = [];
const eventIds = [];

for (const system of systems) {
  const retentionException = findRetentionException(system, exceptions);
  if (retentionException) {
    blockedSystems.push({
      system: system.system,
      recordType: system.recordType,
      reason: retentionException.reason,
      expiresInDays: retentionException.expiresInDays,
    });
    continue;
  }

  const job = queue_erasure_job({
    requestId,
    customerId,
    system: system.system,
    recordType: system.recordType,
    region: system.region,
    jurisdictions,
    requestedDataClasses: request.requestedDataClasses,
    deadlineIso,
  });

  queuedJobs.push({
    system: system.system,
    recordType: system.recordType,
    jobId: job.jobId,
    state: job.state,
  });

  const event = record_case_event({
    requestId,
    message:
      "Queued erasure job " +
      job.jobId +
      " for " +
      system.system +
      " / " +
      system.recordType,
  });
  eventIds.push(event.eventId);
}

const finalState = finalize_request({
  requestId,
  customerId,
  queuedJobCount: queuedJobs.length,
  blockedSystemCount: blockedSystems.length,
  deadlineIso,
});

const output = {};
output.requestId = request.id;
output.customerId = customerId;
output.subjectEmail = request.subjectEmail;
output.deadlineIso = deadlineIso;
output.caseId = finalState.caseId;
output.finalState = finalState.finalState;
output.queuedJobs = queuedJobs;
output.blockedSystems = blockedSystems;
output.eventIds = eventIds;

output;
