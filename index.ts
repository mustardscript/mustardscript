'use strict';

const { loadNative } = require('./native-loader.ts');
const { createExecutorApi } = require('./lib/executor.ts');
const { MustardError } = require('./lib/errors.ts');
const { createProgressApi } = require('./lib/progress.ts');
const { createMustardClass } = require('./lib/runtime.ts');

const native = loadNative();
const { Progress, materializeStep, parseStep } = createProgressApi(native);
const Mustard = createMustardClass({ native, materializeStep, parseStep });
const { InMemoryMustardExecutorStore, MustardExecutor } = createExecutorApi({ Mustard, Progress });

module.exports = {
  InMemoryMustardExecutorStore,
  MustardError,
  Mustard,
  MustardExecutor,
  Progress,
};
