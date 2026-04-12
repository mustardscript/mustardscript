function collect(head, ...tail) {
  return [head, tail.length, tail[0], tail[1]];
}

collect(1, 2, 3);
