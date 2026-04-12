const values = [1, , 2];
({
  length: values.length,
  keys: Object.keys(values),
  holeIsUndefined: values[1] === undefined,
  hasHole: 1 in values,
  json: JSON.stringify(values),
});
