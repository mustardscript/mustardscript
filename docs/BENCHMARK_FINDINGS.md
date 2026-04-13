# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T18-10-20-254Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T18-20-21-238Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T18-20-23-929Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `ecc7269`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Keyed collections now use tombstoned slots plus promoted hashed lookup caches

`Map` and `Set` no longer stay purely vector-backed for all sizes. The runtime
now keeps order-preserving slot arrays with tombstones on delete, promotes large
collections onto `IndexMap` lookup caches once they cross the live-entry
threshold, and tracks per-collection clear epochs so iterator invalidation no
longer scans the live iterator set on `delete()` or `clear()`.

Regression coverage now also includes snapshot/resume of active keyed-collection
iterators across `clear()` and reinsertion, which is the new failure mode this
representation had to preserve.

### 2. Large keyed-collection Rust microbenches improved materially

Relative to the large-collection baseline added at `97edb31`, the new
`npm run bench:rust` medians improved as follows:

| Microbench | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| `map_get_large` | `~16.6 ms` | `~5.95 ms` | `-64%` |
| `map_has_large` | `~17.1 ms` | `~6.35 ms` | `-63%` |
| `map_set_large` | `~10.2 ms` | `~6.51 ms` | `-36%` |
| `set_has_large` | `~16.5 ms` | `~5.66 ms` | `-66%` |
| `set_add_large` | `~5.1 ms` | `~3.28 ms` | `-36%` |
| `set_delete_large` | `~5.95 ms` | `~5.00 ms` | `-16%` |
| `iterator_throughput_large` | `~41.6 ms` | `~41.5 ms` | flat |

The strongest direct win is the intended one: large keyed-collection lookups are
now much cheaper. Update paths improved too, but not yet enough to satisfy the
full Milestone 6 stretch target.

### 3. Broader addon workloads were roughly neutral on a same-machine control rerun

Comparing the landed candidate against a clean same-machine `ecc7269` control
rerun isolates this keyed-collection change from older branch-level drift. The
result is mostly neutral rather than dramatically positive:

| Workload | Control | Current | Delta |
| --- | ---: | ---: | ---: |
| Warm run, small script | `0.95 ms` | `0.96 ms` | `+0.6%` |
| Warm run, code-mode search | `0.52 ms` | `0.54 ms` | `+3.2%` |
| Programmatic tool workflow | `1.50 ms` | `1.52 ms` | `+1.3%` |
| Host fanout, 100 calls | `0.42 ms` | `0.39 ms` | `-7.8%` |
| `execution_only_small` | `2.12 ms` | `2.38 ms` | `+12.0%` |
| `startInputs.small` | `0.17 ms` | `0.15 ms` | `-9.6%` |

That same control comparison suggests the large keyed-collection win is real,
but it does not yet translate into broad addon-latency wins across the public
workload suite. The tracked checked-in workload baseline from
`2026-04-13T16-35-07-798Z` looks much worse on many addon boundary and phase
metrics, which indicates older branch drift and benchmark noise are still mixed
into the raw before/after picture.

### 4. Smoke budgets still pass and stayed flat to slightly better

Relative to the previous checked-in release smoke artifact
`benchmarks/results/2026-04-13T16-34-56-941Z-smoke-release.json`, the latest
release smoke run stayed within budget and mostly improved:

| Surface | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| Startup median | `0.06 ms` | `0.06 ms` | `-3.4%` |
| Compute median | `0.44 ms` | `0.44 ms` | `-0.9%` |
| Host-call median | `0.25 ms` | `0.24 ms` | `-4.9%` |
| Snapshot direct median | `0.07 ms` | `0.07 ms` | `-7.5%` |
| Snapshot round-trip median | `0.45 ms` | `0.45 ms` | `+1.4%` |

Current release smoke medians remain comfortably inside the configured budgets:

| Metric | Current |
| --- | ---: |
| Startup | `0.06 ms` |
| Compute | `0.44 ms` |
| Host-call median ratio | `4.33x` |
| Host-call p95 ratio | `4.73x` |
| Snapshot median ratio | `6.84x` |
| Snapshot p95 ratio | `5.73x` |

## Conclusions

1. This lands the first two Milestone 6 internal collection items: hashed
   lookup/update paths for large `Map` / `Set` workloads, and iterator
   invalidation that no longer repairs live iterator indices by scanning the
   iterator set.
2. The performance signal is good but narrow. Large keyed-collection lookups
   improved by about `2.7x` to `2.9x`, update paths improved by about `1.2x`
   to `1.6x`, and iterator throughput stayed flat.
3. The milestone itself remains open because the broader addon workload suite is
   only roughly neutral so far, and the plan’s `5x` large-collection target is
   not yet met across both membership and update paths.
4. The next high-leverage Milestone 6 paths remain static-property fast paths,
   capacity-aware builders/bulk mutation, the remaining globals cleanup, and
   builtin allocation/cloning audits.
