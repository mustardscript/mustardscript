const set = new Set(['alpha', 'beta', 'alpha']);

const before = Array.from(set.values());
const removed = set.delete('beta');
const after = Array.from(set.entries());
set.clear();

[before, removed, after, set.size];
