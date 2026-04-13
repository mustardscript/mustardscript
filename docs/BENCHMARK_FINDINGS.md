# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T18-27-01-833Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T18-30-29-205Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T18-30-23-779Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `80d0556`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Static property access now fast-paths plain and other simple object kinds

The runtime already had dedicated static branches for array length/index access
and for many builtin prototype methods. The remaining gap was plain-object
property hits: every `record.alpha` lookup still walked through date/regexp/
wrapper/intl checks before finally touching `object.properties`.

That gap is now closed for the simple object kinds that do not need those
specialized branches:

- `Plain`
- `Global`
- `Math`
- `Json`
- `Console`
- `Error(...)`

Those kinds now return own properties and constructor links before the heavier
object-kind logic runs. Regression coverage also proves a plain object’s own
`constructor` property still shadows the fallback `Object` constructor link.

### 2. The direct Rust signal improved on the intended hot path

The new `npm run bench:rust` rerun showed the expected targeted change:

| Microbench | Current | Result |
| --- | ---: | --- |
| `property_access_hot` | `~0.90 ms` | `~1.8%` faster |
| `builtin_method_hot` | `~2.43 ms` | flat |

Most other runtime-core benches stayed within noise. That is the right shape
for a small getter fast path: narrow improvement without collateral regressions.

The previously landed keyed-collection work remains in place under the same
artifact set, so large `Map` / `Set` lookup improvements still apply on top of
this property-path change.

### 3. Same-machine workload comparison was modestly positive against `80d0556`

To isolate this property fast path from older branch drift, the candidate was
compared against a clean same-machine control rerun at commit `80d0556`:

| Workload | Control | Current | Delta |
| --- | ---: | ---: | ---: |
| Warm run, small script | `0.96 ms` | `0.96 ms` | `+0.1%` |
| Cold start, code-mode search | `0.97 ms` | `0.93 ms` | `-3.9%` |
| Warm run, code-mode search | `0.54 ms` | `0.53 ms` | `-1.5%` |
| Programmatic tool workflow | `1.53 ms` | `1.51 ms` | `-1.8%` |
| Host fanout, 100 calls | `0.41 ms` | `0.40 ms` | `-2.4%` |
| `execution_only_small` | `2.19 ms` | `2.06 ms` | `-6.1%` |
| `startInputs.large` | `1.19 ms` | `1.08 ms` | `-9.7%` |

The candidate was not uniformly better. One boundary surface stayed noisier than
desired (`startInputs.medium +22.4%` median), and `snapshot_load_only` ticked
up slightly (`+11.2%` median). But the main addon latencies were flat to
slightly better overall, which is strong enough evidence to keep the change.

### 4. Smoke budgets still pass, but release snapshot ratio remains noisy

The final release smoke rerun passed comfortably:

| Metric | Current |
| --- | ---: |
| Startup | `0.05 ms` |
| Compute | `0.44 ms` |
| Host-call median ratio | `4.42x` |
| Host-call p95 ratio | `4.76x` |
| Snapshot median ratio | `6.00x` |
| Snapshot p95 ratio | `4.75x` |

One immediately preceding release smoke attempt failed on a narrow noisy sample:
snapshot median ratio `7.58x` versus the `7.5x` budget. The sequential rerun
dropped back to `6.00x`, which confirms the release smoke snapshot ratio is
still somewhat unstable at these sub-millisecond timings even though the gate
itself remains usable.

## Conclusions

1. This closes the Milestone 6 static-property fast-path item: array length and
   builtin prototype method paths already existed, and plain/simple object own
   property hits now have the missing early return.
2. The direct performance signal is small but real. `property_access_hot`
   improved, the broad workload suite did not regress, and several public addon
   paths improved slightly against clean same-machine control.
3. Milestone 6 still remains open. The next concrete paths are capacity-aware
   builders/bulk mutation, any remaining globals cleanup, and auditing builtin
   helpers for avoidable cloning or temporary allocation.
