const error = new TypeError('boom');
({
  name: error.name,
  message: error.message,
});
