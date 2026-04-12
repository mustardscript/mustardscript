'use strict';

const { loadNative } = require('./native-loader');
const { createExecutorApi } = require('./lib/executor');
const { JsliteError } = require('./lib/errors');
const { createProgressApi } = require('./lib/progress');
const { createJsliteClass } = require('./lib/runtime');

const native = loadNative();
const { Progress, materializeStep, parseStep } = createProgressApi(native);
const Jslite = createJsliteClass({ native, materializeStep, parseStep });
const { InMemoryJsliteExecutorStore, JsliteExecutor } = createExecutorApi({ Jslite, Progress });

module.exports = {
  InMemoryJsliteExecutorStore,
  JsliteError,
  Jslite,
  JsliteExecutor,
  Progress,
};
