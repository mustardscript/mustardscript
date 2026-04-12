'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const {
  getCurrentPrebuiltTarget,
  localBinaryCandidates,
  loadNative,
  resolvePrebuiltPackage,
} = require('../../native-loader.js');

function withTempDir(prefix, fn) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  try {
    return fn(root);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function writeFile(filePath, contents) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, contents);
}

function aggregateContains(error, snippet) {
  return (
    error instanceof AggregateError &&
    error.errors.some((entry) => entry instanceof Error && entry.message.includes(snippet))
  );
}

test('native loader rejects JavaScript override payloads before execution', () => {
  withTempDir('jslite-loader-override-', (root) => {
    const sentinelPath = path.join(root, 'sentinel.txt');
    const payloadPath = path.join(root, 'payload.js');
    writeFile(
      payloadPath,
      `
        const fs = require('node:fs');
        fs.writeFileSync(${JSON.stringify(sentinelPath)}, 'owned');
        module.exports = {};
      `,
    );

    assert.throws(
      () =>
        loadNative({
          searchRoot: root,
          overrideCwd: root,
          env: {
            JSLITE_NATIVE_LIBRARY_PATH: payloadPath,
          },
        }),
      (error) => aggregateContains(error, 'must point to a native .node addon'),
    );
    assert.equal(fs.existsSync(sentinelPath), false);
  });
});

test(
  'local native fallback only accepts the exact expected local addon filenames',
  {
    skip: !getCurrentPrebuiltTarget(),
  },
  () => {
    withTempDir('jslite-loader-local-', (root) => {
      const target = getCurrentPrebuiltTarget();
      writeFile(path.join(root, 'evil.node'), 'not-a-real-addon');
      writeFile(path.join(root, 'index.node'), 'generic-addon');
      writeFile(path.join(root, target.localFile), 'target-addon');
      writeFile(path.join(root, 'crates', 'jslite-node', 'evil.node'), 'nested-rogue-addon');
      writeFile(
        path.join(root, 'crates', 'jslite-node', target.localFile),
        'nested-target-addon',
      );

      assert.deepEqual(localBinaryCandidates(root), [
        path.join(root, target.localFile),
        path.join(root, 'index.node'),
        path.join(root, 'crates', 'jslite-node', target.localFile),
        path.join(root, 'crates', 'jslite-node', 'index.node'),
      ].filter((candidate) => fs.existsSync(candidate)));
    });
  },
);

test(
  'optional prebuilt resolution rejects JavaScript package fallbacks',
  {
    skip: !getCurrentPrebuiltTarget(),
  },
  () => {
    withTempDir('jslite-loader-prebuilt-', (root) => {
      const target = getCurrentPrebuiltTarget();
      const sentinelPath = path.join(root, 'sentinel.txt');
      const packageRoot = path.join(root, 'node_modules', ...target.packageName.split('/'));
      writeFile(
        path.join(packageRoot, 'package.json'),
        `${JSON.stringify({
          name: target.packageName,
          version: '0.0.0',
          main: 'index.js',
        })}\n`,
      );
      writeFile(
        path.join(packageRoot, 'index.js'),
        `
          const fs = require('node:fs');
          fs.writeFileSync(${JSON.stringify(sentinelPath)}, 'fake-prebuilt-ran');
          module.exports = {};
        `,
      );

      assert.throws(
        () => resolvePrebuiltPackage(root),
        /must expose its native addon as/,
      );
      assert.throws(
        () =>
          loadNative({
            searchRoot: root,
            overrideCwd: root,
            env: {},
          }),
        (error) =>
          aggregateContains(error, 'must expose its native addon as') &&
          error.message.includes('Unable to locate a jslite native addon'),
      );
      assert.equal(fs.existsSync(sentinelPath), false);
    });
  },
);
