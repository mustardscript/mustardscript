/*
Inputs:
  - topic: string
  - maxResults: number

Capabilities:
  - search_docs(query, limit) -> [{ title, url, source, snippet }]
  - fetch_doc(url) -> { url, freshnessHours, sections: string[] }
*/

async function buildResearchBrief() {
  const normalizedTopic = topic.trim().toLowerCase();
  const discovered = await search_docs(normalizedTopic, maxResults);

  const seenUrls = [];
  const selected = [];
  for (const entry of discovered) {
    if (!seenUrls.includes(entry.url) && selected.length < maxResults) {
      seenUrls.push(entry.url);
      selected.push(entry);
    }
  }

  const docRequests = [];
  for (const entry of selected) {
    docRequests.push(fetch_doc(entry.url));
  }
  const docs = await Promise.all(docRequests);

  const citations = [];
  const sourceBreakdown = [];
  const keyPoints = [];
  let totalFreshnessHours = 0;

  for (const entry of selected) {
    const citation = {};
    citation.title = entry.title;
    citation.url = entry.url;
    citation.source = entry.source;
    citation.snippet = entry.snippet.trim();
    citations.push(citation);

    let matchedBucket = null;
    for (const bucket of sourceBreakdown) {
      if (bucket.source === entry.source) {
        matchedBucket = bucket;
      }
    }
    if (matchedBucket) {
      matchedBucket.count += 1;
    } else {
      const bucket = {};
      bucket.source = entry.source;
      bucket.count = 1;
      sourceBreakdown.push(bucket);
    }
  }

  for (const doc of docs) {
    totalFreshnessHours += doc.freshnessHours;
    for (const section of doc.sections) {
      const compact = section.replaceAll("\n", " ").trim();
      if (
        compact.includes(normalizedTopic) ||
        compact.includes("latency") ||
        compact.includes("error budget") ||
        compact.includes("rollback") ||
        compact.includes("saturation")
      ) {
        keyPoints.push(compact);
      }
    }
  }

  const topKeyPoints = [];
  for (const point of keyPoints) {
    if (topKeyPoints.length < 4) {
      topKeyPoints.push(point);
    }
  }

  const output = {};
  output.query = normalizedTopic;
  output.citationCount = citations.length;
  output.citations = citations;
  output.keyPoints = topKeyPoints;
  output.sourceBreakdown = sourceBreakdown;
  output.averageFreshnessHours =
    citations.length === 0 ? null : totalFreshnessHours / citations.length;
  return output;
}

buildResearchBrief();
