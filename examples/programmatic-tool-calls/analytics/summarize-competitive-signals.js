/*
Inputs:
  - company: string

Capabilities:
  - search_news(query, limit)
  - fetch_article(url)
*/

async function main() {
  const hits = await search_news(company, 6);
  const articles = [];
  for (const hit of hits) {
    articles.push(await fetch_article(hit.url));
  }

  const launches = [];
  const risks = [];
  for (const article of articles) {
    const text = article.body.toLowerCase();
    if (text.includes("launch") || text.includes("pricing")) {
      launches.push(article.title);
    }
    if (text.includes("outage") || text.includes("regulator")) {
      risks.push(article.title);
    }
  }

  const output = {};
  output.company = company;
  output.articleCount = articles.length;
  output.launchSignals = launches;
  output.riskSignals = risks;
  return output;
}

main();
