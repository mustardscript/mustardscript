/*
Inputs:
  - controlFamily: string

Capabilities:
  - search_policy_docs(controlFamily)
  - fetch_policy_doc(docId)
*/

async function main() {
  const hits = await search_policy_docs(controlFamily);
  const statuses = [];

  for (const hit of hits) {
    const doc = await fetch_policy_doc(hit.id);
    statuses.push([doc.controlId, doc.status]);
  }

  Object.fromEntries(statuses);
}

main();
