/*
Inputs:
  - cluster: string

Capabilities:
  - list_services(cluster)
  - fetch_capacity_snapshot(serviceId)
*/

async function main() {
  const services = await list_services(cluster);
  const hotspots = [];

  for (const serviceRow of services) {
    const snapshot = await fetch_capacity_snapshot(serviceRow.id);
    if (snapshot.cpu > 0.8 || snapshot.memory > 0.8) {
      const row = {};
      row.serviceId = serviceRow.id;
      row.cpu = snapshot.cpu;
      row.memory = snapshot.memory;
      hotspots.push(row);
    }
  }

  const output = {};
  output.cluster = cluster;
  output.hotspots = hotspots;
  return output;
}

main();
