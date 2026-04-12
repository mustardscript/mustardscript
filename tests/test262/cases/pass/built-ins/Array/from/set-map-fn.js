const result = Array.from(
  new Set([2, 2, 3]),
  function (value, index) {
    return value + index + this.offset;
  },
  { offset: 4 },
);

result;
