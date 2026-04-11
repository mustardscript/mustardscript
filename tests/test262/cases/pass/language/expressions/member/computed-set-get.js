const key = 'value';
const obj = {};
obj[key] = 3;
obj.other = 4;
({
  total: obj[key] + obj.other,
  key: obj[key],
});
