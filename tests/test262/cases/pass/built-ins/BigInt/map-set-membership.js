const left = 2n;
const right = 5n;
const map = new Map([
  [left, 'left'],
  [right, 'right'],
]);
const set = new Set([left, right]);

[String(left + right), map.get(left), set.has(right), typeof left];
