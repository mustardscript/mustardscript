'use strict';

const fs = require('node:fs');
const path = require('node:path');

function packageRoot(fromDir = __dirname) {
  return path.basename(fromDir) === 'dist' ? path.dirname(fromDir) : fromDir;
}

const PREBUILT_TARGETS = Object.freeze([
  {
    triple: 'x86_64-pc-windows-msvc',
    platform: 'win32',
    arch: 'x64',
    platformArchABI: 'win32-x64-msvc',
    packageName: '@mustardscript/binding-win32-x64-msvc',
    localFile: 'index.win32-x64-msvc.node',
    os: ['win32'],
    cpu: ['x64'],
  },
  {
    triple: 'x86_64-apple-darwin',
    platform: 'darwin',
    arch: 'x64',
    platformArchABI: 'darwin-x64',
    packageName: '@mustardscript/binding-darwin-x64',
    localFile: 'index.darwin-x64.node',
    os: ['darwin'],
    cpu: ['x64'],
  },
  {
    triple: 'aarch64-apple-darwin',
    platform: 'darwin',
    arch: 'arm64',
    platformArchABI: 'darwin-arm64',
    packageName: '@mustardscript/binding-darwin-arm64',
    localFile: 'index.darwin-arm64.node',
    os: ['darwin'],
    cpu: ['arm64'],
  },
  {
    triple: 'x86_64-unknown-linux-gnu',
    platform: 'linux',
    arch: 'x64',
    platformArchABI: 'linux-x64-gnu',
    packageName: '@mustardscript/binding-linux-x64-gnu',
    localFile: 'index.linux-x64-gnu.node',
    os: ['linux'],
    cpu: ['x64'],
    libc: ['glibc'],
  },
]);

const TARGETS_BY_RUNTIME = new Map(
  PREBUILT_TARGETS.map((target) => [`${target.platform}:${target.arch}`, target]),
);

function isExplicitFilePath(specifier) {
  return (
    path.isAbsolute(specifier) ||
    specifier.startsWith(`.${path.sep}`) ||
    specifier.startsWith(`..${path.sep}`) ||
    specifier.startsWith('./') ||
    specifier.startsWith('../')
  );
}

function resolveNativeAddonPath(candidate, label, cwd = process.cwd()) {
  if (typeof candidate !== 'string' || candidate.trim() === '') {
    throw new Error(`${label} must be a non-empty file path to a native .node addon`);
  }
  if (!isExplicitFilePath(candidate)) {
    throw new Error(
      `${label} must be an explicit absolute or relative file path to a native .node addon`,
    );
  }
  const resolved = path.resolve(cwd, candidate);
  if (path.extname(resolved) !== '.node') {
    throw new Error(`${label} must point to a native .node addon`);
  }
  const stats = fs.statSync(resolved, { throwIfNoEntry: false });
  if (!stats?.isFile()) {
    throw new Error(`${label} does not exist: ${resolved}`);
  }
  return resolved;
}

function getCurrentPrebuiltTarget() {
  return TARGETS_BY_RUNTIME.get(`${process.platform}:${process.arch}`) ?? null;
}

function getLocalBuildOutputFile() {
  return getCurrentPrebuiltTarget()?.localFile ?? null;
}

function validatePrebuiltPackageManifest(manifest, target, packageJsonPath) {
  if (manifest?.name !== target.packageName) {
    throw new Error(
      `optional prebuilt package at ${packageJsonPath} does not match ${target.packageName}`,
    );
  }
  if (manifest?.main !== target.localFile) {
    throw new Error(
      `optional prebuilt package ${target.packageName} must expose its native addon as ${target.localFile}`,
    );
  }
}

function resolvePrebuiltPackage(searchRoot = packageRoot()) {
  const target = getCurrentPrebuiltTarget();
  if (!target) {
    return null;
  }

  let packageJsonPath;
  try {
    packageJsonPath = require.resolve(`${target.packageName}/package.json`, {
      paths: [searchRoot],
    });
  } catch {
    return null;
  }

  const packageRoot = path.dirname(packageJsonPath);
  const manifest = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
  validatePrebuiltPackageManifest(manifest, target, packageJsonPath);

  const binaryPath = path.join(packageRoot, target.localFile);
  const stats = fs.statSync(binaryPath, { throwIfNoEntry: false });
  if (!stats?.isFile()) {
    throw new Error(`optional prebuilt package ${target.packageName} is missing ${target.localFile}`);
  }

  return {
    ...target,
    packageJsonPath,
    packageRoot,
    binaryPath,
  };
}

function localBinaryCandidates(searchRoot = packageRoot()) {
  const roots = [
    searchRoot,
    path.join(searchRoot, 'crates', 'mustard-node'),
  ];
  const candidates = [];

  for (const root of roots) {
    if (!fs.existsSync(root)) {
      continue;
    }

    const localFile = getLocalBuildOutputFile();
    if (!localFile) {
      continue;
    }
    for (const filename of [localFile]) {
      const candidate = path.join(root, filename);
      const stats = fs.statSync(candidate, { throwIfNoEntry: false });
      if (stats?.isFile()) {
        candidates.push(candidate);
      }
    }
  }

  return candidates;
}

function loadNative(options = {}) {
  const env = options.env ?? process.env;
  const searchRoot = options.searchRoot ?? packageRoot();
  const overrideCwd = options.overrideCwd ?? process.cwd();
  const loadErrors = [];
  const overridePath =
    env.MUSTARDSCRIPT_NATIVE_LIBRARY_PATH ??
    env.MUSTARD_NATIVE_LIBRARY_PATH ??
    env.NAPI_RS_NATIVE_LIBRARY_PATH;
  if (overridePath) {
    try {
      return require(resolveNativeAddonPath(overridePath, 'native library override', overrideCwd));
    } catch (error) {
      loadErrors.push(error);
    }
  }

  for (const candidate of localBinaryCandidates(searchRoot)) {
    try {
      return require(candidate);
    } catch (error) {
      loadErrors.push(error);
    }
  }

  try {
    const prebuilt = resolvePrebuiltPackage(searchRoot);
    if (prebuilt) {
      try {
        return require(prebuilt.binaryPath);
      } catch (error) {
        loadErrors.push(error);
      }
    }
  } catch (error) {
    loadErrors.push(error);
  }

  const target = getCurrentPrebuiltTarget();
  const platformHint = target
    ? `${target.platformArchABI} via ${target.packageName}`
    : `${process.platform}-${process.arch}`;
  throw new AggregateError(
    loadErrors,
    `Unable to locate a MustardScript native addon for ${platformHint}. Install a matching prebuilt package for this platform.`,
  );
}

module.exports = {
  PREBUILT_TARGETS,
  getCurrentPrebuiltTarget,
  getLocalBuildOutputFile,
  localBinaryCandidates,
  resolveNativeAddonPath,
  resolvePrebuiltPackage,
  loadNative,
};
