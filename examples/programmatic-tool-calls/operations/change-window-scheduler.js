/*
Inputs:
  - region: string

Capabilities:
  - list_change_windows(region)
*/

async function main() {
  const now = Date.now();
  const windows = await list_change_windows(region);
  const available = [];

  for (const window of windows) {
    if (window.start > now) {
      available.push(window);
    }
  }

  available;
}

main();
