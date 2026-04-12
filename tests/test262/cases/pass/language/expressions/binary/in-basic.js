const array = [7];
const map = new Map();
const set = new Set();
const regex = /a/g;

[
  "alpha" in { alpha: undefined },
  "missing" in { alpha: 1 },
  0 in array,
  "length" in array,
  "size" in map,
  "add" in set,
  "exec" in regex,
  "from" in Array,
];
