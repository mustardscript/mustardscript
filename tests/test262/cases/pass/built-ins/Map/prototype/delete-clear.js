const map = new Map([
  ['alpha', 1],
  ['beta', 2],
  ['alpha', 3],
]);

const before = Array.from(map.entries());
const removed = map.delete('beta');
const after = Array.from(map.keys());
map.clear();

[before, removed, after, map.size];
