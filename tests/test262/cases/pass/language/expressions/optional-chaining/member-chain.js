const base = { nested: { value: 5 } };
const missing = null;
[
  base?.nested?.value,
  missing?.nested?.value,
  ({ alt: 4 })?.alt,
];
