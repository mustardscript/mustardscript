async function run() {
  let total = 0;
  for await (const value of [Promise.resolve(1), 2]) {
    total += value;
  }
  return total;
}

run();
