/*
Inputs:
  - topic: string
  - maxResults: number

Capabilities:
  - search_docs(query, limit)
  - fetch_doc(url)
*/

async function main() {
  const query = topic.trim().toLowerCase();
  const searchResults = await search_docs(query, maxResults);
  const selected = [];
  const seen = [];

  for (const hit of searchResults) {
    if (!seen.includes(hit.url) && selected.length < maxResults) {
      seen.push(hit.url);
      selected.push(hit);
    }
  }

  const docs = [];
  for (const hit of selected) {
    docs.push(await fetch_doc(hit.url));
  }

  const citations = [];
  const bullets = [];
  for (const hit of selected) {
    const citation = {};
    citation.title = hit.title;
    citation.source = hit.source;
    citation.url = hit.url;
    citations.push(citation);
  }
  for (const doc of docs) {
    for (const section of doc.sections) {
      if (section.includes(query) || section.includes("rollback") || section.includes("latency")) {
        bullets.push(section.trim());
      }
    }
  }

  const output = {};
  output.query = query;
  output.citations = citations;
  output.bullets = bullets;
  output.decision = bullets.length > 0 ? "ready_for_brief" : "needs_more_sources";
  return output;
}

main();
