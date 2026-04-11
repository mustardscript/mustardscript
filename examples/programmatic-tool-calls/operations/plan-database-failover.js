/*
Inputs:
  - cluster: string
  - incidentId: string

Capabilities:
  - load_cluster_topology(cluster) -> {
      primaryRegion: string,
      replicas: [{ region, role, priority }],
      writeTrafficRps: number
    }
  - load_replication_health(cluster) -> {
      replicas: [{ region, lagMs, replaying }],
      stalledReplication: string[]
    }
  - reserve_change_window(cluster) -> { windowId, startsInMinutes }
  - request_operator_approval(payload) -> { approved, approver, ticketId }
  - record_failover_decision(payload) -> { decisionId }
*/

const topology = load_cluster_topology(cluster);
const health = load_replication_health(cluster);

let selectedReplica = null;
for (const replica of health.replicas) {
  if (!replica.replaying) {
    continue;
  }
  if (!selectedReplica || replica.lagMs < selectedReplica.lagMs) {
    selectedReplica = replica;
  }
}

let recommendedAction = "stabilize_primary";
if (
  selectedReplica &&
  selectedReplica.lagMs <= 250 &&
  health.stalledReplication.length === 0 &&
  topology.writeTrafficRps <= 12000
) {
  recommendedAction = "failover";
}

let reservedWindow = null;
if (recommendedAction === "failover") {
  reservedWindow = reserve_change_window(cluster);
}

const approval = request_operator_approval({
  incidentId,
  cluster,
  recommendedAction,
  candidateRegion: selectedReplica ? selectedReplica.region : null,
  reservedWindow,
});

const decision = record_failover_decision({
  incidentId,
  cluster,
  recommendedAction,
  candidateRegion: selectedReplica ? selectedReplica.region : null,
  approval,
  reservedWindow,
});

({
  incidentId,
  cluster,
  primaryRegion: topology.primaryRegion,
  recommendedAction,
  candidateRegion: selectedReplica ? selectedReplica.region : null,
  candidateLagMs: selectedReplica ? selectedReplica.lagMs : null,
  stalledReplication: health.stalledReplication,
  reservedWindow,
  approval,
  decisionId: decision.decisionId,
});
