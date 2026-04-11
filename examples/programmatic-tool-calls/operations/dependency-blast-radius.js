/*
Inputs:
  - dependency: string

Capabilities:
  - list_service_dependencies()
  - list_recent_pages()
*/

async function main() {
  const graph = await list_service_dependencies();
  const pages = await list_recent_pages();
  const impacted = [];

  for (const row of graph) {
    if (row.dependencies.includes(dependency)) {
      const item = {};
      item.service = row.service;
      item.pagedRecently = false;
      for (const page of pages) {
        if (page.service === row.service) {
          item.pagedRecently = true;
        }
      }
      impacted.push(item);
    }
  }

  const output = {};
  output.dependency = dependency;
  output.impacted = impacted;
  return output;
}

main();
