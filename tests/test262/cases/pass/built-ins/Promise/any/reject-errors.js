async function main() {
  try {
    await Promise.any([Promise.reject('alpha'), Promise.reject('beta')]);
    return 'unreachable';
  } catch (error) {
    return [error.name, error.message, error.errors];
  }
}

main();
