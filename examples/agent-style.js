'use strict';

const { Jslite, Progress } = require('../index.js');

async function main() {
  const program = new Jslite(`
    const profile = fetch_profile(userId);
    ({
      greeting: "hello",
      profile,
    });
  `);

  const firstStep = program.start({
    inputs: { userId: 7 },
    capabilities: {
      fetch_profile() {},
    },
  });

  if (!(firstStep instanceof Progress)) {
    console.log('completed immediately:', firstStep);
    return;
  }

  const persisted = firstStep.dump();

  // The host can persist `persisted.snapshot`, `persisted.capability`, and
  // `persisted.args` anywhere durable before resuming later.
  const restored = Progress.load(persisted);
  const result = restored.resume({
    id: restored.args[0],
    name: 'Ada',
  });

  console.log('final result:', result);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
