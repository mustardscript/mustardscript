'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');
const manifest = require('../../package.json');
const lockfile = require('../../package-lock.json');

test('release lockfile resolves optional bindings at the release version', () => {
  assert.equal(lockfile.version, manifest.version);
  assert.equal(lockfile.packages[''].version, manifest.version);
  assert.deepEqual(
    lockfile.packages[''].optionalDependencies,
    manifest.optionalDependencies,
  );

  for (const [name, version] of Object.entries(manifest.optionalDependencies)) {
    assert.equal(version, manifest.version, `${name} must match the release`);
    const entry = lockfile.packages[`node_modules/${name}`];
    assert.ok(entry, `${name} needs a resolved lockfile entry`);
    assert.equal(entry.optional, true, `${name} must remain optional`);
    assert.equal(entry.version, version, `${name} has a missing or stale locked version`);
    assert.equal(
      entry.resolved,
      `https://registry.npmjs.org/${name}/-/binding-${name.split('/binding-')[1]}-${version}.tgz`,
      `${name} must resolve to its published release tarball`,
    );
    assert.match(entry.integrity, /^sha512-[A-Za-z0-9+/]+={0,2}$/, `${name} needs tarball integrity`);
  }
});
