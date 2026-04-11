const parsed = JSON.parse('{"a":2,"b":[1,3]}');
Math.max(parsed.a, parsed.b[1]) + Math.abs(-4);
