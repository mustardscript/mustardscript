let events = [];

function run(flag) {
  try {
    events[events.length] = 'body';
    if (flag) {
      throw 'boom';
    }
    return 'ok';
  } catch (error) {
    events[events.length] = error;
    return 'caught';
  } finally {
    events[events.length] = 'finally';
  }
}

[
  run(true),
  run(false),
  events,
];
