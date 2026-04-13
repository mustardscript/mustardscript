'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { InMemoryJsliteExecutorStore, Jslite, JsliteError, JsliteExecutor, Progress } =
  require('../../../index.ts');

function runtime(code, options) {
  return new Jslite(code, options);
}

function assertGuestSafeMessage(message) {
  assert.ok(!message.includes(process.cwd()));
  assert.ok(!message.includes('crates/jslite'));
  assert.ok(!message.includes('.rs'));
}

function isJsliteError({ kind, name = kind && `Jslite${kind}Error`, message, guestSafe = false, check } = {}) {
  return (error) => {
    assert.ok(error instanceof JsliteError);
    if (name !== undefined) {
      assert.equal(error.name, name);
    }
    if (kind !== undefined) {
      assert.equal(error.kind, kind);
    }
    if (message !== undefined) {
      if (message instanceof RegExp) {
        assert.match(error.message, message);
      } else {
        assert.ok(error.message.includes(message));
      }
    }
    if (guestSafe) {
      assertGuestSafeMessage(error.message);
    }
    if (typeof check === 'function') {
      check(error);
    }
    return true;
  };
}

module.exports = {
  assert,
  assertGuestSafeMessage,
  InMemoryJsliteExecutorStore,
  isJsliteError,
  Jslite,
  JsliteError,
  JsliteExecutor,
  Progress,
  runtime,
  test,
};
