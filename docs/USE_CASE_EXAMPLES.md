# Use Case Examples

This file expands the three primary use cases called out in
[`README.md`](../README.md):

- server-side code mode with a compact tool surface
- programmatic multi-tool execution with local reduction
- resumable host-mediated workflows

The examples below follow the recurring guidance from the surveyed vendor docs:

- Prefer code execution when the task needs loops, conditionals, batching, or multi-step tool orchestration; keep one-shot tasks on ordinary tool calls.
- Keep the tool surface compact, explicit, and non-overlapping.
- Return structured, high-signal results and do filtering or aggregation in code before anything goes back to the model.
- Keep auth, network access, and other side effects in host capabilities rather than in guest code.

Surveyed references:

- Anthropic, [Programmatic tool calling](https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling)
- Anthropic, [Writing effective tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)
- Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- Cloudflare, [Code Mode: give agents an entire API in 1,000 tokens](https://blog.cloudflare.com/code-mode-mcp/)
- Cloudflare, [Codemode docs](https://developers.cloudflare.com/agents/api-reference/codemode/)

All snippets assume they are run from a checked-out repository and therefore use `require("./index.js")`. If you are consuming the published package, replace that import with `require("mustardscript")`.

## Example 1: Compact `search()` / `execute()` Code Mode

Type: Server-side code mode against a small explicit capability surface.

Description: This follows the "compact tool surface" pattern from the README and from Code Mode / programmatic tool-calling docs. The guest code searches a narrow host-owned tool catalog, picks the best operation locally, executes it, and returns only the final structured answer.

```js
'use strict';

const { Mustard } = require('./index.js');

async function main() {
  const runtime = new Mustard(`
    const matches = search_api(question);
    let best = null;

    for (const match of matches) {
      if (best === null || match.score > best.score) {
        best = match;
      }
    }

    let finalResult;
    if (best === null) {
      finalResult = {
        status: "no_match",
        answer: "No supported operation matched the request.",
      };
    } else {
      const execution = execute_api({
        operation: best.operation,
        arguments: {
          account_id: accountId,
          invoice_id: invoiceId,
        },
      });

      finalResult = {
        status: "completed",
        chosen_operation: best.operation,
        answer: execution.message,
      };
    }

    finalResult;
  `, {
    inputs: ['question', 'accountId', 'invoiceId'],
  });

  const result = await runtime.run({
    inputs: {
      question: 'Fix a duplicate invoice for INV-100',
      accountId: 'acct_123',
      invoiceId: 'INV-100',
    },
    capabilities: {
      search_api(query) {
        return [
          {
            operation: 'billing.create_credit_note',
            score: 0.94,
            summary: 'Issue a credit note for a duplicate or incorrect invoice.',
          },
          {
            operation: 'billing.resend_invoice',
            score: 0.52,
            summary: 'Resend the latest invoice to the customer.',
          },
        ];
      },
      execute_api(request) {
        if (request.operation !== 'billing.create_credit_note') {
          throw new Error(`unsupported operation: ${request.operation}`);
        }

        return {
          message: `Created credit note for ${request.arguments.invoice_id} on ${request.arguments.account_id}`,
        };
      },
    },
    limits: {
      instructionBudget: 100_000,
      heapLimitBytes: 4 << 20,
    },
  });

  console.log(result);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
```

> Callout: `mustard` supports the compact global capability pattern today, but it does not yet provide first-class guest-side module imports, generated typed SDKs inside guest code, or dynamic capability injection mid-execution. The current best fit is a narrow host-owned surface such as `search_api()` / `execute_api()`.

## Example 2: Programmatic Multi-Tool Reduction

Type: Multi-tool execution where guest code reduces large intermediate results before returning a final answer.

Description: This matches the README's "programmatic tool-calling workloads" use case. The guest code starts several host lookups, awaits them, reduces the raw records locally, and returns only the compact decision payload that a model would actually need.

```js
'use strict';

const { Mustard } = require('./index.js');

async function main() {
  const runtime = new Mustard(`
    async function summarizeAccount() {
      const profilePromise = fetch_profile(accountId);
      const invoicesPromise = fetch_invoices(accountId);
      const usagePromise = fetch_usage(accountId);

      const profile = await profilePromise;
      const invoices = await invoicesPromise;
      const usage = await usagePromise;

      let overdue = 0;
      let totalBalance = 0;
      for (const invoice of invoices) {
        totalBalance = totalBalance + invoice.amount;
        if (invoice.status === "overdue") {
          overdue = overdue + 1;
        }
      }

      return {
        account: profile.name,
        plan: usage.plan,
        overdue_invoices: overdue,
        total_balance: totalBalance,
        needs_human_follow_up: overdue > 1 || usage.seat_utilization < 0.5,
      };
    }

    summarizeAccount();
  `, {
    inputs: ['accountId'],
  });

  const result = await runtime.run({
    inputs: {
      accountId: 'acct_123',
    },
    capabilities: {
      async fetch_profile(id) {
        return {
          id,
          name: 'Acme Corp',
        };
      },
      async fetch_invoices(id) {
        return [
          { amount: 1200, status: 'paid' },
          { amount: 800, status: 'overdue' },
          { amount: 600, status: 'overdue' },
        ];
      },
      async fetch_usage(id) {
        return {
          plan: 'growth',
          seat_utilization: 0.42,
        };
      },
    },
    limits: {
      instructionBudget: 200_000,
      heapLimitBytes: 8 << 20,
      maxOutstandingHostCalls: 8,
    },
  });

  console.log(result);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
```

> Callout: `mustard` supports guest async functions, `await`, queued host calls, and `maxOutstandingHostCalls` today. The current Node wrapper still resumes one suspended capability at a time, so if you need true parallel host I/O, do that fan-out inside a host capability or in the host orchestration layer rather than expecting parallel host handler execution from `run()`.

## Example 3: Durable Pause / Persist / Resume

Type: Resumable workflow with explicit host boundaries and durable snapshots.

Description: This is the README's "resumable host-mediated workflow" pattern. The guest code pauses at an approval boundary, the host persists the `Progress`, reloads it later, and resumes the same execution state without rerunning earlier guest work.

```js
'use strict';

const { Mustard, Progress } = require('./index.js');

async function main() {
  const runtime = new Mustard(`
    const approval = request_approval({
      invoice_id: invoiceId,
      proposed_credit: creditAmount,
    });

    let finalResult;
    if (!approval.approved) {
      finalResult = {
        status: "cancelled",
        reason: approval.reason,
      };
    } else {
      const issued = issue_credit({
        invoice_id: invoiceId,
        amount: creditAmount,
      });

      finalResult = {
        status: "completed",
        credit_id: issued.credit_id,
      };
    }

    finalResult;
  `, {
    inputs: ['invoiceId', 'creditAmount'],
  });

  const firstStep = runtime.start({
    inputs: {
      invoiceId: 'INV-100',
      creditAmount: 125,
    },
    capabilities: {
      request_approval() {},
      issue_credit() {},
    },
  });

  if (!(firstStep instanceof Progress)) {
    console.log(firstStep);
    return;
  }

  console.log('first suspension:', firstStep.capability, firstStep.args);

  const persisted = firstStep.dump();
  // Persist `persisted` in durable storage here.

  const restored = Progress.load(persisted);
  const secondStep = restored.resume({
    approved: true,
    reason: 'manager approved',
  });

  if (!(secondStep instanceof Progress)) {
    console.log(secondStep);
    return;
  }

  console.log('second suspension:', secondStep.capability, secondStep.args);

  const finalResult = secondStep.resume({
    credit_id: 'cn_9001',
  });

  console.log(finalResult);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
```

> Callout: This pattern is supported today through `start()`, `Progress.dump()`, `Progress.load()`, `resume()`, `resumeError()`, and `cancel()`. Snapshot round trips are same-version only, so durable storage should treat snapshots as versioned runtime state rather than as a long-lived cross-version interchange format.
