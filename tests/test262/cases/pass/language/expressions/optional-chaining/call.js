const fn = (value) => value + 1;
const missing = null;
[
  fn?.(2),
  missing?.(2),
];
