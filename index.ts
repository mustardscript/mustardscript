'use strict';

const { loadNative } = require('./native-loader.ts');
const { createExecutorApi } = require('./lib/executor.ts');
const { JsliteError } = require('./lib/errors.ts');
const { createProgressApi } = require('./lib/progress.ts');
const { createJsliteClass } = require('./lib/runtime.ts');

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
