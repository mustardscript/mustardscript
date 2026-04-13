# Benchmark Findings

This document summarizes the latest kept benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T22-28-11-723Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T22-28-35-619Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `e90f9a0`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Addon warm and cold runs are now consistently sub-millisecond on the tracked small and code-mode workloads

Current addon medians from the latest kept workload artifact:

| Workload | Median | p95 |
| --- | ---: | ---: |
| `cold_start_small` | `0.96 ms` | `0.99 ms` |
| `warm_run_small` | `0.90 ms` | `0.94 ms` |
| `cold_start_code_mode_search` | `0.86 ms` | `0.89 ms` |
| `warm_run_code_mode_search` | `0.45 ms` | `0.45 ms` |
| `programmatic_tool_workflow` | `1.49 ms` | `1.55 ms` |
| `host_fanout_100` | `0.41 ms` | `0.42 ms` |
| `suspend_resume_20` | `2.36 ms` | `2.49 ms` |

The latest kept compiler/runtime-specialization work is good enough to keep:

- object-heavy code-mode search is already down to `0.45 ms` warm median
- startup and warm small runs remain below `1 ms`
- workflow and suspend/resume paths are still slower than the small/code-mode
  search path, which matters for choosing the next optimization target

### 2. The remaining bottleneck is boundary and sidecar overhead, not static property dispatch

Current sidecar/addon median ratios from the latest kept workload artifact:

| Workload | Ratio |
| --- | ---: |
| `warm_run_small` | `1.53x` |
| `programmatic_tool_workflow` | `11.24x` |
| `host_fanout_100` | `14.93x` |

Current addon phase splits:

| Phase | Median | p95 |
| --- | ---: | ---: |
| `runtime_init_only` | `0.03 ms` | `0.05 ms` |
| `execution_only_small` | `2.18 ms` | `2.25 ms` |
| `Progress.load_only` | `0.24 ms` | `0.26 ms` |

This is the key planning conclusion from the current evidence:

- recent kept Milestone 3 and Milestone 8 work already cut ordinary property
  and object-heavy execution enough that static-property dispatch is no longer
  the dominant remaining cost center
- the larger remaining gaps are addon structured-boundary conversion, async
  suspend/resume lifecycle cost, and sidecar transport/session overhead
- that means monomorphic inline caches for static property reads are not the
  next justified optimization bet

### 3. Boundary surfaces are still mixed, but the main addon medians are stable enough to keep the current compiler slices

Current addon boundary medians:

| Metric | Median |
| --- | ---: |
| `startInputs.small` | `0.09 ms` |
| `startInputs.medium` | `0.34 ms` |
| `startInputs.large` | `1.06 ms` |
| `suspendedArgs.medium` | `0.84 ms` |
| `resumeValues.medium` | `0.31 ms` |
| `resumeErrors.medium` | `0.34 ms` |

The boundary surfaces still have enough variance that small compiler tweaks
need same-machine control reruns before landing. That was true for several
follow-on Milestone 8 experiments after `7b5b387`, which were benchmark-mixed
and reverted instead of being committed.

### 4. Release smoke still passes inside budget

Release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.043 ms` |
| Compute | `0.434 ms` |
| Host-call median ratio | `3.65x` |
| Host-call p95 ratio | `4.18x` |
| Snapshot median ratio | `6.92x` |
| Snapshot p95 ratio | `5.51x` |

## Conclusions

1. The latest kept compiler-specialization slices are worth retaining. They
   improve or hold the main addon medians while preserving validation,
   snapshot compatibility, and fail-closed behavior.
2. Static-property inline caches are not warranted yet. The current benchmark
   evidence points more strongly to async clone amplification, addon boundary
   DTO work, and sidecar wire/session overhead.
3. The next performance priorities remain:
   addon boundary transport/encoding work, further async clone reduction, and
   the still-open sidecar protocol changes.
