const allowedOrigins = new Set(['null', window.location.origin])
const workerTimeoutMs = 1500

const workerSource = `
function sanitizeError(error) {
  if (error instanceof Error) {
    return {
      name: error.name || 'Error',
      message: error.message || 'unknown sandbox error',
    };
  }

  return {
    name: 'Error',
    message: typeof error === 'string' ? error : JSON.stringify(error),
  };
}

function sanitizeValue(value, seen = new WeakSet()) {
  if (value === null) {
    return null;
  }
  if (typeof value === 'boolean' || typeof value === 'string') {
    return value;
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value) || Object.is(value, -0)) {
      if (Number.isNaN(value)) {
        return { $mustard: 'nan' };
      }
      if (value === Infinity) {
        return { $mustard: 'infinity' };
      }
      if (value === -Infinity) {
        return { $mustard: 'neg_infinity' };
      }
      return { $mustard: 'neg_zero' };
    }
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item) => sanitizeValue(item, seen));
  }
  if (typeof value !== 'object') {
    throw new Error(\`unsupported sandbox value type: \${typeof value}\`);
  }
  if (seen.has(value)) {
    throw new Error('cyclic sandbox values are not supported');
  }
  seen.add(value);
  const output = {};
  for (const [key, entry] of Object.entries(value)) {
    if (entry === undefined) {
      output[key] = { $mustard: 'undefined' };
      continue;
    }
    output[key] = sanitizeValue(entry, seen);
  }
  seen.delete(value);
  return output;
}

function buildHelpers(helperSources, context) {
  return Object.fromEntries(
    Object.entries(helperSources).map(([name, source]) => [
      name,
      (...args) => new Function('args', 'context', \`"use strict";\\n\${source}\`)(args, context),
    ]),
  );
}

function executeVanilla(code, helperSources, context, inputs) {
  const helpers = buildHelpers(helperSources, context);
  const helperNames = Object.keys(helpers);
  const inputNames = Object.keys(inputs);

  const runner = new Function(
    ...helperNames,
    ...inputNames,
    \`"use strict";\\n\${code}\`,
  );

  return runner(
    ...helperNames.map((name) => helpers[name]),
    ...inputNames.map((name) => inputs[name]),
  );
}

self.onmessage = (event) => {
  const { code, helperSources, context, inputs } = event.data;
  const startedAt = performance.now();
  try {
    const result = executeVanilla(code, helperSources, context, inputs);
    self.postMessage({
      ok: true,
      elapsedMs: performance.now() - startedAt,
      result: sanitizeValue(result),
    });
  } catch (error) {
    self.postMessage({
      ok: false,
      elapsedMs: performance.now() - startedAt,
      error: sanitizeError(error),
    });
  }
};
`

function runInWorker(message, reply) {
  const blob = new Blob([workerSource], { type: 'text/javascript' })
  const workerUrl = URL.createObjectURL(blob)
  const worker = new Worker(workerUrl)

  const timeoutId = window.setTimeout(() => {
    worker.terminate()
    URL.revokeObjectURL(workerUrl)
    reply({
      ok: false,
      elapsedMs: workerTimeoutMs,
      error: {
        name: 'VanillaIframeTimeout',
        message: 'sandboxed iframe execution timed out',
      },
    })
  }, workerTimeoutMs)

  const finish = (response) => {
    window.clearTimeout(timeoutId)
    worker.terminate()
    URL.revokeObjectURL(workerUrl)
    reply(response)
  }

  worker.onmessage = (event) => finish(event.data)
  worker.onerror = (event) =>
    finish({
      ok: false,
      elapsedMs: 0,
      error: {
        name: 'VanillaIframeError',
        message: event.message || 'sandbox worker crashed',
      },
    })

  worker.postMessage({
    code: message.code,
    helperSources: message.helperSources,
    context: message.context,
    inputs: message.inputs,
  })
}

window.addEventListener('message', (event) => {
  if (!allowedOrigins.has(event.origin)) {
    return
  }
  const message = event.data
  if (!message || message.type !== 'playground-run') {
    return
  }

  runInWorker(message, (payload) => {
    event.source?.postMessage(
      {
        type: 'playground-result',
        requestId: message.requestId,
        ...payload,
      },
      '*',
    )
  })
})

window.parent.postMessage({ type: 'playground-ready' }, '*')
