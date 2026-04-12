const obj = {
  value: 3,
  add(step) {
    return this.value + step;
  },
};
[obj.add(4), typeof obj.add];
