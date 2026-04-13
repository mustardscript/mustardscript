# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T18-42-47-043Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T18-46-00-925Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T18-46-01-022Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `61d33d9`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Array and object builder paths now avoid known full-refresh hotspots

This slice does not close the entire Milestone 6 bulk-builder item, but it does
land three concrete runtime wins:

- `Array.length = ...` now applies exact array/accounting deltas instead of a
  full array remeasurement.
- `Object.fromEntries(...)` now updates object accounting by per-entry deltas
  instead of refreshing the whole object after each inserted pair.
- `Array.from(...)` now size-hints the common built-in iterable cases
  (`Array`, `Map`, `Set`, `String`) while keeping the destination array rooted
  on the heap, so common builder paths avoid repeated growth churn.

The same patch also adds capacity hints to promise iterable collection and
plain object literal construction, which are small but related hot-path wins.

### 2. Rust-core benches now cover the new builder hot paths directly

The latest `npm run bench:rust` rerun added and exercised dedicated constructor
and builder benches:

| Microbench | Current |
| --- | ---: |
| `array_from_hot` | `~24.6 ms` |
| `object_from_entries_hot` | `~3.80 ms` |

Those benches are new in this artifact set, so they are useful as a direct
baseline for any follow-on builder work. Existing older runtime-core benches
were otherwise mostly mixed or within noise, so the main keep/revert decision
for this slice came from the end-to-end workload suite below.

### 3. Same-machine workload comparison was mixed, but net positive enough to keep

To isolate the candidate from branch drift, it was compared against a clean
same-machine control rerun at commit `61d33d9`:

| Workload | Control | Current | Delta |
| --- | ---: | ---: | ---: |
| Warm run, small script | `0.96 ms` | `0.96 ms` | `+0.3%` |
| Warm run, code-mode search | `0.54 ms` | `0.55 ms` | `+0.8%` |
| Programmatic tool workflow | `1.53 ms` | `1.55 ms` | `+1.5%` |
| Host fanout, 100 calls | `0.37 ms` | `0.39 ms` | `+3.6%` |
| `execution_only_small` | `2.38 ms` | `2.10 ms` | `-11.9%` |
| `startInputs.medium` | `0.41 ms` | `0.34 ms` | `-17.2%` |
| `startInputs.large` | `1.18 ms` | `1.13 ms` | `-3.9%` |
| `Progress.load_only` | `0.24 ms` | `0.22 ms` | `-8.0%` |

The broader signal is therefore mixed rather than cleanly positive. The public
addon workflow path regressed slightly, and host-fanout stayed a bit worse. But
the direct execution phase and several boundary-only paths improved materially,
which is strong enough evidence to keep this narrower builder/accounting slice
while leaving the full milestone item open.

### 4. Smoke budgets still pass comfortably

Both smoke profiles passed on the first try with the current artifact set.

Release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.02 ms` |
| Compute | `0.19 ms` |
| Host-call median ratio | `4.45x` |
| Host-call p95 ratio | `4.80x` |
| Snapshot median ratio | `7.39x` |
| Snapshot p95 ratio | `5.30x` |

Dev smoke also remained well inside its profile-specific budgets, with startup
`0.11 ms`, compute `1.66 ms`, host-call median ratio `3.74x`, and snapshot
median ratio `9.72x`.

## Conclusions

1. This is worth keeping, but it is only partial Milestone 6 progress. The
   array/object/promise builder paths are better, but `Map`/`Set` constructor
   bulk paths and other append-heavy helpers still need follow-on work.
2. The best measurable win is `execution_only_small -11.9%` on a same-machine
   clean control compare, alongside noticeably better `startInputs.*` boundary
   medians. The largest addon regressions in the same compare stayed smaller
   (`programmatic_tool_workflow +1.5%`, `host_fanout_100 +3.6%`).
3. The next concrete paths inside the still-open action item are `Map` / `Set`
   construction/bulk mutation, broader promise/builder allocation audits, and
   any remaining globals cleanup or builtin clone trimming after that.
