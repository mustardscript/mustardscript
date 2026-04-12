const object = Object.fromEntries([
  ['alpha', 1],
  ['beta', 2],
  ['gamma', 3],
]);

[Object.keys(object), Object.values(object), object.beta, Object.hasOwn(object, 'gamma')];
