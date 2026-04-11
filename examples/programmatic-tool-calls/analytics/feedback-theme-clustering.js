/*
Inputs:
  - productArea: string

Capabilities:
  - search_feedback_threads(productArea)
  - fetch_feedback_thread(threadId)
*/

async function main() {
  const hits = await search_feedback_threads(productArea);
  const threads = [];
  for (const hit of hits) {
    threads.push(await fetch_feedback_thread(hit.id));
  }

  threads.flatMap((thread) => thread.tags);
}

main();
