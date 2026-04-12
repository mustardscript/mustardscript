# Use Case Gaps 2

This file is a pruned second-pass gap audit for the `9cbcca8` gallery-era
runtime state.

Unlike [USE_CAP_GAPS_2.md](USE_CAP_GAPS_2.md), this document does not try to
catalog every plausible programmatic tool-call workload. It keeps only the
synthetic example ideas that would likely:

- fail against the current runtime
- catch an existing correctness gap
- or hit an explicit fail-closed product boundary that matters for realistic
  tool-calling workloads

## Method

Second-pass generation on April 11, 2026:

- raw synthetic ideas generated: `1080`
- retained after pruning: `127`
- pruned as already supported or too weakly justified: `953`

Generation passes:

- `gpt-5.4` with `xhigh`: finance, treasury, insurance, pricing, lending,
  payments, market risk
- `gpt-5.4-mini` with `high`: operations, SRE, platform, infra, observability,
  incident management
- `gpt-5.3-codex` with `high`: compliance, security, legal, privacy,
  governance, healthcare ops, public-sector workflows
- `gpt-5.3-codex` with `medium`: support, sales ops, revops, finance ops,
  procurement backoffice, account management
- `gpt-5.2` with `high`: supply chain, logistics, marketplace, trust and
  safety, fraud operations
- `gpt-5.4-mini` with `medium`: developer productivity, CI, release,
  analytics engineering, research workflows, data platform operations

Adversarial review:

- one critique pass was discarded because it mostly suggested helpers already
  documented as supported
- one host-boundary critique surfaced plausible security-sensitive questions;
  only the parts supported by direct repo evidence are promoted below

Pruning baseline:

- keep ideas only when they map to a known failing gallery pattern, a
  documented unsupported surface, or a high-confidence correctness boundary
- prune ideas that fit the already supported surface in `README.md`,
  `docs/LANGUAGE.md`, `docs/HOST_API.md`, and `docs/SERIALIZATION.md`

## What Was Pruned

The bulk of the 1,080 raw ideas were removed because they fit the current
documented surface. The discarded examples mostly relied on:

- normal array pipelines with supported helpers such as `map`, `filter`,
  `reduce`, `find`, `some`, `every`, `join`, `slice`, and `includes`
- supported string helpers such as `trim`, `split`, `replace`, `replaceAll`,
  `includes`, `startsWith`, `endsWith`, and case normalization
- supported object inspection helpers such as `Object.keys`,
  `Object.values`, `Object.entries`, and `Object.hasOwn`
- supported guest `Map` and `Set` usage entirely inside guest execution
- supported promise combinators and promise instance methods when they did not
  rely on unsupported constructors or broken callback-suspension paths

That pruning matters because it means the retained set is not a wish list. It
is concentrated pressure on the places where realistic workloads still fall off
the documented surface.

## Gap Taxonomy

### A. Confirmed Runtime and Correctness Gaps

These clusters recur across many domain sweeps and overlap directly with the
already failing gallery in [USE_CASE_GAPS.md](USE_CASE_GAPS.md).

| Gap family | Retained examples | Why it matters |
| --- | ---: | --- |
| Comparator-based `Array.prototype.sort` | `14` | Ranking, prioritization, and tie-breaking are core to realistic decision workloads. |
| `Array.from` on supported iterables | `9` | Realistic code constantly converts `Set`s, iterator helpers, and `Map` keys into arrays for downstream reduction. |
| `Object.fromEntries` | `9` | Tool-driven code routinely pivots `[key, value]` tuples into plain objects. |
| `String.prototype.matchAll` | `9` | Multi-match extraction is common for logs, ticket refs, tracking numbers, clauses, and citation scans. |
| `Date` and wall-clock math | `13` | SLA math, freshness checks, cutoff windows, aging buckets, and escalation timing are everywhere. |
| Iterable-surface correctness | `10` | Realistic loops still pressure `Map`, `Set`, iterator helpers, and custom-iterable rejection behavior. |
| Async host calls inside callback helpers | `12` | `Promise.all(items.map(async ...))`, async `reduce`, and async predicates are natural tool-calling shapes. |
| Start/resume correctness and idempotence | `12` | Resumable workflows are a primary target use case, so state replay or skipped work is high impact. |
| Validator/compiler instability | `8` | Internal validation inconsistencies are unacceptable user-facing failure modes for realistic programs. |

Representative retained ideas:

- Comparator sort:
  `guard-payments-rollout`, carrier quote ranking, incident queue ordering,
  procurement bid ranking, CI flake ranking, and support-SLA breach
  prioritization.
- `Array.from`:
  fraud/sanctions `Set` materialization, blocked-SKU review queues, `Map.keys()`
  expansion for batch capture, and deduped tag/worklist generation.
- `Object.fromEntries`:
  approval routing tables, evidence matrices, attribute payload shaping,
  header construction, and analytics dimension projection.
- `matchAll`:
  incident token extraction, CFR citation scans, tracking number parsing,
  discount-code extraction, and changelog backlink auditing.
- Date/time:
  deployment-freeze windows, ticket/SLA breach timing, invoice aging, last-24h
  fraud windows, travel hold expiry, and quarter-boundary routing.
- Iterable correctness:
  `for...of` over host-returned `Map` data, `Set`-driven dedupe loops,
  pseudo-iterables, custom-iterator rejection, and iterator helper plumbing.
- Async callbacks:
  `Promise.all(records.map(async ... hostCall ...))`, async `reduce`, async
  `filter`, and async `some` / `find` patterns.
- Resume correctness:
  checkpoint cursors, approval chains, parallel fan-out recovery, retry after
  transient host errors, idempotent multi-step booking, and `try` / `finally`
  cleanup on resume.

### B. High-Confidence Fail-Closed Product Gaps

These clusters were not the dominant failures in the original 24-case gallery,
but they recurred enough across the 1,080 raw ideas to be treated as real
coverage gaps for realistic tool-calling workloads.

| Gap family | Retained examples | Why it matters |
| --- | ---: | --- |
| `BigInt` and exact-integer money math | `5` | Large settlement, reserve, and ledger workloads routinely exceed safe-number precision. |
| `Intl` formatting and locale-aware presentation | `4` | Cross-border finance, policy notices, and customer-facing ops often need locale-correct formatting. |
| Typed arrays, buffers, and binary payloads | `5` | Attachments, banking files, digest verification, EDI, and network payloads are naturally byte-oriented. |
| Non-JSON host boundary shapes | `6` | Real SDKs often return `Map`, `Set`, class instances, cyclic graphs, accessors, or native `Date`s. |
| Async iteration and generators | `3` | Streaming feeds, paged cursors, and lazy schedule generation naturally want `for await...of` or `yield`. |
| Explicit syntax-surface rejections | `6` | Object spread, default params, default destructuring, and `delete` are common ergonomic shortcuts in ordinary JS. |
| `JSON.stringify` ordering divergence | `1` | Canonicalization and signing workflows can break if guest JSON order differs from ordinary JS insertion order. |
| `Proxy`-based meta-programming | `1` | Some SDK-style routing and configuration patterns assume `Proxy`, which remains an explicit non-goal. |

Representative retained ideas:

- `BigInt`:
  sovereign-scale treasury sweeps in cents, repo haircuts on 18-digit notionals,
  nano-unit fee netting, and micro-cent reserve rollups.
- `Intl`:
  multi-locale premium notices, accounting-style negative currency rendering,
  and localized loan quote text.
- Typed/binary:
  ISO 20022 emitters, ISO 8583 parsers, PDF/image digest verification, and EDI
  remittance parsing.
- Non-JSON host shapes:
  host capabilities returning `Map`, `Set`, class-based `Money` objects,
  cyclic fraud graphs, or payloads containing native `Date` fields.
- Async iteration/generators:
  streaming market ticks, paged alert streams, and generator-driven amortization
  schedules.
- Explicit syntax rejections:
  pricing/config merges with object spread, helpers that rely on default
  parameters, and field scrubbers built around `delete`.

### C. Product-Fit Gaps Found During Doc Review

These did not recur heavily in the generated example residue because the
subagents were optimizing for realistic bounded guest programs, not for typed
SDK ergonomics. They still matter for the use-case story and should stay
visible.

| Gap family | Evidence | Why it matters |
| --- | --- | --- |
| No guest-side modules or generated typed SDK imports | `README.md`, `docs/LANGUAGE.md`, `USE_CASE_EXAMPLES.md` | A major code-mode pattern is `import`-based SDK use rather than only global capabilities. |
| No dynamic capability injection mid-execution | `USE_CASE_EXAMPLES.md` callout | Some realistic orchestration patterns want host-provided capability objects or phased tool availability. |
| No `new Promise(...)` or general thenable adoption | `README.md`, `docs/LANGUAGE.md` | Guest wrappers around callback-style or adapter-style async flows cannot use the full Promise constructor surface. |

Representative examples:

- `import { billing } from "@host/sdk"; const invoice = await billing.lookup(id);`
- `const step2 = await getScopedClient(accountId); await step2.updateLimits(...);`
- `await new Promise((resolve, reject) => wait_for_approval(resolve, reject));`

These are explicit product boundaries, not accidental omissions.

## Highest-Signal Retained Examples By Cluster

### Comparator Sort

- `Support SLA queue sorted by minutes-to-breach comparator`
- `Carrier Quote Picker Uses Multi-Key Comparator`
- `Risk Priority Ordering with Comparator Sort`

### `Array.from`

- `Convert Host-Returned Set Of Blocked SKUs To Array For Review Tool`
- `RevOps convert Map of stage->count into dashboard rows`
- `OIG Exclusion Feed Materialization via Array.from`

### `Object.fromEntries`

- `Build Request Headers From Tuple Pairs For Label Purchase`
- `Customer success flatten health-signal pairs into object payload`
- `Policy Control Map Flatten with Object.fromEntries`

### `matchAll`

- `Parse Multiple Tracking Numbers From Email Body For Auto-Reconcile`
- `Support parse multi-issue IDs from long email thread`
- `CFR Citation Extraction with matchAll`

### Date and Clock Surfaces

- `Travel Reprice Window Enforces "Hold Expires At"`
- `Compute a maintenance window that crosses midnight and a DST shift`
- `HIPAA Breach Clock Using Date Objects`

### Async Host Calls In Callback Helpers

- `Promise.all Over Async map Calling carrier.quote Per Package`
- `Support bulk enrich tickets with async host call inside map`
- `KYC Filter with Async Sanctions Lookup Callback`

### Resume and Snapshot Continuity

- `Multi-Step Booking: Reserve Inventory Then Capture Payment Then Confirm`
- `Customer support workflow pauses after first page then resumes with cursor`
- `Start/Resume Duplicate Filing Guard`

### Exact Integer and Binary Data Boundaries

- `Treasury sweep allocator with sovereign-scale cent balances`
- `ISO 20022 payment-file emitter`
- `Host returns class instance for access decision`

## Not Promoted To The Main Taxonomy

One critique pass suggested additional gaps around helper coverage for ordinary
array pipelines, string normalization, object inspection, and promise chaining.
Those were not promoted because the current docs already mark those helpers as
supported.

Examples intentionally not promoted:

- `alerts.filter(...).map(...).join(",")`
- `note.body.toLowerCase().replaceAll(...).trim()`
- `Promise.all(tasks).then(...).catch(...).finally(...)`
- `Object.keys(summary).filter(...)`

Those ideas were correctly pruned.

## Open Questions Requiring Direct Verification

The repo evidence was not strong enough to promote these into the main gap list
without overclaiming, but they are worth a direct security review:

- whether `Progress.load(...)` trusting caller-supplied `capability`, `args`,
  or `token` can be abused by a host that dispatches on dumped metadata before
  validating against the actual snapshot
- whether persisted snapshot bytes can be replayed or policy-swapped in a way
  that changes capability authority or effective runtime limits

These are not counted in the retained-gap totals above.

## Confidence

After 1,080 raw ideas and a follow-up critique pass, the gap picture looks
stable:

- most realistic new workloads collapse into a small set of repeated runtime
  gaps rather than revealing many brand-new ones
- the strongest unresolved issues are still comparator sorting, iterable and
  collection normalization helpers, wall-clock and localization support,
  async-callback suspension behavior, resume integrity, validator stability, and
  the narrow JSON-like host boundary
- the biggest product-fit boundaries beyond correctness are exact-integer math,
  binary payload handling, guest-side SDK/module ergonomics, and the lack of
  streaming/generator surfaces

If future work adds new executable gallery cases, prefer examples that pressure
these clusters first instead of adding more coverage-only variants of already
supported helper usage.
