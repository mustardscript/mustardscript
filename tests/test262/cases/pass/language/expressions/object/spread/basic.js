const extra = [3];
extra.label = "ok";
const obj = {
  alpha: 1,
  ...null,
  ...undefined,
  ...{ beta: 2 },
  ...extra,
};
({
  alpha: obj.alpha,
  beta: obj.beta,
  zero: obj[0],
  label: obj.label,
});
