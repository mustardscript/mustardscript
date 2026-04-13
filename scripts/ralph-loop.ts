#!/usr/bin/env node
'use strict';

const { spawn } = require('node:child_process');
const fs = require('node:fs/promises');
const path = require('node:path');
const process = require('node:process');

const PLAN_COMPLETED_MARKER = '[PLAN HAS BEEN COMPLETED]';
const PLAN_DONE_AT_MARKER_PREFIX = '[PLAN DONE AT COMMIT ';
const PLAN_BLOCKED_MARKER = '[BLOCKED]';
const DEFAULT_DELAY_MS = 1000;
const DEFAULT_MODEL = 'gpt-5.4';
const DEFAULT_REASONING_EFFORT = 'xhigh';
const STATUS_DONE_PATTERN = /^#{1,6}\s*Status:\s*Done\s*$/im;
const STATUS_BLOCKED_PATTERN = /^#{1,6}\s*Status:\s*Blocked\s*$/im;

const PLAN_PROMPT_PREFIX = `You are implementing the following plan file. Continue executing it until it is truly complete.

Before changing code, read the plan file, inspect the existing implementation, and follow the repository instructions in the current checkout. Verify existing behavior before assuming something is missing. Make concrete implementation progress each iteration instead of stopping at analysis.

When you finish the plan completely:
1. Commit any final verified work if repository instructions require a commit for substantial progress.
2. Run \`git rev-parse --short HEAD\` after the final commit.
3. Add \`${PLAN_COMPLETED_MARKER}\` near the top of the plan file.
4. On the next line add \`${PLAN_DONE_AT_MARKER_PREFIX}<hash>]\`.
5. If the plan has a status line, update it to \`## Status: Done\`.

If the last several iteration-log entries show the same blocker repeatedly with no meaningful progress, add \`${PLAN_BLOCKED_MARKER}\` near the top of the plan file and stop.

If the plan has an iteration log, append the current iteration with a short UTC timestamped summary, commit hash information, and any errors or blockers encountered.

Plan file:`;

class UsageError extends Error {
  constructor(message) {
    super(message);
    this.name = 'UsageError';
  }
}

class CodexNotFoundError extends Error {
  constructor() {
    super(
      'codex CLI was not found on PATH. Install it first, for example with `npm i -g @openai/codex`.',
    );
    this.name = 'CodexNotFoundError';
  }
}

class PlanFileMissingError extends Error {
  constructor(filePath) {
    super(`Plan file does not exist: ${filePath}`);
    this.name = 'PlanFileMissingError';
  }
}

function printUsage(stream = process.stdout) {
  stream.write(
    [
      'Usage: node scripts/ralph-loop.ts <plan.md> [--max-iterations N] [--delay-ms N]',
      '',
      'Runs `codex exec` repeatedly with `gpt-5.4` and `model_reasoning_effort="xhigh"`',
      `until the plan contains ${PLAN_COMPLETED_MARKER} or ${PLAN_BLOCKED_MARKER}.`,
      '',
      'Options:',
      '  --max-iterations N  Stop after N iterations instead of looping indefinitely.',
      `  --delay-ms N        Sleep N milliseconds between iterations (default: ${DEFAULT_DELAY_MS}).`,
      '  --help              Show this help text.',
      '',
      'Example:',
      '  npm run ralph-loop -- plans/my-plan.md --max-iterations 20',
      '',
    ].join('\n'),
  );
}

function parsePositiveInteger(raw, flagName) {
  const value = Number.parseInt(raw, 10);
  if (!Number.isFinite(value) || value < 0) {
    throw new UsageError(`${flagName} must be a non-negative integer, received: ${raw}`);
  }
  return value;
}

function parseArgs(argv) {
  let planPath = null;
  let maxIterations = null;
  let delayMs = DEFAULT_DELAY_MS;

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      return { help: true };
    }
    if (arg === '--max-iterations') {
      index += 1;
      if (index >= argv.length) {
        throw new UsageError('--max-iterations requires a value');
      }
      const parsed = parsePositiveInteger(argv[index], '--max-iterations');
      if (parsed === 0) {
        throw new UsageError('--max-iterations must be at least 1');
      }
      maxIterations = parsed;
      continue;
    }
    if (arg === '--delay-ms') {
      index += 1;
      if (index >= argv.length) {
        throw new UsageError('--delay-ms requires a value');
      }
      delayMs = parsePositiveInteger(argv[index], '--delay-ms');
      continue;
    }
    if (arg.startsWith('-')) {
      throw new UsageError(`Unknown option: ${arg}`);
    }
    if (planPath !== null) {
      throw new UsageError(`Unexpected extra positional argument: ${arg}`);
    }
    planPath = arg;
  }

  if (planPath === null) {
    throw new UsageError('A plan markdown file path is required');
  }

  return {
    help: false,
    planPath,
    maxIterations,
    delayMs,
  };
}

function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function isWithinDirectory(parentPath, candidatePath) {
  const relativePath = path.relative(parentPath, candidatePath);
  return (
    relativePath === '' ||
    (!relativePath.startsWith('..') && !path.isAbsolute(relativePath))
  );
}

function formatPlanDisplayPath(planPath, cwd) {
  return isWithinDirectory(cwd, planPath) ? path.relative(cwd, planPath) : planPath;
}

function extractDoneAtCommit(content) {
  const markerIndex = content.indexOf(PLAN_DONE_AT_MARKER_PREFIX);
  if (markerIndex === -1) {
    return null;
  }

  const hashStart = markerIndex + PLAN_DONE_AT_MARKER_PREFIX.length;
  const hashEnd = content.indexOf(']', hashStart);
  if (hashEnd === -1) {
    return null;
  }
  const hash = content.slice(hashStart, hashEnd).trim();
  return hash.length > 0 ? hash : null;
}

function getPlanStateFromContent(content) {
  return {
    completed:
      content.includes(PLAN_COMPLETED_MARKER) || STATUS_DONE_PATTERN.test(content),
    blocked:
      content.includes(PLAN_BLOCKED_MARKER) || STATUS_BLOCKED_PATTERN.test(content),
    doneAtCommit: extractDoneAtCommit(content),
  };
}

async function readPlanState(planPath) {
  let content;
  try {
    content = await fs.readFile(planPath, 'utf8');
  } catch (error) {
    if (error && error.code === 'ENOENT') {
      throw new PlanFileMissingError(planPath);
    }
    throw error;
  }
  return getPlanStateFromContent(content);
}

function buildPlanPrompt(planPath, iteration, cwd = process.cwd()) {
  return `${PLAN_PROMPT_PREFIX}\n${formatPlanDisplayPath(planPath, cwd)}\n\nThis is iteration ${iteration}.`;
}

function buildCodexExecArgs({
  prompt,
  cwd = process.cwd(),
  planPath,
  model = DEFAULT_MODEL,
  reasoningEffort = DEFAULT_REASONING_EFFORT,
}) {
  const args = [
    'exec',
    '--dangerously-bypass-approvals-and-sandbox',
    '--model',
    model,
    '-c',
    `model_reasoning_effort="${reasoningEffort}"`,
  ];

  const planDirectory = path.dirname(planPath);
  if (!isWithinDirectory(cwd, planDirectory)) {
    args.push('--add-dir', planDirectory);
  }

  args.push(prompt);
  return args;
}

function createSpawnRunner({
  spawnImpl = spawn,
  command = 'codex',
  stderr = process.stderr,
} = {}) {
  return function runCodexIteration({ prompt, cwd, planPath }) {
    const args = buildCodexExecArgs({ prompt, cwd, planPath });
    return new Promise((resolve, reject) => {
      const child = spawnImpl(command, args, {
        cwd,
        stdio: ['inherit', 'inherit', 'inherit'],
      });

      child.on('error', (error) => {
        if (error && error.code === 'ENOENT') {
          reject(new CodexNotFoundError());
          return;
        }
        reject(error);
      });

      child.on('close', (code, signal) => {
        if (signal) {
          stderr.write(`[ralph-loop] codex exited due to signal ${signal}\n`);
          resolve(1);
          return;
        }
        resolve(code ?? 1);
      });
    });
  };
}

async function runLoop({
  planPath,
  cwd = process.cwd(),
  maxIterations = null,
  delayMs = DEFAULT_DELAY_MS,
  runner = createSpawnRunner(),
  logger = console,
  sleepFn = sleep,
}) {
  const resolvedPlanPath = path.resolve(cwd, planPath);
  const displayPath = formatPlanDisplayPath(resolvedPlanPath, cwd);

  let planState = await readPlanState(resolvedPlanPath);
  if (planState.completed) {
    return {
      status: 'completed',
      iterations: 0,
      exitCode: 0,
      planPath: resolvedPlanPath,
      doneAtCommit: planState.doneAtCommit,
    };
  }
  if (planState.blocked) {
    return {
      status: 'blocked',
      iterations: 0,
      exitCode: 1,
      planPath: resolvedPlanPath,
      doneAtCommit: planState.doneAtCommit,
    };
  }

  logger.log(`[ralph-loop] Plan: ${displayPath}`);
  logger.log(
    `[ralph-loop] Model: ${DEFAULT_MODEL} (model_reasoning_effort="${DEFAULT_REASONING_EFFORT}")`,
  );

  let iteration = 1;
  while (maxIterations === null || iteration <= maxIterations) {
    logger.log(`[ralph-loop] Starting iteration ${iteration}`);

    const startedAt = Date.now();
    const prompt = buildPlanPrompt(resolvedPlanPath, iteration, cwd);
    const exitCode = await runner({
      prompt,
      cwd,
      planPath: resolvedPlanPath,
      iteration,
    });
    const durationMs = Date.now() - startedAt;

    logger.log(
      `[ralph-loop] Iteration ${iteration} finished with exit code ${exitCode} after ${durationMs}ms`,
    );

    planState = await readPlanState(resolvedPlanPath);
    if (planState.completed) {
      if (planState.doneAtCommit) {
        logger.log(
          `[ralph-loop] Plan completed at commit ${planState.doneAtCommit} after ${iteration} iteration(s)`,
        );
      } else {
        logger.log(`[ralph-loop] Plan completed after ${iteration} iteration(s)`);
      }
      return {
        status: 'completed',
        iterations: iteration,
        exitCode: 0,
        planPath: resolvedPlanPath,
        doneAtCommit: planState.doneAtCommit,
      };
    }

    if (planState.blocked) {
      logger.error(
        `[ralph-loop] Plan was marked blocked after ${iteration} iteration(s): ${displayPath}`,
      );
      return {
        status: 'blocked',
        iterations: iteration,
        exitCode: 1,
        planPath: resolvedPlanPath,
        doneAtCommit: planState.doneAtCommit,
      };
    }

    if (maxIterations !== null && iteration >= maxIterations) {
      logger.error(
        `[ralph-loop] Reached max iterations (${maxIterations}) before the plan completed`,
      );
      return {
        status: 'max_iterations',
        iterations: iteration,
        exitCode: 1,
        planPath: resolvedPlanPath,
        doneAtCommit: planState.doneAtCommit,
      };
    }

    if (delayMs > 0) {
      logger.log(`[ralph-loop] Sleeping for ${delayMs}ms before the next iteration`);
      await sleepFn(delayMs);
    }

    iteration += 1;
  }

  return {
    status: 'max_iterations',
    iterations: maxIterations ?? 0,
    exitCode: 1,
    planPath: resolvedPlanPath,
    doneAtCommit: planState.doneAtCommit,
  };
}

async function main(argv = process.argv.slice(2)) {
  let args;
  try {
    args = parseArgs(argv);
  } catch (error) {
    if (error instanceof UsageError) {
      process.stderr.write(`${error.message}\n\n`);
      printUsage(process.stderr);
      return 1;
    }
    throw error;
  }

  if (args.help) {
    printUsage();
    return 0;
  }

  try {
    const result = await runLoop({
      planPath: args.planPath,
      maxIterations: args.maxIterations,
      delayMs: args.delayMs,
    });
    return result.exitCode;
  } catch (error) {
    if (
      error instanceof UsageError ||
      error instanceof CodexNotFoundError ||
      error instanceof PlanFileMissingError
    ) {
      process.stderr.write(`${error.message}\n`);
      return 1;
    }
    throw error;
  }
}

if (require.main === module) {
  main()
    .then((exitCode) => {
      process.exitCode = exitCode;
    })
    .catch((error) => {
      const message = error instanceof Error ? error.stack ?? error.message : String(error);
      process.stderr.write(`${message}\n`);
      process.exitCode = 1;
    });
}

module.exports = {
  PLAN_COMPLETED_MARKER,
  PLAN_DONE_AT_MARKER_PREFIX,
  PLAN_BLOCKED_MARKER,
  DEFAULT_DELAY_MS,
  DEFAULT_MODEL,
  DEFAULT_REASONING_EFFORT,
  UsageError,
  CodexNotFoundError,
  PlanFileMissingError,
  printUsage,
  parseArgs,
  extractDoneAtCommit,
  getPlanStateFromContent,
  buildPlanPrompt,
  buildCodexExecArgs,
  createSpawnRunner,
  runLoop,
};
