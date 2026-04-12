const seen = [];

for (const [key, value] of new Map([
  ['alpha', 1],
  ['beta', 2],
])) {
  seen[seen.length] = key + ':' + value;
}

seen;
