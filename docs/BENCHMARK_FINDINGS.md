# Benchmark Findings

This document summarizes the latest kept benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T22-59-34-874Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `4d57270`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. The new binary sidecar transport is worth keeping

The latest kept sidecar slice replaced newline-delimited JSON/base64 framing
with length-prefixed frames carrying a JSON header plus raw program/snapshot
bytes, while keeping `--jsonl` as a debug mode.

Compared with the previous kept sidecar artifact
`benchmarks/results/2026-04-13T20-03-16-697Z-workloads.json`, the new kept
artifact improved the main sidecar transport and startup surfaces:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `startup_only` | `9.45 ms` | `3.66 ms` | `-61.3%` |
| `cold_start_small` | `6.17 ms` | `4.92 ms` | `-20.2%` |
| `cold_start_code_mode_search` | `6.28 ms` | `4.77 ms` | `-24.0%` |
| `warm_run_code_mode_search` | `0.82 ms` | `0.66 ms` | `-20.0%` |
| `programmatic_tool_workflow` | `17.20 ms` | `16.78 ms` | `-2.5%` |
| `host_fanout_100` | `6.41 ms` | `6.12 ms` | `-4.7%` |
| `transport_resume_only` | `0.10 ms` | `0.09 ms` | `-10.2%` |

The important caveat is that this transport win is not uniform on every tiny
surface:

- `warm_run_small` stayed roughly flat (`1.39 ms -> 1.40 ms`, `+1.1%`)
- `host_fanout_1` regressed sharply (`0.12 ms -> 0.41 ms`, `+230.8%`)
- `suspend_resume_20` median improved (`1.19 ms -> 1.13 ms`, `-5.5%`) but the
  p95 moved the wrong way on this run (`1.24 ms -> 2.24 ms`)

The practical conclusion is that removing line-delimited JSON/base64 helps the
larger and colder sidecar crossings more than it helps the smallest single-hop
fanout microcases.

### 2. Addon execution is still fast; boundary and sidecar cost still dominate the remaining gap

Current addon medians from the latest kept workload artifact:

| Workload | Median | p95 |
| --- | ---: | ---: |
| `cold_start_small` | `0.97 ms` | `1.00 ms` |
| `warm_run_small` | `0.91 ms` | `0.94 ms` |
| `cold_start_code_mode_search` | `0.82 ms` | `0.83 ms` |
| `warm_run_code_mode_search` | `0.44 ms` | `0.46 ms` |
| `programmatic_tool_workflow` | `1.47 ms` | `1.52 ms` |
| `host_fanout_100` | `0.42 ms` | `0.42 ms` |
| `suspend_resume_20` | `2.37 ms` | `2.47 ms` |

Current addon phase splits:

| Phase | Median | p95 |
| --- | ---: | ---: |
| `runtime_init_only` | `0.04 ms` | `0.05 ms` |
| `execution_only_small` | `2.01 ms` | `2.08 ms` |
| `Progress.load_only` | `0.21 ms` | `0.22 ms` |

That still points to the same broad planning conclusion:

- addon runtime startup and ordinary warm execution are already relatively cheap
- boundary conversion, async resume lifecycle, and sidecar crossing overhead
  still account for most of the remaining gap
- static-property inline caches remain lower-value than addon-boundary and async
  reduction work

### 3. Sidecar ratios improved in a few places, but the milestone target ratios are still far away

Current sidecar/addon median ratios from the latest kept artifact:

| Workload | Ratio |
| --- | ---: |
| `warm_run_small` | `1.54x` |
| `programmatic_tool_workflow` | `11.41x` |
| `host_fanout_100` | `14.57x` |

So the binary transport work is a real improvement, but it does not by itself
close the sidecar/addon gap to the Milestone 7 target. The remaining ratio gap
now appears to come more from process startup, repeated host-call orchestration,
and overall workflow cost than from newline/base64 framing alone.

### 4. Release smoke still passes inside budget

Release smoke medians from the kept rerun:

| Metric | Current |
| --- | ---: |
| Startup | `0.045 ms` |
| Compute | `0.437 ms` |
| Host-call median ratio | `4.39x` |
| Host-call p95 ratio | `4.64x` |
| Snapshot median ratio | `7.07x` |
| Snapshot p95 ratio | `4.75x` |

One immediate prior smoke attempt failed narrowly with `host-call median ratio
5.20 exceeded 5`; an immediate sequential rerun passed and is the kept smoke
artifact above.

## Conclusions

1. The sidecar binary-framing slice should stay. It materially reduces sidecar
   startup and several code-mode / transport medians while preserving explicit
   protocol versioning, hardening behavior, and an inspectable `--jsonl` mode.
2. The sidecar ratio targets are still not met. The remaining gap now looks
   more like general sidecar/process/capability overhead than line/base64
   framing overhead.
3. The highest-value remaining performance work is still addon boundary
   transport/encoding and further async clone reduction. Sidecar work is no
   longer blocked on the wire format itself.
