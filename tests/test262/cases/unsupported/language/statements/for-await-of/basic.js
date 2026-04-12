async function run() {
  for await (const value of [1, 2]) {
    value;
  }
}

run();
