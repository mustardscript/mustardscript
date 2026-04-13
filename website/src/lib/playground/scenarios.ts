import type { JsonValue } from './iframe-protocol'

export interface PlaygroundScenario {
  id: string
  label: string
  description: string
  mustardFilename: string
  vanillaFilename: string
  mustardTemplate: string
  vanillaTemplate: string
  inputs: Record<string, JsonValue>
  context: JsonValue
  helperSources: Record<string, string>
  expectedResult: JsonValue
}

const quoteBuilderContext = {
  accounts: {
    acct_91: { id: 'acct_91', plan: 'starter', name: 'Northwind' },
    acct_12: { id: 'acct_12', plan: 'growth', name: 'Pine Labs' },
  },
  policies: {
    starter: { targetPlan: 'growth', baseDelta: 16, approvalSeatThreshold: 20 },
    growth: { targetPlan: 'enterprise', baseDelta: 38, approvalSeatThreshold: 35 },
  },
} satisfies JsonValue

const searchReducerContext = {
  indexes: {
    mustard: [
      { title: 'MustardScript release notes', score: 0.91 },
      { title: 'Sandbox runtime benchmark recap', score: 0.84 },
      { title: 'Agent workflow patterns', score: 0.64 },
    ],
    runtime: [
      { title: 'Sandbox runtime benchmark recap', score: 0.88 },
      { title: 'Runtime memory limit notes', score: 0.76 },
      { title: 'Capability tracing handbook', score: 0.73 },
    ],
  },
} satisfies JsonValue

const policyCheckContext = {
  regionPolicies: {
    us: { hardLimit: 5000, requiresReview: false },
    eu: { hardLimit: 2500, requiresReview: true },
  },
  accountFlags: {
    acct_91: ['trusted'],
    acct_44: ['watchlist', 'manual-review'],
  },
} satisfies JsonValue

export const playgroundScenarios: PlaygroundScenario[] = [
  {
    id: 'quote-builder',
    label: 'Quote Builder',
    description: 'Compare host capability orchestration for a simple pricing workflow.',
    mustardFilename: 'quote-builder.mustard.js',
    vanillaFilename: 'quote-builder.js',
    inputs: {
      accountId: 'acct_91',
      seats: 25,
    },
    context: quoteBuilderContext,
    helperSources: {
      load_account: `
        const [accountId] = args;
        const account = context.accounts?.[accountId];
        if (!account) {
          throw new Error(\`unknown account: \${accountId}\`);
        }
        return account;
      `,
      lookup_plan_policy: `
        const [plan, seats] = args;
        const policy = context.policies?.[plan];
        if (!policy) {
          throw new Error(\`unknown plan: \${plan}\`);
        }
        return {
          targetPlan: policy.targetPlan,
          monthlyDelta: policy.baseDelta * seats,
          requiresApproval: seats >= policy.approvalSeatThreshold,
        };
      `,
      create_quote: `
        const [quoteInput] = args;
        return {
          quoteId: \`quote-\${quoteInput.accountId}-\${quoteInput.seats}\`,
          monthlyDelta: quoteInput.monthlyDelta,
          targetPlan: quoteInput.targetPlan,
        };
      `,
    },
    mustardTemplate: `const account = load_account(accountId);
const policy = lookup_plan_policy(account.plan, seats);
const quote = create_quote({
  accountId: account.id,
  targetPlan: policy.targetPlan,
  seats,
  monthlyDelta: policy.monthlyDelta,
});

({
  quoteId: quote.quoteId,
  approvalMode: policy.requiresApproval ? "manual" : "automatic",
  monthlyDelta: quote.monthlyDelta,
});`,
    vanillaTemplate: `const account = load_account(accountId);
const policy = lookup_plan_policy(account.plan, seats);
const quote = create_quote({
  accountId: account.id,
  targetPlan: policy.targetPlan,
  seats,
  monthlyDelta: policy.monthlyDelta,
});

return {
  quoteId: quote.quoteId,
  approvalMode: policy.requiresApproval ? "manual" : "automatic",
  monthlyDelta: quote.monthlyDelta,
};`,
    expectedResult: {
      quoteId: 'quote-acct_91-25',
      approvalMode: 'manual',
      monthlyDelta: 400,
    },
  },
  {
    id: 'search-reducer',
    label: 'Search Reducer',
    description: 'Compare how both runtimes fan out to tools and reduce ranked results.',
    mustardFilename: 'search-reducer.mustard.js',
    vanillaFilename: 'search-reducer.js',
    inputs: {
      primaryTerm: 'mustard',
      secondaryTerm: 'runtime',
    },
    context: searchReducerContext,
    helperSources: {
      search_index: `
        const [term] = args;
        return context.indexes?.[term] ?? [];
      `,
    },
    mustardTemplate: `const combined = [];

for (const item of search_index(primaryTerm)) {
  combined.push(item);
}

for (const item of search_index(secondaryTerm)) {
  let seen = false;
  for (const existing of combined) {
    if (existing.title === item.title) {
      seen = true;
    }
  }
  if (!seen) {
    combined.push(item);
  }
}

let best = null;
for (const item of combined) {
  if (best === null || item.score > best.score) {
    best = item;
  }
}

({
  totalHits: combined.length,
  topTitle: best.title,
});`,
    vanillaTemplate: `const combined = [];

for (const item of search_index(primaryTerm)) {
  combined.push(item);
}

for (const item of search_index(secondaryTerm)) {
  let seen = false;
  for (const existing of combined) {
    if (existing.title === item.title) {
      seen = true;
    }
  }
  if (!seen) {
    combined.push(item);
  }
}

let best = null;
for (const item of combined) {
  if (best === null || item.score > best.score) {
    best = item;
  }
}

return {
  totalHits: combined.length,
  topTitle: best.title,
};`,
    expectedResult: {
      totalHits: 5,
      topTitle: 'MustardScript release notes',
    },
  },
  {
    id: 'policy-check',
    label: 'Policy Check',
    description: 'Compare deterministic policy checks with small structured host responses.',
    mustardFilename: 'policy-check.mustard.js',
    vanillaFilename: 'policy-check.js',
    inputs: {
      accountId: 'acct_44',
      region: 'eu',
      amount: 3200,
    },
    context: policyCheckContext,
    helperSources: {
      lookup_region_policy: `
        const [region] = args;
        const policy = context.regionPolicies?.[region];
        if (!policy) {
          throw new Error(\`unknown region: \${region}\`);
        }
        return policy;
      `,
      list_account_flags: `
        const [accountId] = args;
        return context.accountFlags?.[accountId] ?? [];
      `,
    },
    mustardTemplate: `const policy = lookup_region_policy(region);
const flags = list_account_flags(accountId);

let escalated = policy.requiresReview;
for (const flag of flags) {
  if (flag === "manual-review") {
    escalated = true;
  }
}

({
  approved: amount <= policy.hardLimit && !escalated,
  escalated,
  flagCount: flags.length,
});`,
    vanillaTemplate: `const policy = lookup_region_policy(region);
const flags = list_account_flags(accountId);

let escalated = policy.requiresReview;
for (const flag of flags) {
  if (flag === "manual-review") {
    escalated = true;
  }
}

return {
  approved: amount <= policy.hardLimit && !escalated,
  escalated,
  flagCount: flags.length,
};`,
    expectedResult: {
      approved: false,
      escalated: true,
      flagCount: 2,
    },
  },
]

export function getScenarioById(id: string): PlaygroundScenario {
  return playgroundScenarios.find((scenario) => scenario.id === id) ?? playgroundScenarios[0]
}
