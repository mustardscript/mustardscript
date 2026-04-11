'use strict';

const fs = require('node:fs');
const path = require('node:path');

const PREBUILT_TARGETS = Object.freeze([
  {
    triple: 'x86_64-pc-windows-msvc',
    platform: 'win32',
    arch: 'x64',
    platformArchABI: 'win32-x64-msvc',
    packageName: '@keppoai/jslite-win32-x64-msvc',
    localFile: 'index.win32-x64-msvc.node',
    os: ['win32'],
    cpu: ['x64'],
  },
  {
    triple: 'x86_64-apple-darwin',
    platform: 'darwin',
    arch: 'x64',
    platformArchABI: 'darwin-x64',
    packageName: '@keppoai/jslite-darwin-x64',
    localFile: 'index.darwin-x64.node',
    os: ['darwin'],
    cpu: ['x64'],
  },
  {
    triple: 'aarch64-apple-darwin',
    platform: 'darwin',
    arch: 'arm64',
    platformArchABI: 'darwin-arm64',
    packageName: '@keppoai/jslite-darwin-arm64',
    localFile: 'index.darwin-arm64.node',
    os: ['darwin'],
    cpu: ['arm64'],
  },
  {
    triple: 'x86_64-unknown-linux-gnu',
    platform: 'linux',
    arch: 'x64',
    platformArchABI: 'linux-x64-gnu',
    packageName: '@keppoai/jslite-linux-x64-gnu',
    localFile: 'index.linux-x64-gnu.node',
    os: ['linux'],
    cpu: ['x64'],
    libc: ['glibc'],
  },
]);

const TARGETS_BY_RUNTIME = new Map(
  PREBUILT_TARGETS.map((target) => [`${target.platform}:${target.arch}`, target]),
);

function getCurrentPrebuiltTarget() {
  return TARGETS_BY_RUNTIME.get(`${process.platform}:${process.arch}`) ?? null;
}

function resolvePrebuiltPackage() {
  const target = getCurrentPrebuiltTarget();
  if (!target) {
    return null;
  }

  try {
    require.resolve(`${target.packageName}/package.json`, { paths: [__dirname] });
    return target;
  } catch {
    return null;
  }
}

function localBinaryCandidates() {
  const target = getCurrentPrebuiltTarget();
  const roots = [
    __dirname,
    path.join(__dirname, 'crates', 'jslite-node'),
  ];
  const candidates = [];
  const seen = new Set();

  for (const root of roots) {
    if (target) {
      const candidate = path.join(root, target.localFile);
      if (fs.existsSync(candidate)) {
        candidates.push(candidate);
        seen.add(candidate);
      }
    }

    if (!fs.existsSync(root)) {
      continue;
    }

    for (const entry of fs.readdirSync(root)) {
      if (!entry.endsWith('.node')) {
        continue;
      }
      const candidate = path.join(root, entry);
      if (seen.has(candidate)) {
        continue;
      }
      candidates.push(candidate);
      seen.add(candidate);
    }
  }

  return candidates;
}

function loadNative() {
  const loadErrors = [];
  const overridePath =
    process.env.JSLITE_NATIVE_LIBRARY_PATH ?? process.env.NAPI_RS_NATIVE_LIBRARY_PATH;
  if (overridePath) {
    try {
      return require(overridePath);
    } catch (error) {
      loadErrors.push(error);
    }
  }

  for (const candidate of localBinaryCandidates()) {
    try {
      return require(candidate);
    } catch (error) {
      loadErrors.push(error);
    }
  }

  const prebuilt = resolvePrebuiltPackage();
  if (prebuilt) {
    try {
      return require(prebuilt.packageName);
    } catch (error) {
      loadErrors.push(error);
    }
  }

  const target = getCurrentPrebuiltTarget();
  const platformHint = target
    ? `${target.platformArchABI} via ${target.packageName}`
    : `${process.platform}-${process.arch}`;
  throw new AggregateError(
    loadErrors,
    `Unable to locate a jslite native addon for ${platformHint}. Install a matching optional prebuilt package or allow the source build to run.`,
  );
}

module.exports = {
  PREBUILT_TARGETS,
  getCurrentPrebuiltTarget,
  resolvePrebuiltPackage,
  loadNative,
};
