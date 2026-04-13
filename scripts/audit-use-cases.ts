'use strict';

const fs = require('node:fs');
const path = require('node:path');

const { Jslite, JsliteError, Progress } = require('../index.ts');

const ROOT = path.join(__dirname, '..');
const USE_CASE_ROOT = path.join(ROOT, 'examples/programmatic-tool-calls');

function loadCatalog(relativePath) {
  const catalogPath = path.join(USE_CASE_ROOT, relativePath, 'catalog.ts');
  if (!fs.existsSync(catalogPath)) {
    return [];
  }
  const entries = require(catalogPath);
  return entries.map((entry) => ({
    ...entry,
    category: relativePath,
    absoluteFile: path.join(USE_CASE_ROOT, relativePath, entry.file),
  }));
}

function normalizeError(error) {
  if (error instanceof JsliteError) {
    return {
      kind: error.kind,
      name: error.name,
      message: error.message,
    };
  }
  if (error instanceof Error) {
    return {
      kind: 'Host',
      name: error.name,
      message: error.message,
    };
  }
  return {
    kind: 'Host',
    name: 'Error',
    message: String(error),
  };
}

async function runWithOptions(descriptor, source) {
  const runtime = new Jslite(source);
  const options = {
    ...(descriptor.options ?? {}),
    inputs: descriptor.inputs ?? descriptor.options?.inputs ?? {},
  };
  const value = await runtime.run(options);
  return {
    mode: 'run',
    value,
  };
}

function runWithStartPlan(descriptor, source) {
  const runtime = new Jslite(source);
  const startPlan = descriptor.startPlan;
  let step = runtime.start({
    inputs: descriptor.inputs ?? {},
    capabilities: startPlan.capabilities,
  });
  const suspensions = [];

  for (const payload of startPlan.resumes ?? []) {
    if (!(step instanceof Progress)) {
      throw new Error(
        `Use case ${descriptor.id} completed before exhausting its resume plan`,
      );
    }
    suspensions.push({
      capability: step.capability,
      args: step.args,
    });
    const resumeValue =
      payload &&
      typeof payload === 'object' &&
      !Array.isArray(payload) &&
      'capability' in payload &&
      'value' in payload
        ? payload.value
        : payload;
    step = step.resume(resumeValue);
  }

  if (step instanceof Progress) {
    throw new Error(
      `Use case ${descriptor.id} still suspended after exhausting its resume plan`,
    );
  }

  return {
    mode: 'start_resume',
    value: step,
    suspensions,
  };
}

async function executeDescriptor(descriptor) {
  const source = fs.readFileSync(descriptor.absoluteFile, 'utf8');
  try {
    const result = descriptor.startPlan
      ? runWithStartPlan(descriptor, source)
      : await runWithOptions(descriptor, source);
    return {
      ok: true,
      id: descriptor.id,
      name: descriptor.name,
      category: descriptor.category,
      file: path.relative(ROOT, descriptor.absoluteFile),
      description: descriptor.description,
      mode: result.mode,
      value: result.value,
      suspensions: result.suspensions ?? [],
    };
  } catch (error) {
    return {
      ok: false,
      id: descriptor.id,
      name: descriptor.name,
      category: descriptor.category,
      file: path.relative(ROOT, descriptor.absoluteFile),
      description: descriptor.description,
      error: normalizeError(error),
    };
  }
}

async function main() {
  const descriptors = [
    ...loadCatalog('analytics'),
    ...loadCatalog('operations'),
    ...loadCatalog('workflows'),
  ];

  const results = [];
  for (const descriptor of descriptors) {
    results.push(await executeDescriptor(descriptor));
  }

  const failures = results.filter((result) => !result.ok);
  const summary = {
    total: results.length,
    passed: results.length - failures.length,
    failed: failures.length,
    results,
  };

  if (process.argv.includes('--json')) {
    process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
    return;
  }

  console.log(`Audited ${summary.total} use cases`);
  console.log(`Passed: ${summary.passed}`);
  console.log(`Failed: ${summary.failed}`);
  if (failures.length > 0) {
    console.log('');
    for (const failure of failures) {
      console.log(`[${failure.category}] ${failure.id} -> ${failure.error.kind}: ${failure.error.message}`);
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
