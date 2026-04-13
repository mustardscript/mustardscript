# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T19-02-16-264Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T19-02-22-622Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T19-02-25-347Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `adb2514`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. `Map` / `Set` constructors now reuse hinted builder slots instead of growing blindly

The previous slice already covered the array/object/promise side of the open
Milestone 6 builder item. This follow-on closes the remaining keyed-collection
gap:

- `new Map(iterable)` and `new Set(iterable)` now size-hint built-in iterables
  and allocate heap-backed builder slots up front.
- `map_set(...)` and `set_add(...)` now reuse those prefilled `None` slots
  during construction instead of always pushing a fresh entry.
- Duplicate-heavy constructor inputs now trim the unused trailing builder holes
  once construction completes, so the final collection shape and cached
  accounting match the real live entries.

Broad verification initially exposed an accounting mismatch on suspended
property tests: reused prefilled slots were charging the final entry bytes
without subtracting the placeholder slot bytes first. That delta bug was fixed
before the artifact set below was generated.

### 2. Rust-core benches now include large constructor baselines directly

The latest `npm run bench:rust` rerun exercised the constructor-heavy
microbenches alongside the earlier builder hot paths:

| Microbench | Current |
| --- | ---: |
| `array_from_hot` | `~25.9 ms` |
| `object_from_entries_hot` | `~3.84 ms` |
| `map_ctor_large` | `~171.6 ms` |
| `set_ctor_large` | `~157.7 ms` |

The constructor benches are new and therefore serve mainly as direct baselines
for any later clone/allocation audit work. The existing keyed-collection lookup
benches remain much faster than their pre-redesign `97edb31` baseline, while
this constructor slice focuses on end-to-end workload improvements rather than
another large lookup jump.

### 3. Same-machine clean-control workload comparison is broadly positive

To isolate the candidate from branch drift, it was compared against a clean
same-machine control rerun at commit `adb2514`:

| Workload | Control | Current | Delta |
| --- | ---: | ---: | ---: |
| Warm run, small script | `0.97 ms` | `0.93 ms` | `-4.2%` |
| Warm run, code-mode search | `0.54 ms` | `0.51 ms` | `-4.6%` |
| Programmatic tool workflow | `1.54 ms` | `1.45 ms` | `-6.0%` |
| Host fanout, 100 calls | `0.39 ms` | `0.39 ms` | `+0.4%` |
| `execution_only_small` | `2.20 ms` | `1.83 ms` | `-16.7%` |
| `startInputs.medium` | `0.38 ms` | `0.28 ms` | `-27.9%` |
| `suspendedArgs.medium` | `0.77 ms` | `0.63 ms` | `-18.3%` |
| `Progress.load_only` | `0.26 ms` | `0.20 ms` | `-21.6%` |

The main negative movement stayed on the suspend/resume latency fixtures
(`suspend_resume_1 +21.2%`, `suspend_resume_5 +8.8%`, `suspend_resume_20 +2.6%`),
but the absolute numbers there remain small, and the broader addon surfaces
that this milestone targets moved materially in the right direction.

### 4. Smoke budgets still pass with room to spare

Both smoke profiles passed on the first try with the final artifact set.

Release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.04 ms` |
| Compute | `0.37 ms` |
| Host-call median ratio | `3.75x` |
| Host-call p95 ratio | `4.20x` |
| Snapshot median ratio | `5.91x` |
| Snapshot p95 ratio | `4.81x` |

Dev smoke also stayed comfortably inside budget, with startup `0.22 ms`,
compute `2.48 ms`, host-call median ratio `3.29x`, and snapshot median ratio
`8.62x`.

## Conclusions

1. The Milestone 6 capacity-aware builder item is now closed. Arrays,
   objects, maps, sets, and promise iterable collection all have explicit
   builder/cached-accounting work landed, and the last open keyed-collection
   constructor path now has both regression coverage and benchmark evidence.
2. The clean-control workload compare is materially better than the previous
   partial slice: `programmatic_tool_workflow -6.0%`,
   `execution_only_small -16.7%`, `startInputs.medium -27.9%`,
   `suspendedArgs.medium -18.3%`, and `Progress.load_only -21.6%`, with
   `host_fanout_100` essentially flat.
3. The next open performance work is no longer about collection builders. The
   remaining concrete paths near this milestone are duplicated globals cleanup
   and builtin clone/temp-allocation audits, while larger future wins still sit
   in the open addon-boundary and sidecar milestones.
