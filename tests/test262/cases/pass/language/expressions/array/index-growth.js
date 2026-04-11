const values = [1, 2];
values[values.length] = 4;
({
  first: values[0],
  third: values[2],
  size: values.length,
});
