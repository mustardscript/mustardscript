'use strict';

const { loadNative } = require('./native-loader');
const { JsliteError } = require('./lib/errors');
const { createProgressApi } = require('./lib/progress');
const { createJsliteClass } = require('./lib/runtime');

const native = loadNative();
const { Progress, materializeStep, parseStep } = createProgressApi(native);
const Jslite = createJsliteClass({ native, materializeStep, parseStep });

module.exports = {
  JsliteError,
  Jslite,
  Progress,
};
