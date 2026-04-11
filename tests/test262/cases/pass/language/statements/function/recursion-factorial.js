function fact(value) {
  if (value <= 1) {
    return 1;
  }
  return value * fact(value - 1);
}

fact(5);
